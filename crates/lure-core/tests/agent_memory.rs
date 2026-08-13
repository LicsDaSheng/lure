//! memory 接入 agent loop：每轮把长期记忆注入 context，user/assistant turn 追加到
//! `history.jsonl`，并可用可替换 `DreamRunner` 触发整合。
//!
//! 用 capturing fake provider + 临时 workspace 驱动，不接真实 LLM/网络。

use std::sync::{Arc, Mutex};

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::InboundMessage;
use lure_core::memory::{DreamRunner, HistoryEntry, MemoryStore};
use lure_core::provider::{CompletionRequest, LlmProvider, LlmResponse, ProviderError};
use lure_core::session::SessionManager;
use serde_json::Value;
use tempfile::TempDir;

/// 记录最近一次请求、返回固定回复的 fake provider。
struct CapturingProvider {
    last: Arc<Mutex<Option<CompletionRequest>>>,
    reply: String,
}

#[async_trait::async_trait]
impl LlmProvider for CapturingProvider {
    fn default_model(&self) -> &str {
        "mem-model"
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        *self.last.lock().unwrap() = Some(request.clone());
        Ok(LlmResponse::text(&self.reply))
    }
}

/// 把历史条数写进 MEMORY.md 的 fake dream runner。
struct CountingDream;

#[async_trait::async_trait]
impl DreamRunner for CountingDream {
    async fn consolidate(&self, _current: &str, entries: &[HistoryEntry]) -> String {
        format!("整合了 {} 条历史", entries.len())
    }
}

fn capturing(reply: &str) -> (CapturingProvider, Arc<Mutex<Option<CompletionRequest>>>) {
    let last = Arc::new(Mutex::new(None));
    (
        CapturingProvider {
            last: Arc::clone(&last),
            reply: reply.to_string(),
        },
        last,
    )
}

fn loop_with_memory(dir: &TempDir, provider: CapturingProvider) -> AgentLoop {
    let store = MemoryStore::new(dir.path()).unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    AgentLoop::new(
        Box::new(provider),
        sessions,
        ContextBuilder::new(Some("你是助手".to_string())),
    )
    .with_memory(store)
}

#[tokio::test]
async fn memory_context_is_injected_into_provider_messages() {
    let dir = tempfile::tempdir().unwrap();
    // 先写入长期记忆。
    MemoryStore::new(dir.path())
        .unwrap()
        .write_memory("用户喜欢简洁回答");

    let (provider, captured) = capturing("好的");
    let mut agent_loop = loop_with_memory(&dir, provider);

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .await
        .unwrap();

    let request = captured.lock().unwrap().clone().unwrap();
    let has_memory = request.messages.iter().any(|m| {
        m.get("role").and_then(Value::as_str) == Some("system")
            && m.get("content").and_then(Value::as_str).is_some_and(|c| {
                c.contains("## Long-term Memory") && c.contains("用户喜欢简洁回答")
            })
    });
    assert!(
        has_memory,
        "provider 消息应含长期记忆块: {:?}",
        request.messages
    );
}

#[tokio::test]
async fn workspace_context_is_one_complete_system_message_in_production_loop() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "只使用中文回答").unwrap();
    let store = MemoryStore::new(dir.path()).unwrap();
    store.write_memory("用户偏好简洁答案");
    let sessions = SessionManager::new(dir.path()).unwrap();
    let (provider, captured) = capturing("好的");
    let mut agent = AgentLoop::new(
        Box::new(provider),
        sessions,
        ContextBuilder::for_workspace(dir.path()),
    )
    .with_memory(store);

    agent
        .process(&InboundMessage::new("cli", "direct", "你好"))
        .await
        .unwrap();

    let request = captured.lock().unwrap().clone().unwrap();
    let systems: Vec<&Value> = request
        .messages
        .iter()
        .filter(|message| message["role"] == "system")
        .collect();
    assert_eq!(systems.len(), 1);
    let prompt = systems[0]["content"].as_str().unwrap();
    assert!(prompt.contains("## Runtime"));
    assert!(prompt.contains("只使用中文回答"));
    assert!(prompt.contains("# Tool Usage Notes"));
    assert!(prompt.contains("用户偏好简洁答案"));
}

#[tokio::test]
async fn process_appends_user_and_assistant_to_history_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let (provider, _captured) = capturing("回答内容");
    let mut agent_loop = loop_with_memory(&dir, provider);

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "问题内容"))
        .await
        .unwrap();

    // 冷启动读回 history.jsonl（按 session 过滤）。
    let store = MemoryStore::new(dir.path()).unwrap();
    let entries = store.read_recent_history_for_prompt(0, Some("cli:direct"));
    let contents: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
    assert!(
        contents.iter().any(|c| c.contains("问题内容")),
        "history 应含 user 内容: {contents:?}"
    );
    assert!(
        contents.iter().any(|c| c.contains("回答内容")),
        "history 应含 assistant 内容: {contents:?}"
    );
}

#[tokio::test]
async fn consolidate_uses_runner_and_updates_memory() {
    let dir = tempfile::tempdir().unwrap();
    let (provider, _captured) = capturing("回答");
    let mut agent_loop = loop_with_memory(&dir, provider);

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "问题"))
        .await
        .unwrap();

    let outcome = agent_loop.consolidate(&CountingDream).await.unwrap();
    assert!(outcome.processed >= 2, "应整合 user+assistant 至少 2 条");

    // MEMORY.md 被 runner 结果覆盖。
    let store = MemoryStore::new(dir.path()).unwrap();
    assert!(store.read_memory().contains("整合了"));
}

#[tokio::test]
async fn without_memory_no_history_and_consolidate_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let (provider, _captured) = capturing("答");
    let sessions = SessionManager::new(dir.path()).unwrap();
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .await
        .unwrap();

    // 未挂 memory：consolidate 返回 None。
    assert!(agent_loop.consolidate(&CountingDream).await.is_none());
    // history.jsonl 无内容。
    let store = MemoryStore::new(dir.path()).unwrap();
    assert!(store.read_recent_history_for_prompt(0, None).is_empty());
}
