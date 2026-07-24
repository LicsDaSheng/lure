//! 映射上游 `tests/agent/test_loop_runner_integration.py` 的最小闭环场景。
//!
//! 用 fake provider 替代真实 LLM：验证输入进入 loop、返回最终回复、user/assistant
//! turn 被保存、下一轮可读历史、provider 失败形成结构化错误。
//!
//! 暂未映射：streaming、tool 执行循环、goal/subagent、consolidation —— 属后续 phase。

use std::cell::RefCell;
use std::rc::Rc;

use lure_core::agent::{AgentError, AgentLoop, ContextBuilder, ProgressEvent};
use lure_core::bus::InboundMessage;
use lure_core::config::Config;
use lure_core::provider::{
    CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ModelRuntimeResolver,
    ProviderError,
};
use lure_core::session::SessionManager;
use tempfile::TempDir;

/// 按序返回预置回复的 fake provider。
struct ScriptedProvider {
    replies: RefCell<Vec<String>>,
}

impl ScriptedProvider {
    fn new(replies: Vec<&str>) -> Self {
        Self {
            replies: RefCell::new(replies.into_iter().rev().map(String::from).collect()),
        }
    }
}

impl LlmProvider for ScriptedProvider {
    fn default_model(&self) -> &str {
        "test-model"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        let reply = self.replies.borrow_mut().pop().unwrap_or_default();
        Ok(LlmResponse::text(reply))
    }
}

/// 总是失败的 provider。
struct FailingProvider;

impl LlmProvider for FailingProvider {
    fn default_model(&self) -> &str {
        "failing"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        Err(ProviderError::Request("模拟网络失败".to_string()))
    }
}

/// 返回带 reasoning_content 的推理模型 provider。
struct ReasoningProvider;

impl LlmProvider for ReasoningProvider {
    fn default_model(&self) -> &str {
        "reasoner"
    }

    fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        Ok(LlmResponse {
            content: Some("答案".to_string()),
            reasoning_content: Some("思考过程".to_string()),
            finish_reason: "stop".to_string(),
            usage: Default::default(),
            tool_calls: Vec::new(),
        })
    }
}

/// 记录最近一次 `CompletionRequest`（用于断言 model/settings 来自 runtime）。
struct CapturingProvider {
    last: Rc<RefCell<Option<CompletionRequest>>>,
}

impl LlmProvider for CapturingProvider {
    fn default_model(&self) -> &str {
        "capturing-default"
    }

    fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        *self.last.borrow_mut() = Some(request.clone());
        Ok(LlmResponse::text("ok"))
    }
}

#[test]
fn with_runtime_drives_model_and_settings_for_provider_call() {
    // resolver 从 config 解析出的 runtime 应决定 provider 调用的 model 与生成参数。
    let mut config = Config::default();
    config.agents.defaults.model = "deepseek-chat".to_string();
    config.agents.defaults.max_tokens = 1234;
    config.agents.defaults.temperature = 0.42;
    let runtime = ModelRuntimeResolver::new(config).admit(None).unwrap();

    let captured = Rc::new(RefCell::new(None));
    let provider = CapturingProvider {
        last: Rc::clone(&captured),
    };

    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_runtime(&runtime);

    let input = InboundMessage::new("cli", "direct", "hi".to_string());
    agent_loop.process(&input).unwrap();

    let request = captured.borrow().clone().expect("provider 应被调用一次");
    assert_eq!(request.model, "deepseek-chat");
    assert_eq!(
        request.settings,
        GenerationSettings {
            temperature: 0.42,
            max_tokens: 1234,
            reasoning_effort: None,
        }
    );
}

fn loop_with(provider: Box<dyn LlmProvider>) -> (TempDir, AgentLoop) {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent_loop = AgentLoop::new(provider, sessions, ContextBuilder::new(None));
    (dir, agent_loop)
}

#[test]
fn reasoning_content_is_persisted_and_exposed_in_outcome() {
    let (dir, mut agent_loop) = loop_with(Box::new(ReasoningProvider));
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "问题"))
        .unwrap();

    // TurnOutcome 暴露 reasoning，供 CLI --show-reasoning 打印。
    assert_eq!(outcome.final_content, "答案");
    assert_eq!(outcome.reasoning.as_deref(), Some("思考过程"));

    // 冷启动读回，assistant turn 带 reasoning_content。
    let mut reloaded = SessionManager::new(dir.path()).unwrap();
    let session = reloaded.get_or_create("cli:direct").unwrap();
    let history = session.get_history(100);
    assert_eq!(history[1]["role"], "assistant");
    assert_eq!(history[1]["content"], "答案");
    assert_eq!(history[1]["reasoning_content"], "思考过程");
}

#[test]
fn process_returns_provider_final_reply() {
    let (_dir, mut agent_loop) = loop_with(Box::new(ScriptedProvider::new(vec!["done"])));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hello"))
        .unwrap();

    assert_eq!(outcome.final_content, "done");
    assert_eq!(
        outcome.progress.first(),
        Some(&ProgressEvent::TurnStarted {
            session_key: "cli:direct".to_string()
        })
    );
    assert_eq!(
        outcome.progress.last(),
        Some(&ProgressEvent::FinalResponse {
            content: "done".to_string()
        })
    );
}

#[test]
fn process_saves_user_and_assistant_turns() {
    let (dir, mut agent_loop) = loop_with(Box::new(ScriptedProvider::new(vec!["reply"])));

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "hello"))
        .unwrap();

    // 冷启动新 manager，从磁盘读回，确认两条 turn 已持久化。
    let mut reloaded = SessionManager::new(dir.path()).unwrap();
    let session = reloaded.get_or_create("cli:direct").unwrap();
    let history = session.get_history(100);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["role"], "user");
    assert_eq!(history[0]["content"], "hello");
    assert_eq!(history[1]["role"], "assistant");
    assert_eq!(history[1]["content"], "reply");
}

#[test]
fn second_turn_sees_prior_history() {
    let provider = ScriptedProvider::new(vec!["first", "second"]);
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .unwrap();
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "again"))
        .unwrap();

    assert_eq!(outcome.final_content, "second");

    // 两轮后历史应含：user/assistant/user/assistant = 4 条。
    let session = agent_loop
        .sessions_mut()
        .get_or_create("cli:direct")
        .unwrap();
    let history = session.get_history(100);
    let roles: Vec<&str> = history
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["user", "assistant", "user", "assistant"]);
}

#[test]
fn provider_failure_surfaces_structured_error() {
    let (_dir, mut agent_loop) = loop_with(Box::new(FailingProvider));

    let err = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hello"))
        .unwrap_err();

    assert!(matches!(
        err,
        AgentError::Provider(ProviderError::Request(_))
    ));
    assert!(err.to_string().contains("provider"));
}

#[test]
fn context_projection_drops_internal_fields() {
    let builder = ContextBuilder::new(Some("system prompt".to_string()));
    let history = vec![serde_json::json!({
        "role": "assistant",
        "content": "答案",
        "timestamp": "2026-07-23T00:00:00+00:00",
        "reasoning_content": "思考过程"
    })];

    let built = builder.build(&history);
    assert_eq!(built[0]["role"], "system");
    assert_eq!(
        built[1],
        serde_json::json!({"role": "assistant", "content": "答案"})
    );
    // timestamp 与 reasoning_content 不回放给 provider。
    assert!(built[1].get("timestamp").is_none());
    assert!(built[1].get("reasoning_content").is_none());
}
