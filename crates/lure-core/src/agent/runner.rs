//! 单次 provider 调用的 `AgentRunner`。
//!
//! 对齐上游 `nanobot/agent/runner.py` 的职责边界：把构建好的消息交给 provider
//! 执行一次补全，并把当前工具 function schemas 随请求发送；tool 执行循环由 `AgentLoop`
//! 负责。

use serde_json::Value;

use crate::provider::{
    CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError, StreamChunk,
};

/// 单次补全 runner。
pub struct AgentRunner<'a> {
    provider: &'a dyn LlmProvider,
    settings: GenerationSettings,
    tools: Vec<Value>,
}

impl<'a> AgentRunner<'a> {
    /// 绑定 provider 与生成参数。
    pub fn new(provider: &'a dyn LlmProvider, settings: GenerationSettings) -> Self {
        Self {
            provider,
            settings,
            tools: Vec::new(),
        }
    }

    /// 为本次 provider 请求挂载 function-calling schema。
    pub fn with_tools(mut self, tools: Vec<Value>) -> Self {
        self.tools = tools;
        self
    }

    /// 用给定 model 与消息执行一次补全。
    pub async fn run(
        &self,
        model: &str,
        messages: Vec<Value>,
    ) -> Result<LlmResponse, ProviderError> {
        let request = CompletionRequest {
            model: model.to_string(),
            messages,
            settings: self.settings.clone(),
            tools: self.tools.clone(),
        };
        self.provider.complete(&request).await
    }

    /// 用给定 model 与消息执行一次**流式**补全：每个增量回调 `on_delta`。
    ///
    /// provider 未覆盖流式时回退为单块回调（见 `LlmProvider::complete_streaming`）。
    pub async fn run_streaming(
        &self,
        model: &str,
        messages: Vec<Value>,
        on_delta: &mut (dyn FnMut(StreamChunk) + Send),
    ) -> Result<LlmResponse, ProviderError> {
        let request = CompletionRequest {
            model: model.to_string(),
            messages,
            settings: self.settings.clone(),
            tools: self.tools.clone(),
        };
        self.provider.complete_streaming(&request, on_delta).await
    }
}
