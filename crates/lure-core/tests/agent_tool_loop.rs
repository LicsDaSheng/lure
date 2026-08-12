//! Agent tool-call 循环：provider 返回 tool_calls → registry 执行 → 追加 tool turn →
//! 回灌历史再调用，直到无 tool_calls 或达到上限。
//!
//! 映射上游 `tests/agent/` 的 tool 执行循环最小切片：用脚本化 fake provider + 内存
//! echo tool 驱动，不触网。streaming、并行 tool、subagent 属后续。

use std::sync::{Arc, Mutex};

use lure_core::agent::{
    AgentLoop, ContextBuilder, ProgressEvent, EMPTY_FINAL_RESPONSE_MESSAGE,
    FINALIZATION_RETRY_PROMPT, MAX_TOOL_ITERATIONS,
};
use lure_core::bus::InboundMessage;
use lure_core::provider::{CompletionRequest, LlmProvider, LlmResponse, ProviderError, ToolCall};
use lure_core::session::SessionManager;
use lure_core::tool::{Tool, ToolRegistry, ToolResult};
use serde_json::{json, Map, Value};
use tempfile::TempDir;

/// 按序返回预置 `LlmResponse` 的 fake provider；记录每次收到的 messages。
struct ScriptedToolProvider {
    responses: Mutex<Vec<LlmResponse>>,
    seen: Arc<Mutex<Vec<Vec<Value>>>>,
}

impl ScriptedToolProvider {
    fn new(responses: Vec<LlmResponse>, seen: Arc<Mutex<Vec<Vec<Value>>>>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().rev().collect()),
            seen,
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ScriptedToolProvider {
    fn default_model(&self) -> &str {
        "tool-model"
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        self.seen.lock().unwrap().push(request.messages.clone());
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| LlmResponse::text("")))
    }
}

/// 始终返回一个 tool_call 的 provider（用于验证迭代上限）。
struct AlwaysToolProvider {
    calls: Arc<Mutex<usize>>,
}

#[async_trait::async_trait]
impl LlmProvider for AlwaysToolProvider {
    fn default_model(&self) -> &str {
        "always-tool"
    }

    async fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        *self.calls.lock().unwrap() += 1;
        Ok(tool_call_response("call_x", "echo", r#"{"text":"loop"}"#))
    }
}

/// 记录调用参数的内存 echo tool。
struct EchoTool {
    calls: Arc<Mutex<Vec<Value>>>,
}

impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "echo back the text argument"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        })
    }
    fn execute(&self, args: &Value) -> ToolResult {
        self.calls.lock().unwrap().push(args.clone());
        let text = args.get("text").and_then(Value::as_str).unwrap_or("");
        ToolResult::ok(format!("tool-echo: {text}"))
    }
}

/// 构造 usage map（prompt / completion / cached）。
fn usage_map(prompt: i64, completion: i64, cached: i64) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("prompt_tokens".to_string(), prompt.into());
    m.insert("completion_tokens".to_string(), completion.into());
    m.insert("cached_tokens".to_string(), cached.into());
    m
}

/// 内容为空、无 tool_calls 的响应（触发空终响应路径）。
fn blank_response() -> LlmResponse {
    LlmResponse {
        content: None,
        reasoning_content: None,
        finish_reason: "stop".to_string(),
        usage: Default::default(),
        tool_calls: Vec::new(),
    }
}

fn tool_call_response(id: &str, name: &str, arguments: &str) -> LlmResponse {
    LlmResponse {
        content: None,
        reasoning_content: None,
        finish_reason: "tool_calls".to_string(),
        usage: Default::default(),
        tool_calls: vec![ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        }],
    }
}

/// setup 产物：临时目录、loop、echo 调用记录、每次 provider 收到的 messages。
type ToolLoopFixture = (
    TempDir,
    AgentLoop,
    Arc<Mutex<Vec<Value>>>,
    Arc<Mutex<Vec<Vec<Value>>>>,
);

fn setup(responses: Vec<LlmResponse>) -> ToolLoopFixture {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedToolProvider::new(responses, Arc::clone(&seen));

    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool {
        calls: Arc::clone(&calls),
    }));

    let agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);
    (dir, agent_loop, calls, seen)
}

