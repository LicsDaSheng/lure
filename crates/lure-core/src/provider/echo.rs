//! `EchoProvider`：Phase 3 占位 provider。
//!
//! 回显最近一条 user 消息内容，使 CLI one-shot 闭环在没有真实 LLM 时也可运行。
//! Phase 4 引入 registry 与真实 provider 后，此占位仅用于本地/测试。

use serde_json::Value;

use crate::provider::types::{CompletionRequest, LlmProvider, LlmResponse, ProviderError};

/// 回显式占位 provider。
#[derive(Debug, Clone)]
pub struct EchoProvider {
    model: String,
}

impl EchoProvider {
    /// 新建占位 provider，默认模型标识为 `echo`。
    pub fn new() -> Self {
        Self {
            model: "echo".to_string(),
        }
    }
}

impl Default for EchoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LlmProvider for EchoProvider {
    fn default_model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: &CompletionRequest) -> Result<LlmResponse, ProviderError> {
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        Ok(LlmResponse::text(format!("echo: {last_user}")))
    }
}
