//! OpenAI-compatible provider。
//!
//! 对齐上游 `nanobot/providers/openai_compat_provider.py` 的核心请求/响应形状：
//! - 请求体：`{model, messages, temperature, max_tokens}`，POST 到 `{base}/chat/completions`。
//! - 响应：`choices[0].message.content` + `choices[0].finish_reason` + `usage`。
//! - 错误按 HTTP 状态分类为结构化 [`ProviderError`]。
//!
//! Phase 4 不做：`max_completion_tokens`/模型专属覆盖、streaming、tool call、
//! prompt caching、重试策略（见 upstream-test-ledger）。

use serde_json::{json, Map, Value};

use crate::provider::http::{HttpRequest, HttpResponse, HttpTransport};
use crate::provider::types::{
    CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError,
};

/// 基于 [`HttpTransport`] 的 OpenAI-compatible provider。
pub struct OpenAiCompatProvider<T: HttpTransport> {
    base_url: String,
    api_key: Option<String>,
    model: String,
    transport: T,
}

impl<T: HttpTransport> OpenAiCompatProvider<T> {
    /// 绑定 base URL、可选 api key、默认模型与传输。
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        transport: T,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            transport,
        }
    }
}

impl<T: HttpTransport> LlmProvider for OpenAiCompatProvider<T> {
    fn default_model(&self) -> &str {
        &self.model
    }

    fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        let body = build_chat_request(&request.model, &request.messages, &request.settings);
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        if let Some(key) = &self.api_key {
            headers.push(("authorization".to_string(), format!("Bearer {key}")));
        }

        let http_request = HttpRequest { url, headers, body };
        let response = self
            .transport
            .post_json(&http_request)
            .map_err(ProviderError::Transport)?;

        parse_chat_response(&response)
    }
}

/// 构建 OpenAI-compatible chat completions 请求体。
pub fn build_chat_request(model: &str, messages: &[Value], settings: &GenerationSettings) -> Value {
    json!({
        "model": model,
        "messages": messages,
        "temperature": settings.temperature,
        "max_tokens": settings.max_tokens.max(1),
    })
}

/// 按状态码分类并解析 chat completions 响应。
pub fn parse_chat_response(response: &HttpResponse) -> Result<LlmResponse, ProviderError> {
    let status = response.status;
    let body = &response.body;
    match status {
        200..=299 => {}
        401 | 403 => {
            return Err(ProviderError::Auth {
                status,
                message: snippet(body),
            })
        }
        429 => {
            return Err(ProviderError::RateLimited {
                status,
                message: snippet(body),
            })
        }
        500..=599 => {
            return Err(ProviderError::Server {
                status,
                message: snippet(body),
            })
        }
        _ => {
            return Err(ProviderError::Api {
                status,
                message: snippet(body),
            })
        }
    }

    let value: Value = serde_json::from_str(body)
        .map_err(|e| ProviderError::Response(format!("响应不是合法 JSON: {e}")))?;

    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| ProviderError::Response("响应缺少 choices".to_string()))?;

    let content = choice
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop")
        .to_string();

    let usage = value
        .get("usage")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);

    Ok(LlmResponse {
        content,
        finish_reason,
        usage,
    })
}

/// 截断过长的错误响应体，避免污染错误消息。
fn snippet(body: &str) -> String {
    const MAX: usize = 500;
    if body.chars().count() <= MAX {
        return body.to_string();
    }
    let head: String = body.chars().take(MAX).collect();
    format!("{head}…")
}
