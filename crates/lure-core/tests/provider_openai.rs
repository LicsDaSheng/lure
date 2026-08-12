//! OpenAI-compatible provider 的请求/响应 golden 与错误分类。
//!
//! 全部使用假传输，不触网；真实网络 smoke 留待接入真实 HTTP 传输时以显式 opt-in
//! 方式提供（见 upstream-test-ledger）。

use std::sync::{Arc, Mutex};

use lure_core::provider::{
    build_chat_request, parse_chat_response, CompletionRequest, GenerationSettings, HttpRequest,
    HttpResponse, HttpTransport, LlmProvider, OpenAiCompatProvider, ProviderError, ToolCall,
};
use serde_json::json;

type Capture = Arc<Mutex<Option<HttpRequest>>>;

struct FakeTransport {
    response: HttpResponse,
    captured: Capture,
}

impl FakeTransport {
    fn new(status: u16, body: &str) -> Self {
        Self {
            response: HttpResponse {
                status,
                body: body.to_string(),
            },
            captured: Arc::new(Mutex::new(None)),
        }
    }

    fn with_capture(status: u16, body: &str, captured: Capture) -> Self {
        Self {
            response: HttpResponse {
                status,
                body: body.to_string(),
            },
            captured,
        }
    }
}

#[async_trait::async_trait]
impl HttpTransport for FakeTransport {
    async fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        *self.captured.lock().unwrap() = Some(request.clone());
        Ok(self.response.clone())
    }
}

struct FailingTransport;

#[async_trait::async_trait]
impl HttpTransport for FailingTransport {
    async fn post_json(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
        Err("连接被拒绝".to_string())
    }
}

fn request(messages: Vec<serde_json::Value>) -> CompletionRequest {
    CompletionRequest {
        model: "gpt-4o".to_string(),
        messages,
        settings: GenerationSettings::default(),
    }
}

#[tokio::test]
async fn build_chat_request_matches_openai_shape() {
    let messages = vec![json!({"role": "user", "content": "hi"})];
    let body = build_chat_request("gpt-4o", &messages, &GenerationSettings::default());
    assert_eq!(
        body,
        json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hi"}],
            "temperature": 0.7,
            "max_tokens": 4096,
        })
    );
}

#[tokio::test]
async fn complete_sends_request_and_parses_response() {
    let body = json!({
        "choices": [{"message": {"role": "assistant", "content": "hello there"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 2}
    })
    .to_string();
    let transport = FakeTransport::new(200, &body);
    let provider = OpenAiCompatProvider::new(
        "https://api.example.test/v1",
        Some("sk-test".to_string()),
        "gpt-4o",
        transport,
    );

    let response = provider
        .complete(&request(vec![json!({"role": "user", "content": "hi"})]))
        .await
        .unwrap();

    assert_eq!(response.content.as_deref(), Some("hello there"));
    assert_eq!(response.finish_reason, "stop");
    assert_eq!(response.usage["prompt_tokens"], 5);
}

#[tokio::test]
async fn complete_targets_chat_completions_with_auth_header() {
    let body = json!({"choices": [{"message": {"content": "ok"}}]}).to_string();
    let captured: Capture = Arc::new(Mutex::new(None));
    let transport = FakeTransport::with_capture(200, &body, Arc::clone(&captured));
    let provider = OpenAiCompatProvider::new(
        "https://api.example.test/v1/",
        Some("sk-secret".to_string()),
        "gpt-4o",
        transport,
    );

    provider
        .complete(&request(vec![json!({"role": "user", "content": "hi"})]))
        .await
        .unwrap();

    let request = captured.lock().unwrap().clone().expect("应捕获到请求");
    assert_eq!(request.url, "https://api.example.test/v1/chat/completions");
    assert!(request
        .headers
        .iter()
        .any(|(k, v)| k == "authorization" && v == "Bearer sk-secret"));
    assert_eq!(request.body["model"], "gpt-4o");
    assert_eq!(request.body["messages"][0]["content"], "hi");
}

#[tokio::test]
async fn missing_content_yields_none_not_error() {
    let body = json!({"choices": [{"message": {"role": "assistant"}, "finish_reason": "stop"}]})
        .to_string();
    let provider = OpenAiCompatProvider::new(
        "https://api.example.test/v1",
        None,
        "gpt-4o",
        FakeTransport::new(200, &body),
    );

    let response = provider
        .complete(&request(vec![json!({"role": "user", "content": "hi"})]))
        .await
        .unwrap();
    assert_eq!(response.content, None);
    assert_eq!(response.reasoning_content, None);
    assert_eq!(response.finish_reason, "stop");
}

#[tokio::test]
async fn reasoning_content_is_parsed_alongside_content() {
    let body = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "答案是 4",
                "reasoning_content": "2 加 2 等于 4"
            },
            "finish_reason": "stop"
        }]
    })
    .to_string();
    let provider = OpenAiCompatProvider::new(
        "https://api.example.test/v1",
        None,
        "deepseek-reasoner",
        FakeTransport::new(200, &body),
    );

    let response = provider
        .complete(&request(vec![json!({"role": "user", "content": "2+2"})]))
        .await
        .unwrap();
    assert_eq!(response.content.as_deref(), Some("答案是 4"));
    assert_eq!(response.reasoning_content.as_deref(), Some("2 加 2 等于 4"));
}

