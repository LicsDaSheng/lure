//! 最小 `AgentRunner`。
//!
//! 对齐上游 `nanobot/agent/runner.py` 的职责边界：把构建好的消息交给 provider
//! 执行一次补全。Phase 3 只做单次调用，无 tool 执行循环、无 streaming、无重试
//! 策略——这些属 Phase 4/5。

use serde_json::Value;

use crate::provider::{
    CompletionRequest, GenerationSettings, LlmProvider, LlmResponse, ProviderError, StreamChunk,
};

/// 单次补全 runner。
pub struct AgentRunner<'a> {
    provider: &'a dyn LlmProvider,
    settings: GenerationSettings,
}

impl<'a> AgentRunner<'a> {
    /// 绑定 provider 与生成参数。
    pub fn new(provider: &'a dyn LlmProvider, settings: GenerationSettings) -> Self {
        Self { provider, settings }
    }

    /// 用给定 model 与消息执行一次补全。
    pub fn run(&self, model: &str, messages: Vec<Value>) -> Result<LlmResponse, ProviderError> {
        let request = CompletionRequest {
            model: model.to_string(),
            messages,
            settings: self.settings.clone(),
        };
        self.provider.complete(&request)
    }

    /// 用给定 model 与消息执行一次**流式**补全：每个增量回调 `on_delta`。
    ///
    /// provider 未覆盖流式时回退为单块回调（见 `LlmProvider::complete_streaming`）。
    pub fn run_streaming(
        &self,
        model: &str,
        messages: Vec<Value>,
        on_delta: &mut dyn FnMut(&StreamChunk),
    ) -> Result<LlmResponse, ProviderError> {
        let request = CompletionRequest {
            model: model.to_string(),
            messages,
            settings: self.settings.clone(),
        };
        self.provider.complete_streaming(&request, on_delta)
    }
}
