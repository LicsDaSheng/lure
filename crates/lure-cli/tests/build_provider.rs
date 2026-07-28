//! `build_provider`：为 dream 等场景解析出与 chat 同源的独立 provider 实例。
//!
//! 与 `build_agent_loop` 同语义：`--model echo` 显式返回离线 EchoProvider；
//! 否则从 config/preset/model 解析出真实 provider。desktop dream 复用此函数，
//! 使记忆整合走真实 LLM 而非离线 Echo。

use std::fs;

use lure_cli::build_provider;
use tempfile::tempdir;

#[test]
fn echo_model_returns_offline_echo_provider() {
    // `--model echo` 无需 config/api key，直接产出 echo（default_model == "echo"）。
    let provider = build_provider(None, None, Some("echo")).unwrap();
    assert_eq!(provider.default_model(), "echo");
}

#[test]
fn preset_and_model_are_mutually_exclusive() {
    // Box<dyn LlmProvider> 无 Debug，不能用 unwrap_err，手动解构。
    let err = match build_provider(None, Some("fast"), Some("gpt-4")) {
        Ok(_) => panic!("preset 与 model 同时给出应报错"),
        Err(e) => e,
    };
    assert!(err.contains("互斥"), "错误应说明 preset/model 互斥: {err}");
}

#[test]
fn default_config_with_api_key_returns_real_provider() {
    // config 提供 anthropic apiKey → 默认 preset 解析出真实 provider，
    // default_model 为默认模型标识（非 "echo"），证明走真实分支。
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(
        &path,
        r#"{"providers":{"anthropic":{"apiKey":"test-key"}}}"#,
    )
    .unwrap();

    let provider = build_provider(path.to_str(), None, None).unwrap();
    assert_ne!(provider.default_model(), "echo");
    assert!(
        provider.default_model().contains("anthropic"),
        "默认模型应来自 anthropic preset: {}",
        provider.default_model()
    );
}
