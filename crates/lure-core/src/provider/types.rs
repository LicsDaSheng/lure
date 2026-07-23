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
}

impl LlmResponse {
    /// 构造一个仅含文本、`finish_reason = "stop"` 的响应。
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            reasoning_content: None,
            finish_reason: "stop".to_string(),
            usage: Map::new(),
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
}
