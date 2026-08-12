//! Gateway 把 agent 的 progress 事件转发给目标 channel（outbound 运行时事件）。
//!
//! 覆盖：echo 单轮的 Started/Final 转发、tool 轮的 ToolInvoked 转发、路由信息带出、
//! 最终 outbound 仍投递。用 fake provider + RecordingChannel，不触网。

use std::sync::Mutex;

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::bus::{InboundMessage, ProgressKind};
use lure_core::channel::RecordingChannel;
use lure_core::gateway::Gateway;
use lure_core::provider::{
    CompletionRequest, EchoProvider, LlmProvider, LlmResponse, ProviderError, ToolCall,
};
use lure_core::session::SessionManager;
use lure_core::tool::{Tool, ToolRegistry, ToolResult};
use serde_json::{json, Value};
use tempfile::tempdir;

#[tokio::test]
async fn gateway_forwards_started_and_final_progress_to_channel() {
    let dir = tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    let mut gateway = Gateway::new(agent);

    let channel = RecordingChannel::new("cli");
    let progress_log = channel.progress_log();
    let delivery_log = channel.delivery_log();
    gateway.register_channel(Box::new(channel)).unwrap();
    gateway.start();

    gateway.submit(InboundMessage::new("cli", "direct", "hello"));
    gateway.dispatch_pending().await.unwrap();

    // 最终 outbound 仍投递。
    assert_eq!(delivery_log.lock().unwrap().len(), 1);

    // progress 转发：Started + Final，带路由信息。
    let progress = progress_log.lock().unwrap();
    assert!(
        progress.iter().any(|p| p.kind == ProgressKind::Started),
        "应含 Started: {progress:?}"
    );
    assert!(
        progress
            .iter()
            .any(|p| p.kind == ProgressKind::Final && p.content == "echo: hello"),
        "应含 Final: {progress:?}"
    );
    assert!(progress
        .iter()
        .all(|p| p.channel == "cli" && p.chat_id == "direct"));
}

/// 先返回 tool_call、再返回终态文本的脚本化 provider。
struct ScriptedToolProvider {
    responses: Mutex<Vec<LlmResponse>>,
}

#[async_trait::async_trait]
impl LlmProvider for ScriptedToolProvider {
    fn default_model(&self) -> &str {
        "tool-model"
    }
    async fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| LlmResponse::text("")))
    }
}

struct NoopTool;

impl Tool for NoopTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "noop"
    }
    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }
    fn execute(&self, _args: &Value) -> ToolResult {
        ToolResult::ok("done")
    }
}

#[tokio::test]
async fn gateway_forwards_tool_invoked_progress() {
    let dir = tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();

    let tool_call = LlmResponse {
        content: None,
        reasoning_content: None,
        finish_reason: "tool_calls".to_string(),
        usage: Default::default(),
        tool_calls: vec![ToolCall {
            id: "call_1".to_string(),
            name: "echo".to_string(),
            arguments: "{}".to_string(),
        }],
    };
    let provider = ScriptedToolProvider {
        responses: Mutex::new(vec![LlmResponse::text("最终答案"), tool_call]),
    };
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(NoopTool));

    let agent = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);
    let mut gateway = Gateway::new(agent);

    let channel = RecordingChannel::new("cli");
    let progress_log = channel.progress_log();
    gateway.register_channel(Box::new(channel)).unwrap();
    gateway.start();

    gateway.submit(InboundMessage::new("cli", "direct", "go"));
    gateway.dispatch_pending().await.unwrap();

    let progress = progress_log.lock().unwrap();
    assert!(
        progress
            .iter()
            .any(|p| p.kind == ProgressKind::ToolInvoked && p.content == "echo"),
        "应含 ToolInvoked(echo): {progress:?}"
    );
}