async fn provider_error(status: u16, body: &str) -> ProviderError {
    let provider = OpenAiCompatProvider::new(
        "https://api.example.test/v1",
        Some("sk".to_string()),
        "gpt-4o",
        FakeTransport::new(status, body),
    );
    provider
        .complete(&request(vec![json!({"role": "user", "content": "hi"})]))
        .await
        .unwrap_err()
}

#[tokio::test]
async fn classifies_http_error_statuses() {
    assert!(matches!(
        provider_error(401, "unauthorized").await,
        ProviderError::Auth { status: 401, .. }
    ));
    assert!(matches!(
        provider_error(403, "forbidden").await,
        ProviderError::Auth { status: 403, .. }
    ));
    assert!(matches!(
        provider_error(429, "slow down").await,
        ProviderError::RateLimited { status: 429, .. }
    ));
    assert!(matches!(
        provider_error(500, "boom").await,
        ProviderError::Server { status: 500, .. }
    ));
    assert!(matches!(
        provider_error(400, "bad request").await,
        ProviderError::Api { status: 400, .. }
    ));
}

#[tokio::test]
async fn transport_failure_is_structured() {
    let provider = OpenAiCompatProvider::new(
        "https://api.example.test/v1",
        None,
        "gpt-4o",
        FailingTransport,
    );
    let err = provider
        .complete(&request(vec![json!({"role": "user", "content": "hi"})]))
        .await
        .unwrap_err();
    assert!(matches!(err, ProviderError::Transport(_)));
}

#[tokio::test]
async fn malformed_and_missing_choices_are_response_errors() {
    assert!(matches!(
        provider_error(200, "not json at all").await,
        ProviderError::Response(_)
    ));
    assert!(matches!(
        provider_error(200, "{}").await,
        ProviderError::Response(_)
    ));
}

#[tokio::test]
async fn parse_response_extracts_tool_calls() {
    let body = json!({
        "choices": [{
            "message": {
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "read_file", "arguments": "{\"path\":\"a.txt\"}"}
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
    .to_string();
    let response = HttpResponse { status: 200, body };

    let parsed = parse_chat_response(&response).unwrap();

    assert_eq!(parsed.finish_reason, "tool_calls");
    assert!(parsed.content.is_none());
    assert_eq!(
        parsed.tool_calls,
        vec![ToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: "{\"path\":\"a.txt\"}".to_string(),
        }]
    );
}

#[tokio::test]
async fn parse_response_without_tool_calls_yields_empty_vec() {
    let body = json!({
        "choices": [{"message": {"content": "hi"}, "finish_reason": "stop"}]
    })
    .to_string();
    let response = HttpResponse { status: 200, body };

    let parsed = parse_chat_response(&response).unwrap();
    assert_eq!(parsed.content.as_deref(), Some("hi"));
    assert!(parsed.tool_calls.is_empty());
}
