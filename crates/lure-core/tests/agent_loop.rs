//! 映射上游 `tests/agent/test_loop_runner_integration.py` 的最小闭环场景。
//!
//! 用 fake provider 替代真实 LLM：验证输入进入 loop、返回最终回复、user/assistant
//! turn 被保存、下一轮可读历史、provider 失败形成结构化错误。
//!
//! 暂未映射：streaming、tool 执行循环、goal/subagent、consolidation —— 属后续 phase。

use std::sync::{Arc, Mutex};

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
    replies: Mutex<Vec<String>>,
}

impl ScriptedProvider {
    fn new(replies: Vec<&str>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().rev().map(String::from).collect()),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ScriptedProvider {
    fn default_model(&self) -> &str {
        "test-model"
    }

    async fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        let reply = self.replies.lock().unwrap().pop().unwrap_or_default();
        Ok(LlmResponse::text(reply))
    }
}

/// 总是失败的 provider。
struct FailingProvider;

#[async_trait::async_trait]
impl LlmProvider for FailingProvider {
    fn default_model(&self) -> &str {
        "failing"
    }

    async fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        Err(ProviderError::Request("模拟网络失败".to_string()))
    }
}

/// 返回带 reasoning_content 的推理模型 provider。
struct ReasoningProvider;

#[async_trait::async_trait]
impl LlmProvider for ReasoningProvider {
    fn default_model(&self) -> &str {
        "reasoner"
    }

    async fn complete(&self, _request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
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
/// 用 `Arc<Mutex>` 满足 `LlmProvider + Send`（异步调度跨线程共享）。
struct CapturingProvider {
    last: Arc<Mutex<Option<CompletionRequest>>>,
}

#[async_trait::async_trait]
impl LlmProvider for CapturingProvider {
    fn default_model(&self) -> &str {
        "capturing-default"
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        *self.last.lock().unwrap() = Some(request.clone());
        Ok(LlmResponse::text("ok"))
    }
}

#[tokio::test]
async fn with_runtime_drives_model_and_settings_for_provider_call() {
    // resolver 从 config 解析出的 runtime 应决定 provider 调用的 model 与生成参数。
    let mut config = Config::default();
    config.agents.defaults.model = "deepseek-chat".to_string();
    config.agents.defaults.max_tokens = 1234;
    config.agents.defaults.temperature = 0.42;
    let runtime = ModelRuntimeResolver::new(config).admit(None).unwrap();

    let captured = Arc::new(Mutex::new(None));
    let provider = CapturingProvider {
        last: Arc::clone(&captured),
    };

    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None))
        .with_runtime(&runtime);

    let input = InboundMessage::new("cli", "direct", "hi".to_string());
    agent_loop.process(&input).await.unwrap();

    let request = captured
        .lock()
        .unwrap()
        .clone()
        .expect("provider 应被调用一次");
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

fn loop_with(provider: Box<dyn LlmProvider + Send>) -> (TempDir, AgentLoop) {
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent_loop = AgentLoop::new(provider, sessions, ContextBuilder::new(None));
    (dir, agent_loop)
}

#[tokio::test]
async fn reasoning_content_is_persisted_and_exposed_in_outcome() {
    let (dir, mut agent_loop) = loop_with(Box::new(ReasoningProvider));
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "问题"))
        .await
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

#[tokio::test]
async fn process_returns_provider_final_reply() {
    let (_dir, mut agent_loop) = loop_with(Box::new(ScriptedProvider::new(vec!["done"])));

    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hello"))
        .await
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

#[tokio::test]
async fn process_saves_user_and_assistant_turns() {
    let (dir, mut agent_loop) = loop_with(Box::new(ScriptedProvider::new(vec!["reply"])));

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "hello"))
        .await
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

#[tokio::test]
async fn second_turn_sees_prior_history() {
    let provider = ScriptedProvider::new(vec!["first", "second"]);
    let dir = tempfile::tempdir().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let mut agent_loop = AgentLoop::new(Box::new(provider), sessions, ContextBuilder::new(None));

    agent_loop
        .process(&InboundMessage::new("cli", "direct", "hi"))
        .await
        .unwrap();
    let outcome = agent_loop
        .process(&InboundMessage::new("cli", "direct", "again"))
        .await
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

#[tokio::test]
async fn provider_failure_surfaces_structured_error() {
    let (_dir, mut agent_loop) = loop_with(Box::new(FailingProvider));

    let err = agent_loop
        .process(&InboundMessage::new("cli", "direct", "hello"))
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        AgentError::Provider(ProviderError::Request(_))
    ));
    assert!(err.to_string().contains("provider"));
}

#[tokio::test]
async fn context_projection_drops_internal_fields() {
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
