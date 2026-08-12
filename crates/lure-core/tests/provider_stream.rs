//! Provider 侧真实流式（SSE）消费：解析 `data:` 增量、逐增量回调、组装最终 LlmResponse。
//!
//! 用假流式传输逐行喂 SSE，不触网。对齐 `api::sse_chunks` 的 server 侧生成格式。

use std::sync::{Arc, Mutex};

use lure_core::provider::{
    parse_sse_line, CompletionRequest, GenerationSettings, HttpRequest, HttpResponse,
    HttpTransport, LlmProvider, OpenAiCompatProvider, ProviderError, StreamChunk,
};
use serde_json::{json, Value};

/// 假流式传输：按行回放预置 SSE，返回预置状态码。
struct FakeStreamTransport {
    status: u16,
    lines: Vec<String>,
}

#[async_trait::async_trait]
impl HttpTransport for FakeStreamTransport {
    async fn post_json(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
        Ok(HttpResponse {
            status: self.status,
            body: self.lines.join("\n"),
        })
    }

    async fn post_json_streaming(
        &self,
        _request: &HttpRequest,
        on_line: &mut (dyn FnMut(String) + Send),
    ) -> Result<u16, String> {
        for line in &self.lines {
            on_line(line.to_string());
        }
        Ok(self.status)
    }
}

fn request() -> CompletionRequest {
    CompletionRequest {
        model: "gpt-4o".to_string(),
        messages: vec![json!({"role": "user", "content": "hi"})],
        settings: GenerationSettings::default(),
    }
}

#[tokio::test]
async fn parse_sse_line_extracts_content_delta() {
    let line = r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
    let chunk = parse_sse_line(line).unwrap().unwrap();
    assert_eq!(chunk.content_delta.as_deref(), Some("Hello"));
    assert!(chunk.finish_reason.is_none());
}

#[tokio::test]
async fn parse_sse_line_done_and_non_data_yield_none() {
    assert!(parse_sse_line("data: [DONE]").unwrap().is_none());
    assert!(parse_sse_line(": keep-alive").unwrap().is_none());
    assert!(parse_sse_line("").unwrap().is_none());
}

#[tokio::test]
async fn parse_sse_line_reads_finish_reason() {
    let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    let chunk = parse_sse_line(line).unwrap().unwrap();
    assert!(chunk.content_delta.is_none());
    assert_eq!(chunk.finish_reason.as_deref(), Some("stop"));
}

#[tokio::test]
async fn complete_streaming_delivers_deltas_in_order_and_assembles_response() {
    let transport = FakeStreamTransport {
        status: 200,
        lines: vec![
            r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#.to_string(),
            r#"data: {"choices":[{"delta":{"content":" world"},"finish_reason":null}]}"#
                .to_string(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.to_string(),
            "data: [DONE]".to_string(),
        ],
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    let deltas = Mutex::new(Vec::<String>::new());
    let response = provider
        .complete_streaming(&request(), &mut |chunk: StreamChunk| {
            if let Some(text) = &chunk.content_delta {
                deltas.lock().unwrap().push(text.clone());
            }
        })
        .await
        .unwrap();

    assert_eq!(deltas.lock().unwrap().as_slice(), &["Hello", " world"]);
    assert_eq!(response.content.as_deref(), Some("Hello world"));
    assert_eq!(response.finish_reason, "stop");
}

#[tokio::test]
async fn complete_streaming_assembles_tool_calls_across_chunks() {
    let transport = FakeStreamTransport {
        status: 200,
        lines: vec![
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"{\"te"}}]}}]}"#.to_string(),
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"xt\":\"hi\"}"}}]}}]}"#.to_string(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#.to_string(),
            "data: [DONE]".to_string(),
        ],
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    let response = provider
        .complete_streaming(&request(), &mut |_chunk| {})
        .await
        .unwrap();

    assert_eq!(response.finish_reason, "tool_calls");
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id, "call_1");
    assert_eq!(response.tool_calls[0].name, "echo");
    assert_eq!(response.tool_calls[0].arguments, r#"{"text":"hi"}"#);
}