#[tokio::test]
async fn single_tool_round_executes_and_returns_final_reply() {
    let (dir, mut agent_loop, calls, seen) = setup(vec![
        tool_call_response("call_1", "echo", r#"{"text":"hi"}"#),
        LlmResponse::text("done"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    // 最终回复来自第二次（无 tool_calls）响应。
    assert_eq!(outcome.final_content, "done");

    // echo tool 被调用一次，参数解析自 tool_call.arguments。
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(calls.lock().unwrap()[0], json!({"text": "hi"}));

    // 第二次调用的上下文应含 assistant(tool_calls) 与 tool 结果。
    let second_call = &seen.lock().unwrap()[1];
    let has_tool_result = second_call.iter().any(|m| {
        m.get("role").and_then(Value::as_str) == Some("tool")
            && m.get("content").and_then(Value::as_str) == Some("tool-echo: hi")
    });
    assert!(
        has_tool_result,
        "第二次上下文应含 tool 结果: {second_call:?}"
    );

    // 冷启动读回历史：user, assistant(tool_calls), tool, assistant(final)。
    let mut reloaded = SessionManager::new(dir.path()).unwrap();
    let history = reloaded
        .get_or_create("cli:direct")
        .unwrap()
        .get_history(100);
    let roles: Vec<&str> = history
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);
    assert_eq!(history[2]["tool_call_id"], "call_1");
    assert_eq!(history[3]["content"], "done");
}

#[tokio::test]
async fn tool_round_emits_tool_invoked_progress() {
    let (_dir, mut agent_loop, _calls, _seen) = setup(vec![
        tool_call_response("call_1", "echo", r#"{"text":"hi"}"#),
        LlmResponse::text("done"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert!(
        outcome
            .progress
            .iter()
            .any(|e| matches!(e, ProgressEvent::ToolInvoked { name } if name == "echo")),
        "progress 应含 ToolInvoked(echo): {:?}",
        outcome.progress
    );
}

#[tokio::test]
async fn process_streaming_emits_progress_events_live() {
    // 回调应实时收到 ToolInvoked / ContentDelta / FinalResponse，且序列与最终 progress 一致。
    let (_dir, mut agent_loop, _calls, _seen) = setup(vec![
        tool_call_response("call_1", "echo", r#"{"text":"hi"}"#),
        LlmResponse::text("done"),
    ]);

    let mut events: Vec<ProgressEvent> = Vec::new();
    let outcome = agent_loop
        .process_streaming(&InboundMessage::new("cli", "direct", "go"), &mut |ev| {
            events.push(ev.clone())
        })
        .await
        .unwrap();

    assert_eq!(outcome.final_content, "done");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ProgressEvent::ToolInvoked { name } if name == "echo")),
        "回调应含 ToolInvoked(echo): {events:?}"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, ProgressEvent::FinalResponse { content } if content == "done")));
    // 回调是并行实时通道：其序列应与最终 outcome.progress 完全一致。
    assert_eq!(events, outcome.progress);
}

#[tokio::test]
async fn tool_loop_stops_at_max_iterations() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let call_count = Arc::new(Mutex::new(0usize));
    let provider = AlwaysToolProvider {
        calls: Arc::clone(&call_count),
    };
    let echo_calls = Arc::new(Mutex::new(Vec::new()));
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool {
        calls: Arc::clone(&echo_calls),
    }));
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);

    // 不应挂起：达到上限即停止。
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    // provider 调用次数 = 上限；tool 执行次数 = 上限。
    assert_eq!(*call_count.lock().unwrap(), MAX_TOOL_ITERATIONS);
    assert_eq!(echo_calls.lock().unwrap().len(), MAX_TOOL_ITERATIONS);
    // 有结构化 progress，不 panic。
    assert!(!outcome.progress.is_empty());
}

