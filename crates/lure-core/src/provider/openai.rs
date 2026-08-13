//! OpenAI-compatible provider。
//!
//! 对齐上游 `nanobot/providers/openai_compat_provider.py` 的核心请求/响应形状：
//! - 请求体：`{model, messages, temperature, max_tokens}`，POST 到 `{base}/chat/completions`。
//! - 响应：`choices[0].message.content` + `.tool_calls` + `choices[0].finish_reason` + `usage`。
//! - 错误按 HTTP 状态分类为结构化 [`ProviderError`]。
//!
//! `complete_streaming` 走 SSE：请求带 `stream: true`，逐行解析 `data:` 增量并回调，
//! 组装出完整响应（内容/推理拼接、tool_calls 按 index 累积）。
//!
//! Phase 4 不做：`max_completion_tokens`/模型专属覆盖、prompt caching、重试策略
//! （见 upstream-test-ledger）。tool_calls 已解析，供 agent tool-call 循环消费。

use serde_json::{json, Map, Value};

use crate::provider::http::{HttpRequest, HttpResponse, HttpTransport};
use crate::provider::stream::{parse_sse_line, StreamAssembler};
use crate::provider::types::{
    CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError, StreamChunk,
    ToolCall,
};
use crate::provider::usage::normalize_usage;

/// 基于 [`HttpTransport`] 的 OpenAI-compatible provider。
pub struct OpenAiCompatProvider<T: HttpTransport> {
    base_url: String,
    api_key: Option<String>,
    model: String,
    provider_name: Option<String>,
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
            provider_name: None,
            transport,
        }
    }

    /// 绑定 registry provider 名，用于应用 provider 专属请求参数。
    pub fn with_provider_name(mut self, provider_name: impl Into<String>) -> Self {
        self.provider_name = Some(provider_name.into());
        self
    }

    /// 构建 `/chat/completions` 请求（`stream` 控制是否要求 SSE）。
    fn http_request(&self, request: &CompletionRequest, stream: bool) -> HttpRequest {
        let mut body = build_chat_request(&request.model, &request.messages, &request.settings);
        apply_provider_reasoning(&mut body, self.provider_name.as_deref(), &request.settings);
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(request.tools.clone());
        }
        if stream {
            body["stream"] = Value::Bool(true);
            // 请求末帧回传 usage（对齐上游 openai_compat_provider）。
            body["stream_options"] = serde_json::json!({"include_usage": true});
        }
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
        if let Some(key) = &self.api_key {
            headers.push(("authorization".to_string(), format!("Bearer {key}")));
        }
        HttpRequest { url, headers, body }
    }
}

#[async_trait::async_trait]
impl<T: HttpTransport> LlmProvider for OpenAiCompatProvider<T> {
    fn default_model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        let http_request = self.http_request(request, false);
        let response = self
            .transport
            .post_json(&http_request)
            .await
            .map_err(ProviderError::Transport)?;

        parse_chat_response(&response)
    }

    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        on_delta: &mut (dyn FnMut(StreamChunk) + Send),
    ) -> Result<LlmResponse, ProviderError> {
        let http_request = self.http_request(request, true);

        let mut assembler = StreamAssembler::new();
        let mut raw = String::new();
        // 单行解析失败按容错忽略（SSE 常含 keep-alive/注释）；累积原始文本供错误分类。
        let status = self
            .transport
            .post_json_streaming(&http_request, &mut |line| {
                raw.push_str(&line);
                raw.push('\n');
                if let Ok(Some(chunk)) = parse_sse_line(&line) {
                    assembler.push(&chunk);
                    on_delta(chunk);
                }
            })
            .await
            .map_err(ProviderError::Transport)?;

        if let Some(error) = status_error(status, &raw) {
            return Err(error);
        }
        Ok(assembler.finish())
    }
}

/// 非 2xx 状态映射为结构化错误；2xx 返回 `None`。
fn status_error(status: u16, body: &str) -> Option<ProviderError> {
    match status {
        200..=299 => None,
        401 | 403 => Some(ProviderError::Auth {
            status,
            message: snippet(body),
        }),
        429 => Some(ProviderError::RateLimited {
            status,
            message: snippet(body),
        }),
        500..=599 => Some(ProviderError::Server {
            status,
            message: snippet(body),
        }),
        _ => Some(ProviderError::Api {
            status,
            message: snippet(body),
        }),
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

/// 把通用 reasoning 设置映射为 provider 原生的请求形状。
///
/// DeepSeek V4 默认开启思考；`reasoning_effort = "none"` 必须显式发送
/// `thinking.type = "disabled"` 才能切换到非思考模式。关闭时不发送
/// `reasoning_effort`，因为 DeepSeek 只在思考模式接受该力度参数。
fn apply_provider_reasoning(
    body: &mut Value,
    provider_name: Option<&str>,
    settings: &GenerationSettings,
) {
    if !provider_name.is_some_and(|name| name.eq_ignore_ascii_case("deepseek")) {
        return;
    }
    if settings
        .reasoning_effort
        .as_deref()
        .is_some_and(|effort| effort.eq_ignore_ascii_case("none"))
    {
        body["thinking"] = json!({"type": "disabled"});
    }
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

    let message = choice.get("message");
    let content = message
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let reasoning_content = message
        .and_then(|m| m.get("reasoning_content"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let tool_calls = parse_tool_calls(message);

    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop")
        .to_string();

    let raw_usage = value
        .get("usage")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_else(Map::new);
    let usage = normalize_usage(&raw_usage);

    Ok(LlmResponse {
        content,
        reasoning_content,
        finish_reason,
        usage,
        tool_calls,
    })
}

/// 从 `message.tool_calls` 解析工具调用；缺失或非数组时返回空。
fn parse_tool_calls(message: Option<&Value>) -> Vec<ToolCall> {
    message
        .and_then(|m| m.get("tool_calls"))
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let function = call.get("function")?;
                    let name = function.get("name").and_then(Value::as_str)?;
                    let id = call.get("id").and_then(Value::as_str).unwrap_or_default();
                    let arguments = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    Some(ToolCall {
                        id: id.to_string(),
                        name: name.to_string(),
                        arguments: arguments.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
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
