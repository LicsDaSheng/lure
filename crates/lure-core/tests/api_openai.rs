//! 映射上游 `tests/test_openai_api.py` 与 `test_api_stream.py` 的传输无关契约。

use lure_core::api::{
    api_session_key, authorize, chat_completion_response, error_body, parse_chat_request,
    sse_chunks, validate_model, ApiError, API_SESSION_KEY,
};
use serde_json::{json, Map, Value};

fn usage(pairs: &[(&str, i64)]) -> Map<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), json!(v)))
        .collect()
}

#[test]
fn error_body_shape() {
    let body = error_body(400, "bad request", "invalid_request_error");
    assert_eq!(body["error"]["message"], "bad request");
    assert_eq!(body["error"]["code"], 400);
    assert_eq!(body["error"]["type"], "invalid_request_error");
}

#[test]
fn chat_completion_response_shape_and_usage() {
    let result = chat_completion_response("hello world", "test-model", &Map::new());
    assert_eq!(result["object"], "chat.completion");
    assert_eq!(result["model"], "test-model");
    assert_eq!(result["choices"][0]["message"]["content"], "hello world");
    assert_eq!(result["choices"][0]["message"]["role"], "assistant");
    assert_eq!(result["choices"][0]["finish_reason"], "stop");
    assert!(result["id"].as_str().unwrap().starts_with("chatcmpl-"));
    assert_eq!(result["usage"]["total_tokens"], 0);

    // prompt + completion 求和。
    let with = chat_completion_response(
        "x",
        "m",
        &usage(&[("prompt_tokens", 150), ("completion_tokens", 42)]),
    );
    assert_eq!(with["usage"]["total_tokens"], 192);

    // 保留 provider 提供的 total。
    let total_only = chat_completion_response("x", "m", &usage(&[("total_tokens", 77)]));
    assert_eq!(total_only["usage"]["prompt_tokens"], 0);
    assert_eq!(total_only["usage"]["total_tokens"], 77);
}

#[test]
fn parse_request_requires_single_user_message() {
    // 缺 messages。
    let err = parse_chat_request(&json!({"model": "test"})).unwrap_err();
    assert_eq!(err.status, 400);

    // 只有 system。
    let err = parse_chat_request(&json!({"messages": [{"role": "system", "content": "bot"}]}))
        .unwrap_err();
    assert_eq!(err.status, 400);

    // 多条消息。
    let err = parse_chat_request(&json!({"messages": [
        {"role": "user", "content": "hi"},
        {"role": "assistant", "content": "prev"}
    ]}))
    .unwrap_err();
    assert_eq!(err.status, 400);

    // 合法单条 user。
    let parsed =
        parse_chat_request(&json!({"messages": [{"role": "user", "content": "hello"}]})).unwrap();
    assert_eq!(parsed.text, "hello");
    assert!(!parsed.stream);
    assert_eq!(parsed.model, None);
}

#[test]
fn parse_request_extracts_multimodal_text_and_stream_flag() {
    let parsed = parse_chat_request(&json!({
        "model": "m",
        "stream": true,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "part1 "},
            {"type": "image_url", "image_url": {"url": "http://x"}},
            {"type": "text", "text": "part2"}
        ]}]
    }))
    .unwrap();
    assert_eq!(parsed.text, "part1 part2");
    assert!(parsed.stream);
    assert_eq!(parsed.model.as_deref(), Some("m"));
}

#[test]
fn validate_model_rejects_mismatch() {
    assert!(validate_model(None, "test-model").is_ok());
    assert!(validate_model(Some("test-model"), "test-model").is_ok());
    let err = validate_model(Some("other-model"), "test-model").unwrap_err();
    assert_eq!(err.status, 400);
}

#[test]
fn authorize_allows_when_no_key_and_checks_bearer() {
    // 未配置 key：放行（含无 header）。
    assert!(authorize(None, None).is_ok());
    assert!(authorize(Some(""), None).is_ok());

    // 配置 key：需正确 Bearer。
    assert!(authorize(Some("secret"), Some("Bearer secret")).is_ok());
    assert_eq!(
        authorize(Some("secret"), Some("Bearer wrong"))
            .unwrap_err()
            .status,
        401
    );
    assert_eq!(authorize(Some("secret"), None).unwrap_err().status, 401);
}

#[test]
fn api_session_key_is_fixed_by_default() {
    assert_eq!(api_session_key(None), API_SESSION_KEY);
    assert_eq!(api_session_key(Some("")), API_SESSION_KEY);
    assert_eq!(api_session_key(Some("u1")), "api:u1");
}

#[test]
fn sse_chunks_have_ordered_content_finish_done() {
    let chunks = sse_chunks("hi", "test-model", "chatcmpl-abc");
    assert_eq!(chunks.len(), 3);

    // 首块：内容 delta，无 finish。
    assert!(chunks[0].starts_with("data: "));
    let first: Value = serde_json::from_str(chunks[0].trim_start_matches("data: ").trim()).unwrap();
    assert_eq!(first["object"], "chat.completion.chunk");
    assert_eq!(first["choices"][0]["delta"]["content"], "hi");
    assert!(first["choices"][0]["finish_reason"].is_null());

    // 次块：finish_reason stop。
    let second: Value =
        serde_json::from_str(chunks[1].trim_start_matches("data: ").trim()).unwrap();
    assert_eq!(second["choices"][0]["finish_reason"], "stop");

    // 末块：[DONE]。
    assert_eq!(chunks[2], "data: [DONE]\n\n");
}

#[test]
fn api_error_body_roundtrips() {
    let err = ApiError::invalid_request(400, "no messages");
    let body = err.body();
    assert_eq!(body["error"]["code"], 400);
    assert_eq!(body["error"]["message"], "no messages");
}
