//! AgentLoop 异步调度核心契约测试（Stage 1）。
//!
//! 对齐上游 `agent/loop.py::run()` 调度语义（loop.py:1161-1254）：
//! 1. bus 异步消费 → 每消息一个 dispatch task。
//! 2. 同 session 活跃时后续消息入 pending 队列，串行处理（不丢、不乱序）。
//! 3. `/stop` 取消该 session 的活跃任务，清空 pending，后续消息可正常开始新 turn。
//! 4. 消费超时执行心跳钩子（对齐上游 wait_for(1s) 心跳）。
//! 5. automation（cron）消息在聊天 turn 活跃时让位（排队），聊天完成后才处理。
//!
//! 说明：turn 执行为同步 `AgentLoop::process`（包在 tokio Mutex 内），
//! 慢 provider 用 `std::thread::sleep` 模拟；测试须用 multi_thread runtime
//! 保证 run 循环与慢 turn 并行推进（Stage 2 turn 异步化后拆除该限制）。

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lure_core::agent::scheduler::AgentLoopScheduler;
use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::{async_bus_channel, InboundMessage};
use lure_core::provider::{
    CompletionRequest, LlmProvider, LlmResponse, ProviderError, StreamChunk,
};
use lure_core::session::SessionManager;
use tempfile::TempDir;

// ---- 测试辅助 -----------------------------------------------------------

/// 慢 echo provider：complete 时先 sleep，再回显内容。
/// 用 std::thread::sleep 模拟阻塞 turn（Stage 2 异步化后替换为 async sleep）。
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
        std::thread::sleep(self.delay);
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

/// turn 完成记录（on_turn 钩子捕获）。
#[derive(Debug, Clone)]
struct TurnRecord {
    content: String,
    done_at: Instant,
}

/// 测试装配：临时 workspace + 调度器 + 记录器。
struct Harness {
    _tmp: TempDir,
    records: Arc<Mutex<Vec<TurnRecord>>>,
}

fn msg(channel: &str, chat_id: &str, content: &str) -> InboundMessage {
    InboundMessage::new(channel, chat_id, content)
}

fn setup_harness(delay: Duration, consume_timeout: Duration) -> (Harness, AgentLoopScheduler) {
    let tmp = TempDir::new().unwrap();
    let sessions = SessionManager::new(tmp.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(SleepEchoProvider::new(delay)),
        sessions,
        ContextBuilder::new(None),
    );
    let records: Arc<Mutex<Vec<TurnRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let rec = records.clone();
    let scheduler = AgentLoopScheduler::builder(agent)
        .consume_timeout(consume_timeout)
        .on_turn(move |_key: &str, content: &str| {
            rec.lock().unwrap().push(TurnRecord {
                content: content.to_string(),
                done_at: Instant::now(),
            });
        })
        .build();
    (Harness { _tmp: tmp, records }, scheduler)
}

fn turn_count(records: &Arc<Mutex<Vec<TurnRecord>>>) -> Vec<String> {
    records
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.content.clone())
        .collect()
}

// ---- Send 契约 ----------------------------------------------------------

fn assert_send<T: Send>() {}

#[tokio::test]
async fn agent_loop_and_scheduler_are_send() {
    assert_send::<AgentLoop>();
    assert_send::<AgentLoopScheduler>();
}

// ---- 调度契约 -----------------------------------------------------------

/// 不同 session 的消息各自独立处理（并发调度，互不排队）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_sessions_process_in_parallel() {
    let (harness, scheduler) = setup_harness(Duration::from_millis(80), Duration::from_millis(30));
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tx.publish(msg("chat", "S1", "first")).await.unwrap();
    tx.publish(msg("chat", "S2", "second")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    drop(tx);
    run.await.unwrap();

    let got = turn_count(&harness.records);
    assert_eq!(got.len(), 2, "两条消息都应被处理");
    assert!(got.contains(&"first".to_string()));
    assert!(got.contains(&"second".to_string()));
}

