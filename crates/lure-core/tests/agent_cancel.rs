//! 取消令牌（Ctrl-C 中断当前 turn）的核心语义验证。
//!
//! `with_cancel` 挂载的 `Arc<AtomicBool>` 置位后，`process_streaming` 在检查点中止本轮：
//! 返回 `stop_reason="interrupted"`、空内容，且不持久化 assistant / 不保存 session。

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::InboundMessage;
use lure_core::provider::{CompletionRequest, LlmProvider, LlmResponse, ProviderError};
use lure_core::session::SessionManager;

/// 记录被调用次数的 provider；可选在被调用时置位 cancel flag（模拟流式期间 Ctrl-C）。
struct ProbeProvider {
    calls: Arc<AtomicUsize>,
    cancel_on_call: Option<Arc<AtomicBool>>,
    reply: Cell<u32>,
}

impl ProbeProvider {
    fn new(calls: Arc<AtomicUsize>, cancel_on_call: Option<Arc<AtomicBool>>) -> Self {
        Self {
            calls,
            cancel_on_call,
            reply: Cell::new(0),
        }
    }
}

impl LlmProvider for ProbeProvider {
    fn default_model(&self) -> &str {
        "probe"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(flag) = &self.cancel_on_call {
            flag.store(true, Ordering::SeqCst);
        }
        self.reply.set(self.reply.get() + 1);
        Ok(LlmResponse::text(format!("reply-{}", self.reply.get())))
    }
}

fn loop_with(provider: Box<dyn LlmProvider + Send>) -> (tempfile::TempDir, AgentLoop) {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent_loop = AgentLoop::new(provider, sessions, ContextBuilder::new(None));
    (dir, agent_loop)
}

#[test]
fn pre_set_cancel_aborts_before_calling_provider() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(true)); // 进入前即已置位
    let provider = ProbeProvider::new(Arc::clone(&calls), None);
    let (_dir, agent_loop) = loop_with(Box::new(provider));
    let mut agent_loop = agent_loop.with_cancel(Arc::clone(&cancel));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .unwrap();

    assert_eq!(outcome.stop_reason, "interrupted");
    assert_eq!(outcome.final_content, "");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "已取消时不应调用 provider");
}

#[test]
fn cancel_during_stream_discards_turn_without_persisting() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    // provider 被调用时置位 cancel：模拟流式响应期间按下 Ctrl-C。
    let provider = ProbeProvider::new(Arc::clone(&calls), Some(Arc::clone(&cancel)));
    let (dir, agent_loop) = loop_with(Box::new(provider));
    let mut agent_loop = agent_loop.with_cancel(Arc::clone(&cancel));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .unwrap();

    // 调了一次 provider，但流后检查点发现取消 → 作废本轮。
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(outcome.stop_reason, "interrupted");
    assert_eq!(outcome.final_content, "");

    // 冷启动读回：不应有 assistant turn（本轮未持久化、未 save）。
    let mut reloaded = SessionManager::new(dir.path()).unwrap();
    let session = reloaded.get_or_create("cli:direct").unwrap();
    let history = session.get_history(100);
    assert!(
        history.iter().all(|m| m["role"] != "assistant"),
        "中断的 turn 不应持久化 assistant 回复: {history:?}"
    );
}

#[test]
fn without_cancel_token_completes_normally() {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = ProbeProvider::new(Arc::clone(&calls), None);
    let (_dir, mut agent_loop) = loop_with(Box::new(provider));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .unwrap();

    // 未挂 cancel token：正常完成。
    assert_eq!(outcome.stop_reason, "completed");
    assert_eq!(outcome.final_content, "reply-1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
