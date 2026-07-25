//! Provider 契约类型：请求、响应、生成参数、错误与 trait。
//!
//! 对齐上游 `nanobot/providers/base.py` 的核心形状（`LLMResponse`、
//! `GenerationSettings`、`LLMProvider`）。
//!
//! Phase 3 采用**同步** trait 且不含 tool call / streaming：上游 provider 为
//! async 且支持 streaming 与 tool call，这些留待 Phase 4/5/7 落地时收敛
//! （见 upstream-test-ledger）。

use std::fmt;

use serde_json::{Map, Value};

/// 生成参数，默认对齐上游 `GenerationSettings`。
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationSettings {
    /// 采样温度。
    pub temperature: f64,
    /// 单次生成最大 token。
    pub max_tokens: u32,
    /// 推理力度（部分模型支持）。
    pub reasoning_effort: Option<String>,
}

impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            max_tokens: 4096,
            reasoning_effort: None,
        }
    }
}

/// 一次补全请求。
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// 目标模型标识。
    pub model: String,
    /// 消息序列（`{role, content, ...}` 对象）。
    pub messages: Vec<Value>,
    /// 生成参数。
    pub settings: GenerationSettings,
}

/// provider 请求的一次工具调用（OpenAI function-calling 形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// tool_call id（回灌 tool 结果时用作 `tool_call_id`）。
    pub id: String,
    /// 目标工具名。
    pub name: String,
    /// 原始参数 JSON 字符串（OpenAI 以字符串返回；由调用方解析）。
    pub arguments: String,
}

/// 流式 tool_call 增量（OpenAI streaming `delta.tool_calls` 单项，按 `index` 累积）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallDelta {
    /// 该 tool_call 在本次响应中的序号。
    pub index: usize,
    /// tool_call id（通常只在首个增量出现）。
    pub id: Option<String>,
    /// 工具名（通常只在首个增量出现）。
    pub name: Option<String>,
    /// 参数片段（跨增量拼接为完整 JSON 字符串）。
    pub arguments: Option<String>,
}

/// 一条流式增量（OpenAI SSE `choices[0].delta` 的解析结果）。
///
/// 注：`usage` 为 `serde_json::Map`（`Value` 不实现 `Eq`），故本类型只派生 `PartialEq`。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreamChunk {
    /// 内容文本增量。
    pub content_delta: Option<String>,
    /// 推理内容增量。
    pub reasoning_delta: Option<String>,
    /// tool_call 增量。
    pub tool_call_deltas: Vec<ToolCallDelta>,
    /// 结束原因（通常只在末尾出现）。
    pub finish_reason: Option<String>,
    /// 顶层 usage（OpenAI `include_usage` 末帧携带，此时 `choices` 为空）。
    pub usage: Map<String, Value>,
}

/// provider 返回的补全响应。
#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    /// 文本内容（可能为空）。
    pub content: Option<String>,
    /// 推理内容（思维链）；推理模型如 deepseek-reasoner / DeepSeek-R1 / Kimi 提供。
    pub reasoning_content: Option<String>,
    /// 结束原因，默认 `stop`。
    pub finish_reason: String,
    /// 用量统计。
    pub usage: Map<String, Value>,
    /// 请求的工具调用（无则为空）。
    pub tool_calls: Vec<ToolCall>,
}

impl LlmResponse {
    /// 构造一个仅含文本、`finish_reason = "stop"` 的响应。
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            reasoning_content: None,
            finish_reason: "stop".to_string(),
            usage: Map::new(),
            tool_calls: Vec::new(),
        }
    }
}

/// provider 结构化错误。
#[derive(Debug)]
pub enum ProviderError {
    /// 请求构造或参数错误。
    Request(String),
    /// 传输/网络层失败。
    Transport(String),
    /// 认证或授权失败（401/403）。
    Auth { status: u16, message: String },
    /// 触发限流（429）。
    RateLimited { status: u16, message: String },
    /// 服务端错误（5xx）。
    Server { status: u16, message: String },
    /// 其他非 2xx 响应。
    Api { status: u16, message: String },
    /// 响应解析或语义失败。
    Response(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Request(msg) => write!(f, "provider 请求失败: {msg}"),
            ProviderError::Transport(msg) => write!(f, "provider 传输失败: {msg}"),
            ProviderError::Auth { status, message } => {
                write!(f, "provider 认证失败({status}): {message}")
            }
            ProviderError::RateLimited { status, message } => {
                write!(f, "provider 限流({status}): {message}")
            }
            ProviderError::Server { status, message } => {
                write!(f, "provider 服务端错误({status}): {message}")
            }
            ProviderError::Api { status, message } => {
                write!(f, "provider API 错误({status}): {message}")
            }
            ProviderError::Response(msg) => write!(f, "provider 响应失败: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// LLM provider 契约。
///
/// Phase 3 仅需 `default_model` 与同步 `complete`；registry、fallback、真实
/// OpenAI-compatible 调用属 Phase 4。
pub trait LlmProvider {
    /// 默认模型标识。
    fn default_model(&self) -> &str;

    /// 执行一次补全。
    fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError>;

    /// 执行一次**流式**补全：每产生一个增量调用 `on_delta`，返回组装后的完整响应。
    ///
    /// 默认实现回退到非流式 [`complete`](Self::complete)，并把整段内容作为**单个**增量
    /// 回调一次——让所有 provider 都可被 streaming 调用路径统一驱动；支持真实 SSE 的
    /// provider（如 OpenAI-compatible）覆盖此方法以逐 token 回调。
    fn complete_streaming(
        &self,
        request: &CompletionRequest,
        on_delta: &mut dyn FnMut(&StreamChunk),
    ) -> Result<LlmResponse, ProviderError> {
        let response = self.complete(request)?;
        if let Some(content) = response.content.as_ref().filter(|c| !c.is_empty()) {
            on_delta(&StreamChunk {
                content_delta: Some(content.clone()),
                finish_reason: Some(response.finish_reason.clone()),
                ..StreamChunk::default()
            });
        }
        Ok(response)
    }
}
