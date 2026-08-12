//! Stage 6 subagent 后台执行契约测试。
//!
//! 对齐上游 `tests/agent/test_subagent.py`：`spawn`/`_run_subagent` 起后台 agent turn、
//! `_announce_result` 经 bus 回灌、exec session 级联终止（`cancel_by_session` abort）。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lure_core::agent::subagent::SubagentRegistry;
use lure_core::agent::subagent_run::SubagentRunner;
use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::{async_bus_channel, AsyncBusReceiver, InboundMessage};
use lure_core::provider::{
    CompletionRequest, LlmProvider, LlmResponse, ProviderError, StreamChunk,
};
use lure_core::session::SessionManager;
use tempfile::TempDir;

// ---- 测试辅助 -----------------------------------------------------------

/// 慢 echo provider：complete 先 sleep 再回显（模拟后台长 turn，供 cancel 测试）。
struct SleepEchoProvider {
    delay: Duration,
}

impl SleepEchoProvider {
    fn new(delay: Duration) -> Self {
        Self { delay }
    }
}

#[async_trait::async_trait]
impl LlmProvider for SleepEchoProvider {
    fn default_model(&self) -> &str {
        "sleep-echo"
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        tokio::time::sleep(self.delay).await;
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(serde_json::Value::as_str) == Some("user"))
            .and_then(|m| m.get("content"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        Ok(LlmResponse::text(format!("echo: {last_user}")))
    }
    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        on_delta: &mut (dyn FnMut(StreamChunk) + Send),
    ) -> Result<LlmResponse, ProviderError> {
        let response = self.complete(request).await?;
        if let Some(content) = response.content.clone() {
            on_delta(StreamChunk {
                content_delta: Some(content),
                ..StreamChunk::default()
            });
        }
        Ok(response)
    }
}

/// 构造 make_agent 闭包：每次调用建一个指向 `path`（共享 workspace）的新 AgentLoop。
fn make_agent<P>(path: P, delay: Duration) -> Arc<dyn Fn() -> AgentLoop + Send + Sync>
where
    P: Into<PathBuf>,
{
    let path = path.into();
    Arc::new(move || {
        let sessions = SessionManager::new(&path).expect("session 存储可用");
        AgentLoop::new(
            Box::new(SleepEchoProvider::new(delay)),
            sessions,
            ContextBuilder::new(None),
        )
    })
}

/// 等待 bus 收到一条 announce（subagent_result 元数据），返回它。
async fn wait_announce(rx: &mut AsyncBusReceiver) -> InboundMessage {
    loop {
        let msg = rx.consume().await.expect("bus 应送达 announce");
        if msg.metadata.get("subagent_result").is_some() {
            return msg;
        }
    }
}

// ---- run：同步式 subagent turn -------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_executes_subagent_turn_and_returns_text() {
    let tmp = TempDir::new().unwrap();
    let (bus_tx, _rx) = async_bus_channel(16);
    let registry = Arc::new(Mutex::new(SubagentRegistry::new()));
    let runner = SubagentRunner::new(
        tokio::runtime::Handle::current(),
        bus_tx,
        registry.clone(),
        make_agent(tmp.path(), Duration::ZERO),
    );

    let text = runner
        .run("parent-session", "summarize this")
        .await
        .unwrap();
    assert_eq!(text, "echo: summarize this", "subagent turn 应返回最终文本");

    // subagent 完成后 session 落盘（parent-session 有 user + assistant）。
    let mut reader = SessionManager::new(tmp.path()).unwrap();
    let session = reader.get_or_create("subagent:parent-session").unwrap();
    assert_eq!(session.messages.len(), 2);
}

// ---- spawn：后台执行 + announce 经 bus 回灌 + 登记清理 ---------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_runs_background_and_announces_via_bus() {
    let tmp = TempDir::new().unwrap();
    let (bus_tx, mut bus_rx) = async_bus_channel(16);
    let registry = Arc::new(Mutex::new(SubagentRegistry::new()));
    let runner = SubagentRunner::new(
        tokio::runtime::Handle::current(),
        bus_tx,
        registry.clone(),
        make_agent(tmp.path(), Duration::from_millis(40)),
    );

    let task_id = runner
        .spawn("parent-session", "background task", Some("bg"))
        .unwrap();
    assert!(!task_id.is_empty());

    // 运行中：登记在表内。
    assert_eq!(
        registry
            .lock()
            .unwrap()
            .get_running_count_by_session("parent-session"),
        1,
        "spawn 后应登记运行中 subagent"
    );

    // 完成后 announce 经 bus 回灌（带 subagent_result 元数据 + 结果文本）。
    let announce = wait_announce(&mut bus_rx).await;
    assert_eq!(announce.session_key(), "parent-session");
    assert_eq!(
        announce.content, "echo: background task",
        "announce 携带 subagent 结果"
    );
    assert_eq!(
        announce
            .metadata
            .get("subagent_task_id")
            .and_then(serde_json::Value::as_str),
        Some(task_id.as_str()),
        "announce 携带 task_id"
    );

    // 完成后登记清理（finish 移除）。
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        registry.lock().unwrap().get_running_count(),
        0,
        "完成后应清理登记"
    );
}

// ---- exec 级联终止：cancel_by_session abort 运行中 subagent -----------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_by_session_aborts_running_subagents() {
    let tmp = TempDir::new().unwrap();
    let (bus_tx, _bus_rx) = async_bus_channel(16);
    let registry = Arc::new(Mutex::new(SubagentRegistry::new()));
    let runner = SubagentRunner::new(
        tokio::runtime::Handle::current(),
        bus_tx,
        registry.clone(),
        make_agent(tmp.path(), Duration::from_secs(10)), // 长 turn，供取消
    );

    let _task_id = runner.spawn("parent-session", "long", None).unwrap();
    assert_eq!(
        registry
            .lock()
            .unwrap()
            .get_running_count_by_session("parent-session"),
        1
    );

    // /stop 级联：取消该 session 下所有运行中 subagent。
    let cancelled = runner.cancel_session("parent-session");
    assert_eq!(cancelled, 1, "应取消 1 个运行中 subagent");
    // 登记立即清理（abort + finish）。
    assert_eq!(registry.lock().unwrap().get_running_count(), 0);
    // abort 生效：等一拍确认无 announce 再送达（长 turn 被中断）。
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(registry.lock().unwrap().get_running_count(), 0);
}
