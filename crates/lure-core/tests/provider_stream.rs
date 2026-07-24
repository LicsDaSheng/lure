//! Provider 侧真实流式（SSE）消费：解析 `data:` 增量、逐增量回调、组装最终 LlmResponse。
//!
//! 用假流式传输逐行喂 SSE，不触网。对齐 `api::sse_chunks` 的 server 侧生成格式。

use std::cell::RefCell;

use lure_core::provider::{
    parse_sse_line, CompletionRequest, GenerationSettings, HttpRequest, HttpResponse,
    HttpTransport, LlmProvider, OpenAiCompatProvider, ProviderError, StreamChunk,
};
use serde_json::json;

/// 假流式传输：按行回放预置 SSE，返回预置状态码。
struct FakeStreamTransport {
    status: u16,
    lines: Vec<String>,
}

impl HttpTransport for FakeStreamTransport {
    fn post_json(&self, _request: &HttpRequest) -> Result<HttpResponse, String> {
        Ok(HttpResponse {
            status: self.status,
            body: self.lines.join("\n"),
        })
    }

    fn post_json_streaming(
        &self,
        _request: &HttpRequest,
        on_line: &mut dyn FnMut(&str),
    ) -> Result<u16, String> {
        for line in &self.lines {
            on_line(line);
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

#[test]
fn parse_sse_line_extracts_content_delta() {
    let line = r#"data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
    let chunk = parse_sse_line(line).unwrap().unwrap();
    assert_eq!(chunk.content_delta.as_deref(), Some("Hello"));
    assert!(chunk.finish_reason.is_none());
}

#[test]
fn parse_sse_line_done_and_non_data_yield_none() {
    assert!(parse_sse_line("data: [DONE]").unwrap().is_none());
    assert!(parse_sse_line(": keep-alive").unwrap().is_none());
    assert!(parse_sse_line("").unwrap().is_none());
}

#[test]
fn parse_sse_line_reads_finish_reason() {
    let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    let chunk = parse_sse_line(line).unwrap().unwrap();
    assert!(chunk.content_delta.is_none());
    assert_eq!(chunk.finish_reason.as_deref(), Some("stop"));
}

#[test]
fn complete_streaming_delivers_deltas_in_order_and_assembles_response() {
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

    let deltas = RefCell::new(Vec::<String>::new());
    let response = provider
        .complete_streaming(&request(), &mut |chunk: &StreamChunk| {
            if let Some(text) = &chunk.content_delta {
                deltas.borrow_mut().push(text.clone());
            }
        })
        .unwrap();

    assert_eq!(deltas.borrow().as_slice(), &["Hello", " world"]);
    assert_eq!(response.content.as_deref(), Some("Hello world"));
    assert_eq!(response.finish_reason, "stop");
}

#[test]
fn complete_streaming_assembles_tool_calls_across_chunks() {
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
        .unwrap();

    assert_eq!(response.finish_reason, "tool_calls");
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id, "call_1");
    assert_eq!(response.tool_calls[0].name, "echo");
    assert_eq!(response.tool_calls[0].arguments, r#"{"text":"hi"}"#);
}

#[test]
fn complete_streaming_classifies_http_error_status() {
    let transport = FakeStreamTransport {
        status: 401,
        lines: vec![r#"{"error":"bad key"}"#.to_string()],
    };
    let provider = OpenAiCompatProvider::new("https://api.test/v1", None, "gpt-4o", transport);

    let err = provider
        .complete_streaming(&request(), &mut |_chunk| {})
        .unwrap_err();
    assert!(matches!(err, ProviderError::Auth { status: 401, .. }));
}
