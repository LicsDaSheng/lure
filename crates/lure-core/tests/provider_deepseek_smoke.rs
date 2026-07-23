//! 真实 provider smoke（显式 opt-in）。
//!
//! 仅当设置了 `DEEPSEEK_API_KEY` 时才真正出网；未设置则跳过（保持默认套件不触网）。
//! 运行：`DEEPSEEK_API_KEY=... cargo test --test provider_deepseek_smoke -- --nocapture`

use lure_core::provider::{
    match_provider, CompletionRequest, GenerationSettings, LlmProvider, OpenAiCompatProvider,
    ProviderError, UreqTransport,
};
use serde_json::json;

const DEFAULT_MODEL: &str = "deepseek-v4-pro";

#[test]
fn deepseek_chat_completion_round_trip() {
    let Ok(api_key) = std::env::var("DEEPSEEK_API_KEY") else {
        eprintln!("跳过：未设置 DEEPSEEK_API_KEY");
        return;
    };
    // 可用 DEEPSEEK_SMOKE_MODEL 覆盖模型（如 deepseek-reasoner 验证 reasoning_content）。
    let model = std::env::var("DEEPSEEK_SMOKE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    let spec = match_provider(&model, "auto").expect("deepseek 应可匹配");
    eprintln!(
        "provider={} base={} model={model}",
        spec.name, spec.default_api_base
    );

    let provider = OpenAiCompatProvider::new(
        spec.default_api_base,
        Some(api_key),
        &model,
        UreqTransport::new(),
    );

    let request = CompletionRequest {
        model: model.clone(),
        messages: vec![json!({"role": "user", "content": "只回复两个字：你好"})],
        settings: GenerationSettings {
            temperature: 0.1,
            max_tokens: 512,
            reasoning_effort: None,
        },
    };

    match provider.complete(&request) {
        Ok(response) => {
            eprintln!(
                "成功 finish={} content={:?} reasoning_len={:?}",
                response.finish_reason,
                response.content,
                response
                    .reasoning_content
                    .as_ref()
                    .map(|r| r.chars().count()),
            );
            assert!(response.content.is_some(), "成功响应应含 content");
        }
        // 传输层失败视为 smoke 失败；结构化 API 错误（如模型名无效）说明 HTTP 往返成功。
        Err(ProviderError::Transport(msg)) => panic!("传输失败: {msg}"),
        Err(other) => eprintln!("HTTP 往返成功但 API 返回结构化错误: {other}"),
    }
}