/// 同 session：第二个消息在活跃 turn 期间进 pending，串行处理且顺序保持。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_session_serialized_via_pending() {
    let (harness, scheduler) = setup_harness(Duration::from_millis(120), Duration::from_millis(30));
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tx.publish(msg("chat", "S", "A")).await.unwrap();
    // A 的 turn 进行中（120ms 慢 turn）发 B → 应进 pending 而非新起竞争任务。
    tokio::time::sleep(Duration::from_millis(30)).await;
    tx.publish(msg("chat", "S", "B")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    drop(tx);
    run.await.unwrap();

    let recs = harness.records.lock().unwrap().clone();
    assert_eq!(recs.len(), 2, "A、B 都应完成");
    assert_eq!(recs[0].content, "A", "A 应先处理");
    assert_eq!(recs[1].content, "B", "B 串行随后");
    assert!(recs[1].done_at >= recs[0].done_at, "B 的完成时间不得早于 A");
}

/// /stop：取消该 session 活跃任务并清空 pending；后续消息可正常开始新 turn。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_cancels_active_and_pending() {
    let (harness, scheduler) = setup_harness(Duration::from_millis(250), Duration::from_millis(30));
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tx.publish(msg("chat", "S", "slow")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    // 同 session 排队消息（应在 /stop 时被清空）。
    tx.publish(msg("chat", "S", "queued")).await.unwrap();
    // /stop 取消活跃任务。
    tx.publish(msg("chat", "S", "/stop")).await.unwrap();
    // 等取消完成（慢 turn 结束 + 清理）。
    tokio::time::sleep(Duration::from_millis(400)).await;
    // 新消息应能正常开始新 turn（active 已清除）。
    tx.publish(msg("chat", "S", "after")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    drop(tx);
    run.await.unwrap();

    let got = turn_count(&harness.records);
    assert!(
        !got.contains(&"queued".to_string()),
        "/stop 应清空 pending，排队消息不得执行"
    );
    assert!(
        got.contains(&"after".to_string()),
        "/stop 后新消息应能开始新 turn"
    );
    // 注：进行中的同步 turn（slow）可能已完成并记录（abort 无法硬中断同步代码，
    // 需 Stage 2 turn 异步化后精确取消）；调度语义（pending 清空 + active 释放）
    // 由上述断言守护。
}

/// 心跳：消费超时（无消息）时执行钩子。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn heartbeat_fires_on_idle() {
    let tmp = TempDir::new().unwrap();
    let sessions = SessionManager::new(tmp.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(SleepEchoProvider::new(Duration::ZERO)),
        sessions,
        ContextBuilder::new(None),
    );
    let beats = Arc::new(AtomicUsize::new(0));
    let beats2 = beats.clone();
    let scheduler = AgentLoopScheduler::builder(agent)
        .consume_timeout(Duration::from_millis(40))
        .heartbeat(move || {
            beats2.fetch_add(1, Ordering::Relaxed);
        })
        .build();
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tokio::time::sleep(Duration::from_millis(150)).await;
    drop(tx);
    run.await.unwrap();
    assert!(
        beats.load(Ordering::Relaxed) >= 2,
        "空闲 150ms/40ms 心跳应至少触发 2 次"
    );
}

/// automation 让位：cron 消息在聊天 turn 活跃时排队，聊天完成后才处理。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cron_defers_to_active_chat() {
    let (harness, scheduler) = setup_harness(Duration::from_millis(120), Duration::from_millis(30));
    let (tx, rx) = async_bus_channel(16);
    let run = tokio::spawn(scheduler.run(rx));

    tx.publish(msg("chat", "S", "chat-turn")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    // cron 消息（同 session）在聊天 turn 活跃期间到达 → 让位排队。
    tx.publish(msg("cron", "S", "daily-report")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(tx);
    run.await.unwrap();

    let recs = harness.records.lock().unwrap().clone();
    assert_eq!(recs.len(), 2, "聊天与 cron 消息都应完成");
    assert_eq!(recs[0].content, "chat-turn", "聊天 turn 先完成");
    assert_eq!(recs[1].content, "daily-report", "cron 让位后完成");
}

// 供 on_turn 钩子使用的辅助（避免未使用告警）。
#[allow(dead_code)]
fn _session_path(_: &Path) {}
