//! OpenAI-compatible API 表面（传输无关）。
//!
//! 对齐上游 `nanobot/api/server.py` 的请求/响应契约，但与具体 HTTP 服务解耦：
//! - `parse_chat_request`：校验 messages（恰好一条 user）并抽取文本。
//! - `validate_model`：请求 model 与配置不符返回 400。
//! - `authorize`：未配置 key 放行；配置后校验 `Bearer <key>`。
//! - `chat_completion_response` / `error_body`：OpenAI JSON 形状。
//! - `sse_chunks`：streaming 事件序列（chunk… + `[DONE]`）。
//!
//! Phase 9 不做：真实 HTTP server、/v1/models、media 上传、并发 session lock、
//! 真实 agent 接线（见 upstream-test-ledger）。

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

/// API 固定 session key。
pub const API_SESSION_KEY: &str = "api:default";

/// API 层结构化错误（携带 HTTP 状态）。
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    /// HTTP 状态码。
    pub status: u16,
    /// 错误消息。
    pub message: String,
    /// 错误类型。
    pub err_type: String,
}

impl ApiError {
    /// 构造 `invalid_request_error` 类型的错误。
    pub fn invalid_request(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            err_type: "invalid_request_error".to_string(),
        }
    }

    /// 转为 OpenAI 错误响应体：`{error:{message,type,code}}`。
    pub fn body(&self) -> Value {
        error_body(self.status, &self.message, &self.err_type)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "API 错误({}): {}", self.status, self.message)
    }
}

impl std::error::Error for ApiError {}

/// 构造 OpenAI 错误响应体。
pub fn error_body(status: u16, message: &str, err_type: &str) -> Value {
    json!({"error": {"message": message, "type": err_type, "code": status}})
}

/// 解析后的 chat 请求。
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChatRequest {
    /// 用户消息文本。
    pub text: String,
    /// 请求指定的 model（可选）。
    pub model: Option<String>,
    /// 是否 streaming。
    pub stream: bool,
}

/// 校验并解析 chat completions 请求体。
///
/// 契约：`messages` 必须是恰好一条 role 为 `user` 的消息；content 可为字符串或
/// 多模态 parts（抽取其中的 text）。
pub fn parse_chat_request(body: &Value) -> Result<ParsedChatRequest, ApiError> {
    let messages = body.get("messages").and_then(Value::as_array);
    let single = match messages {
        Some(list) if list.len() == 1 => &list[0],
        _ => {
            return Err(ApiError::invalid_request(
                400,
                "Only a single user message is supported",
            ))
        }
    };

    if single.get("role").and_then(Value::as_str) != Some("user") {
        return Err(ApiError::invalid_request(
            400,
            "Only a single user message is supported",
        ));
    }

    let text = extract_text(single.get("content"));
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    Ok(ParsedChatRequest {
        text,
        model,
        stream,
    })
}

/// 从 content 抽取文本：字符串直接返回；数组抽取 `type == "text"` 的部分。
fn extract_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// 请求 model 与配置不符时返回 400。
pub fn validate_model(request_model: Option<&str>, configured: &str) -> Result<(), ApiError> {
    match request_model {
        Some(model) if model != configured => Err(ApiError::invalid_request(
            400,
            format!("model '{model}' is not available"),
        )),
        _ => Ok(()),
    }
}

/// 鉴权：未配置 key 放行；配置后要求 `Bearer <key>`。
pub fn authorize(configured_key: Option<&str>, auth_header: Option<&str>) -> Result<(), ApiError> {
    let Some(key) = configured_key.filter(|k| !k.is_empty()) else {
        return Ok(());
    };
    let Some(header) = auth_header else {
        return Err(ApiError {
            status: 401,
            message: "Missing Authorization header. Use: Bearer <api_key>".to_string(),
            err_type: "invalid_request_error".to_string(),
        });
    };
    match header.strip_prefix("Bearer ") {
        Some(token) if token == key => Ok(()),
        _ => Err(ApiError {
            status: 401,
            message: "Invalid API key".to_string(),
            err_type: "invalid_request_error".to_string(),
        }),
    }
}

/// 派生 API session key：`api:{id}` 或固定 [`API_SESSION_KEY`]。
pub fn api_session_key(session_id: Option<&str>) -> String {
    match session_id.filter(|s| !s.is_empty()) {
        Some(id) => format!("api:{id}"),
        None => API_SESSION_KEY.to_string(),
    }
}

/// 构造非 streaming 的 chat completion 响应体。
pub fn chat_completion_response(content: &str, model: &str, usage: &Map<String, Value>) -> Value {
    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let total = match usage.get("total_tokens").and_then(Value::as_i64) {
        Some(t) if t != 0 => t,
        _ => prompt + completion,
    };

    json!({
        "id": generate_completion_id(),
        "object": "chat.completion",
        "created": unix_seconds(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop",
        }],
        "usage": {
            "prompt_tokens": prompt,
            "completion_tokens": completion,
            "total_tokens": total,
        }
    })
}

/// 构造 `/v1/models` 响应体：单条已配置模型（对齐上游 `handle_models`）。
///
/// `owned_by` 固定为 `"nanobot"`，`created` 固定为 `0`。
pub fn models_response(model: &str) -> Value {
    json!({
        "object": "list",
        "data": [{
            "id": model,
            "object": "model",
            "created": 0,
            "owned_by": "nanobot",
        }]
    })
}

/// 构造 streaming SSE 事件序列：内容 chunk → finish chunk → `[DONE]`。
pub fn sse_chunks(content: &str, model: &str, chunk_id: &str) -> Vec<String> {
    vec![
        sse_chunk(content, model, chunk_id, None),
        sse_chunk("", model, chunk_id, Some("stop")),
        "data: [DONE]\n\n".to_string(),
    ]
}

/// 单条 OpenAI-compatible SSE chunk。
fn sse_chunk(delta: &str, model: &str, chunk_id: &str, finish_reason: Option<&str>) -> String {
    let delta_obj = if delta.is_empty() {
        json!({})
    } else {
        json!({"content": delta})
    };
    let payload = json!({
        "id": chunk_id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta_obj,
            "finish_reason": finish_reason,
        }]
    });
    format!(
        "data: {}\n\n",
        serde_json::to_string(&payload).expect("chunk 可序列化")
    )
}

/// 生成 `chatcmpl-<12 hex>` id。
pub fn generate_completion_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("chatcmpl-{:012x}", nanos & 0xffff_ffff_ffff)
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