#[tokio::test]
async fn unknown_tool_yields_error_result_and_loop_recovers() {
    let (_dir, mut agent_loop, calls, _seen) = setup(vec![
        tool_call_response("call_1", "does_not_exist", "{}"),
        LlmResponse::text("recovered"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert_eq!(outcome.final_content, "recovered");
    // echo 未被调用（请求的是未知工具）。
    assert!(calls.lock().unwrap().is_empty());
}

/// 返回空/纯空白内容的内存工具（验证空结果被替换为标记）。
struct BlankTool;

impl Tool for BlankTool {
    fn name(&self) -> &str {
        "blank"
    }
    fn description(&self) -> &str {
        "returns empty output"
    }
    fn parameters(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }
    fn execute(&self, _args: &Value) -> ToolResult {
        // 纯空白也应被视为空。
        ToolResult::ok("   ")
    }
}

#[tokio::test]
async fn empty_tool_result_is_replaced_with_marker() {
    // 对齐上游 `ensure_nonempty_tool_result`：工具产出空/纯空白时，回灌历史前替换为
    // `(<tool> completed with no output)`，避免模型看到空白 tool turn。
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedToolProvider::new(
        vec![
            tool_call_response("call_1", "blank", "{}"),
            LlmResponse::text("done"),
        ],
        Arc::clone(&seen),
    );
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(BlankTool));
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();
    assert_eq!(outcome.final_content, "done");

    // 第二次上下文里 tool 结果应为标记，而非空串。
    let second_call = &seen.lock().unwrap()[1];
    let tool_msg = second_call
        .iter()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("tool"))
        .expect("应有 tool 消息");
    assert_eq!(
        tool_msg.get("content").and_then(Value::as_str),
        Some("(blank completed with no output)")
    );

    // 持久化历史里的 tool turn 也应是标记。
    let mut reloaded = SessionManager::new(dir.path()).unwrap();
    let history = reloaded
        .get_or_create("cli:direct")
        .unwrap()
        .get_history(100);
    assert_eq!(history[2]["role"], "tool");
    assert_eq!(history[2]["content"], "(blank completed with no output)");
}

#[tokio::test]
async fn usage_accumulates_across_tool_rounds() {
    // 对齐上游 `test_runner_core::test_runner_accumulates_usage_and_preserves_cached_tokens`：
    // 跨轮次 provider 调用的 usage 按字段累加（含 cached_tokens）。
    let mut r1 = tool_call_response("call_1", "echo", r#"{"text":"hi"}"#);
    r1.content = Some("thinking".to_string());
    r1.usage = usage_map(100, 10, 80);
    let mut r2 = LlmResponse::text("done");
    r2.usage = usage_map(200, 20, 150);

    let (_dir, mut agent_loop, _calls, _seen) = setup(vec![r1, r2]);
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert_eq!(outcome.final_content, "done");
    assert_eq!(
        outcome.usage.get("prompt_tokens").and_then(Value::as_i64),
        Some(300)
    );
    assert_eq!(
        outcome
            .usage
            .get("completion_tokens")
            .and_then(Value::as_i64),
        Some(30)
    );
    assert_eq!(
        outcome.usage.get("cached_tokens").and_then(Value::as_i64),
        Some(230)
    );
}

#[tokio::test]
async fn normal_final_response_has_completed_stop_reason() {
    let (_dir, mut agent_loop, _calls, _seen) = setup(vec![LlmResponse::text("hi there")]);
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();
    assert_eq!(outcome.final_content, "hi there");
    assert_eq!(outcome.stop_reason, "completed");
}

#[tokio::test]
async fn empty_final_response_retries_then_finalizes_to_content() {
    // 对齐上游 `test_runner_retries_empty_final_response_with_summary_prompt`：
    // 空终响应先静默重试（<MAX_EMPTY_RETRIES），再触发 finalization（追加提示后请求一次）。
    let (dir, mut agent_loop, _calls, seen) = setup(vec![
        blank_response(),
        blank_response(),
        LlmResponse::text("final answer"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert_eq!(outcome.final_content, "final answer");
    assert_eq!(outcome.stop_reason, "completed");
    // provider 调用 3 次：主 + 1 次静默重试 + 1 次 finalization。
    assert_eq!(seen.lock().unwrap().len(), 3);

    // 第 3 次（finalization）上下文含 finalization 提示。
    let third = &seen.lock().unwrap()[2];
    let has_prompt = third.iter().any(|m| {
        m.get("role").and_then(Value::as_str) == Some("user")
            && m.get("content").and_then(Value::as_str) == Some(FINALIZATION_RETRY_PROMPT)
    });
    assert!(has_prompt, "finalization 上下文应含提示: {third:?}");

    // finalization 提示是瞬态的，不写入持久化历史：历史仅 user + assistant(final answer)。
    let mut reloaded = SessionManager::new(dir.path()).unwrap();
    let history = reloaded
        .get_or_create("cli:direct")
        .unwrap()
        .get_history(100);
    let roles: Vec<&str> = history
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["user", "assistant"]);
    assert_eq!(history[1]["content"], "final answer");
}

#[tokio::test]
async fn finalization_emits_finalizing_progress_event() {
    // 空终响应触发 finalization 时应发出 ProgressEvent::Finalizing（供等待指示器切文案）。
    let (_dir, mut agent_loop, _calls, _seen) = setup(vec![
        blank_response(),
        blank_response(),
        LlmResponse::text("final answer"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert_eq!(outcome.final_content, "final answer");
    assert!(
        outcome
            .progress
            .iter()
            .any(|e| matches!(e, ProgressEvent::Finalizing)),
        "finalization 应发出 Finalizing 事件: {:?}",
        outcome.progress
    );
}

#[tokio::test]
async fn all_empty_yields_empty_final_response_message() {
    // 对齐上游 `test_runner_uses_specific_message_after_empty_finalization_retry`：
    // 静默重试 + finalization 全部为空 → 固定兜底文案 + stop_reason=empty_final_response。
    let (_dir, mut agent_loop, _calls, seen) =
        setup(vec![blank_response(), blank_response(), blank_response()]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert_eq!(outcome.final_content, EMPTY_FINAL_RESPONSE_MESSAGE);
    assert_eq!(outcome.stop_reason, "empty_final_response");
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn without_registry_tool_calls_are_treated_as_final() {
    // 未注册 tool registry 时，即便 provider 返回 tool_calls 也直接作为终态。
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut response = tool_call_response("call_1", "echo", r#"{"text":"hi"}"#);
    response.content = Some("partial".to_string());
    let provider = ScriptedToolProvider::new(vec![response], Arc::clone(&seen));
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .await
        .unwrap();

    assert_eq!(outcome.final_content, "partial");
    // 只调用一次 provider，历史仅 user + assistant。
    assert_eq!(seen.lock().unwrap().len(), 1);
    let history = agent_loop
        .sessions_mut()
        .get_or_create("cli:direct")
        .unwrap()
        .get_history(100);
    assert_eq!(history.len(), 2);
}
