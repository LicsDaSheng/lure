//! Agent tool-call 循环：provider 返回 tool_calls → registry 执行 → 追加 tool turn →
//! 回灌历史再调用，直到无 tool_calls 或达到上限。
//!
//! 映射上游 `tests/agent/` 的 tool 执行循环最小切片：用脚本化 fake provider + 内存
//! echo tool 驱动，不触网。streaming、并行 tool、subagent 属后续。

use std::cell::RefCell;
use std::rc::Rc;

use lure_core::agent::{AgentLoop, ContextBuilder, ProgressEvent, MAX_TOOL_ITERATIONS};
use lure_core::bus::InboundMessage;
use lure_core::provider::{CompletionRequest, LlmProvider, LlmResponse, ProviderError, ToolCall};
use lure_core::session::SessionManager;
use lure_core::tool::{Tool, ToolRegistry, ToolResult};
use serde_json::{json, Value};
use tempfile::TempDir;

/// 按序返回预置 `LlmResponse` 的 fake provider；记录每次收到的 messages。
struct ScriptedToolProvider {
    responses: RefCell<Vec<LlmResponse>>,
    seen: Rc<RefCell<Vec<Vec<Value>>>>,
}

impl ScriptedToolProvider {
    fn new(responses: Vec<LlmResponse>, seen: Rc<RefCell<Vec<Vec<Value>>>>) -> Self {
        Self {
            responses: RefCell::new(responses.into_iter().rev().collect()),
            seen,
        }
    }
}

impl LlmProvider for ScriptedToolProvider {
    fn default_model(&self) -> &str {
        "tool-model"
    }

    fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        self.seen.borrow_mut().push(request.messages.clone());
        Ok(self
            .responses
            .borrow_mut()
            .pop()
            .unwrap_or_else(|| LlmResponse::text("")))
    }
}

/// 始终返回一个 tool_call 的 provider（用于验证迭代上限）。
struct AlwaysToolProvider {
    calls: Rc<RefCell<usize>>,
}

impl LlmProvider for AlwaysToolProvider {
    fn default_model(&self) -> &str {
        "always-tool"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        *self.calls.borrow_mut() += 1;
        Ok(tool_call_response("call_x", "echo", r#"{"text":"loop"}"#))
    }
}

/// 记录调用参数的内存 echo tool。
struct EchoTool {
    calls: Rc<RefCell<Vec<Value>>>,
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
        self.calls.borrow_mut().push(args.clone());
        let text = args.get("text").and_then(Value::as_str).unwrap_or("");
        ToolResult::ok(format!("tool-echo: {text}"))
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
    Rc<RefCell<Vec<Value>>>,
    Rc<RefCell<Vec<Vec<Value>>>>,
);

fn setup(responses: Vec<LlmResponse>) -> ToolLoopFixture {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let provider = ScriptedToolProvider::new(responses, Rc::clone(&seen));

    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool {
        calls: Rc::clone(&calls),
    }));

    let agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);
    (dir, agent_loop, calls, seen)
}

#[test]
fn single_tool_round_executes_and_returns_final_reply() {
    let (dir, mut agent_loop, calls, seen) = setup(vec![
        tool_call_response("call_1", "echo", r#"{"text":"hi"}"#),
        LlmResponse::text("done"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .unwrap();

    // 最终回复来自第二次（无 tool_calls）响应。
    assert_eq!(outcome.final_content, "done");

    // echo tool 被调用一次，参数解析自 tool_call.arguments。
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(calls.borrow()[0], json!({"text": "hi"}));

    // 第二次调用的上下文应含 assistant(tool_calls) 与 tool 结果。
    let second_call = &seen.borrow()[1];
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

#[test]
fn tool_round_emits_tool_invoked_progress() {
    let (_dir, mut agent_loop, _calls, _seen) = setup(vec![
        tool_call_response("call_1", "echo", r#"{"text":"hi"}"#),
        LlmResponse::text("done"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
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

#[test]
fn tool_loop_stops_at_max_iterations() {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let call_count = Rc::new(RefCell::new(0usize));
    let provider = AlwaysToolProvider {
        calls: Rc::clone(&call_count),
    };
    let echo_calls = Rc::new(RefCell::new(Vec::new()));
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(EchoTool {
        calls: Rc::clone(&echo_calls),
    }));
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);

    // 不应挂起：达到上限即停止。
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .unwrap();

    // provider 调用次数 = 上限；tool 执行次数 = 上限。
    assert_eq!(*call_count.borrow(), MAX_TOOL_ITERATIONS);
    assert_eq!(echo_calls.borrow().len(), MAX_TOOL_ITERATIONS);
    // 有结构化 progress，不 panic。
    assert!(!outcome.progress.is_empty());
}

#[test]
fn unknown_tool_yields_error_result_and_loop_recovers() {
    let (_dir, mut agent_loop, calls, _seen) = setup(vec![
        tool_call_response("call_1", "does_not_exist", "{}"),
        LlmResponse::text("recovered"),
    ]);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .unwrap();

    assert_eq!(outcome.final_content, "recovered");
    // echo 未被调用（请求的是未知工具）。
    assert!(calls.borrow().is_empty());
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

#[test]
fn empty_tool_result_is_replaced_with_marker() {
    // 对齐上游 `ensure_nonempty_tool_result`：工具产出空/纯空白时，回灌历史前替换为
    // `(<tool> completed with no output)`，避免模型看到空白 tool turn。
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let provider = ScriptedToolProvider::new(
        vec![
            tool_call_response("call_1", "blank", "{}"),
            LlmResponse::text("done"),
        ],
        Rc::clone(&seen),
    );
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(BlankTool));
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_tools(registry);

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .unwrap();
    assert_eq!(outcome.final_content, "done");

    // 第二次上下文里 tool 结果应为标记，而非空串。
    let second_call = &seen.borrow()[1];
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

#[test]
fn without_registry_tool_calls_are_treated_as_final() {
    // 未注册 tool registry 时，即便 provider 返回 tool_calls 也直接作为终态。
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut response = tool_call_response("call_1", "echo", r#"{"text":"hi"}"#);
    response.content = Some("partial".to_string());
    let provider = ScriptedToolProvider::new(vec![response], Rc::clone(&seen));
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "go"))
        .unwrap();

    assert_eq!(outcome.final_content, "partial");
    // 只调用一次 provider，历史仅 user + assistant。
    assert_eq!(seen.borrow().len(), 1);
    let history = agent_loop
        .sessions_mut()
        .get_or_create("cli:direct")
        .unwrap()
        .get_history(100);
    assert_eq!(history.len(), 2);
}