#[tokio::test]
async fn parse_sse_line_extracts_usage_from_usage_only_chunk() {
    // OpenAI `include_usage` 末帧：choices 为空，usage 在顶层。
    let line = r#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#;
    let chunk = parse_sse_line(line).unwrap().unwrap();
    assert_eq!(
        chunk.usage.get("prompt_tokens").and_then(Value::as_i64),
        Some(12)
    );
    assert_eq!(
        chunk.usage.get("total_tokens").and_then(Value::as_i64),
        Some(17)
    );
    assert!(chunk.content_delta.is_none());
}

#[tokio::test]
async fn complete_streaming_captures_usage_from_final_chunk() {
    let transport = FakeStreamTransport {
        status: 200,
        lines: vec![
            r#"data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#.to_string(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.to_string(),
            r#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#.to_string(),
            "data: [DONE]".to_string(),
        ],
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    let response = provider
        .complete_streaming(&request(), &mut |_chunk| {})
        .await
        .unwrap();

    assert_eq!(response.content.as_deref(), Some("Hi"));
    assert_eq!(response.finish_reason, "stop");
    assert_eq!(
        response.usage.get("prompt_tokens").and_then(Value::as_i64),
        Some(12)
    );
    assert_eq!(
        response
            .usage
            .get("completion_tokens")
            .and_then(Value::as_i64),
        Some(5)
    );
    assert_eq!(
        response.usage.get("total_tokens").and_then(Value::as_i64),
        Some(17)
    );
}

#[tokio::test]
async fn complete_streaming_normalizes_nested_cached_tokens() {
    // 末帧 usage 用嵌套 prompt_tokens_details.cached_tokens；归一后应见顶层 cached_tokens。
    let transport = FakeStreamTransport {
        status: 200,
        lines: vec![
            r#"data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}"#.to_string(),
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#.to_string(),
            r#"data: {"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":10,"total_tokens":110,"prompt_tokens_details":{"cached_tokens":80}}}"#.to_string(),
            "data: [DONE]".to_string(),
        ],
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    let response = provider
        .complete_streaming(&request(), &mut |_chunk| {})
        .await
        .unwrap();

    assert_eq!(
        response.usage.get("cached_tokens").and_then(Value::as_i64),
        Some(80)
    );
    assert_eq!(
        response.usage.get("prompt_tokens").and_then(Value::as_i64),
        Some(100)
    );
}

/// 记录请求 body 的流式传输（验证 stream_options）。
struct RecordingStreamTransport {
    seen: Arc<Mutex<Option<Value>>>,
}

#[async_trait::async_trait]
impl HttpTransport for RecordingStreamTransport {
    async fn post_json(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
        Ok(HttpResponse {
            status: 200,
            body: String::new(),
        })
    }

    async fn post_json_streaming(
        &self,
        request: &HttpRequest,
        _on_line: &mut (dyn FnMut(String) + Send),
    ) -> Result<u16, String> {
        *self.seen.lock().unwrap() = Some(request.body.clone());
        Ok(200)
    }
}

#[tokio::test]
async fn streaming_request_includes_stream_options_include_usage() {
    // 对齐上游 openai_compat_provider：流式请求带 stream_options.include_usage=true，
    // 上游 LLM 才会在末帧回传 usage。
    let seen = Arc::new(Mutex::new(None));
    let transport = RecordingStreamTransport {
        seen: Arc::clone(&seen),
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    provider
        .complete_streaming(&request(), &mut |_chunk| {})
        .await
        .unwrap();

    let body = seen.lock().unwrap().clone().expect("应记录请求 body");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
}

#[tokio::test]
async fn complete_streaming_classifies_http_error_status() {
    let transport = FakeStreamTransport {
        status: 401,
        lines: vec![r#"{"error":"bad key"}"#.to_string()],
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    let err = provider
        .complete_streaming(&request(), &mut |_chunk| {})
        .await
        .unwrap_err();
    assert!(matches!(err, ProviderError::Auth { status: 401, .. }));
}
