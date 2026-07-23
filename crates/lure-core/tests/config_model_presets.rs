//! 映射上游 `tests/config/test_model_presets.py` 的 preset 解析场景（config 层）。
//!
//! 暂未映射：`_match_provider`/`get_provider_name` 依赖尚未建模的 `ProvidersConfig`
//! （api_key/OAuth/local/transcription 过滤），随 Phase 4 后续或 Phase 7 收敛。

use lure_core::config::{load_config, Config, PresetError};

fn parse(json: &str) -> Config {
    serde_json::from_str(json).unwrap()
}

#[test]
fn resolve_preset_returns_defaults_when_no_preset() {
    let config = Config::default();
    let resolved = config.resolve_preset(None).unwrap();
    let defaults = &config.agents.defaults;
    assert_eq!(resolved.model, defaults.model);
    assert_eq!(resolved.provider, defaults.provider);
    assert_eq!(resolved.max_tokens, defaults.max_tokens);
    assert_eq!(
        resolved.context_window_tokens,
        defaults.context_window_tokens
    );
    assert_eq!(resolved.temperature, defaults.temperature);
    assert_eq!(resolved.reasoning_effort, defaults.reasoning_effort);
}

#[test]
fn legacy_defaults_without_presets_still_resolves() {
    let config = parse(
        r#"{"agents":{"defaults":{"model":"openai/gpt-4.1","provider":"openai",
        "maxTokens":4096,"contextWindowTokens":128000,"temperature":0.2,"reasoningEffort":"low"}}}"#,
    );
    assert!(config.agents.defaults.model_preset.is_none());
    assert!(config.model_presets.is_empty());

    let resolved = config.resolve_preset(None).unwrap();
    assert_eq!(resolved.model, "openai/gpt-4.1");
    assert_eq!(resolved.provider, "openai");
    assert_eq!(resolved.max_tokens, 4096);
    assert_eq!(resolved.context_window_tokens, 128000);
    assert_eq!(resolved.temperature, 0.2);
    assert_eq!(resolved.reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn resolve_preset_returns_active_preset() {
    let config = parse(
        r#"{"modelPresets":{"fast":{"model":"openai/gpt-4.1","provider":"openai",
        "maxTokens":4096,"contextWindowTokens":32768,"temperature":0.5,"reasoningEffort":"low"}},
        "agents":{"defaults":{"modelPreset":"fast"}}}"#,
    );
    let resolved = config.resolve_preset(None).unwrap();
    assert_eq!(resolved.model, "openai/gpt-4.1");
    assert_eq!(resolved.max_tokens, 4096);
    assert_eq!(resolved.context_window_tokens, 32768);
    assert_eq!(resolved.temperature, 0.5);
    assert_eq!(resolved.reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn default_preset_is_agents_defaults_even_when_named_preset_active() {
    let config = parse(
        r#"{"agents":{"defaults":{"model":"openai/gpt-4.1","provider":"openai","modelPreset":"fast"}},
        "modelPresets":{"fast":{"model":"openai/gpt-4.1-mini","provider":"openai"}}}"#,
    );
    assert_eq!(
        config.resolve_preset(None).unwrap().model,
        "openai/gpt-4.1-mini"
    );
    assert_eq!(
        config.resolve_preset(Some("default")).unwrap().model,
        "openai/gpt-4.1"
    );
}

#[test]
fn model_presets_accepts_and_serializes_camel_case_root_key() {
    let config =
        parse(r#"{"modelPresets":{"fast":{"model":"openai/gpt-4.1","provider":"openai"}}}"#);
    assert_eq!(config.model_presets["fast"].model, "openai/gpt-4.1");
    assert_eq!(config.model_presets["fast"].provider, "openai");
    // preset 未指定的字段回落默认。
    assert_eq!(config.model_presets["fast"].max_tokens, 8192);

    let dumped = serde_json::to_value(&config).unwrap();
    assert!(dumped.get("modelPresets").is_some());
    assert!(dumped.get("model_presets").is_none());
    assert_eq!(dumped["modelPresets"]["fast"]["model"], "openai/gpt-4.1");
}

#[test]
fn model_preset_field_accepts_snake_case_alias() {
    let config = parse(
        r#"{"agents":{"defaults":{"model_preset":"fast"}},
        "model_presets":{"fast":{"model":"x/y"}}}"#,
    );
    assert_eq!(config.agents.defaults.model_preset.as_deref(), Some("fast"));
    assert_eq!(config.resolve_preset(None).unwrap().model, "x/y");
}

#[test]
fn resolve_preset_can_target_named_preset_without_activating() {
    let config = parse(
        r#"{"modelPresets":{"fast":{"model":"openai/gpt-4.1","provider":"openai"},
        "deep":{"model":"anthropic/claude-opus-4-5","provider":"anthropic"}},
        "agents":{"defaults":{"modelPreset":"fast"}}}"#,
    );
    let resolved = config.resolve_preset(Some("deep")).unwrap();
    assert_eq!(resolved.model, "anthropic/claude-opus-4-5");
    assert_eq!(resolved.provider, "anthropic");
}

#[test]
fn resolve_preset_rejects_unknown_named_preset() {
    let err = Config::default()
        .resolve_preset(Some("missing"))
        .unwrap_err();
    assert_eq!(err, PresetError::NotFound("missing".to_string()));
}

#[test]
fn validate_rejects_unknown_active_preset() {
    let config = parse(r#"{"agents":{"defaults":{"modelPreset":"unknown"}}}"#);
    let err = config.validate().unwrap_err();
    assert!(err.contains("'unknown' not found"));
}

#[test]
fn validate_rejects_reserved_default_preset_name() {
    let config = parse(r#"{"modelPresets":{"default":{"model":"custom-model"}}}"#);
    let err = config.validate().unwrap_err();
    assert!(err.contains("reserved"));
}

#[test]
fn validate_accepts_explicit_default_preset_name() {
    let config =
        parse(r#"{"agents":{"defaults":{"model":"openai/gpt-4.1","modelPreset":"default"}}}"#);
    assert!(config.validate().is_ok());
    assert_eq!(config.resolve_preset(None).unwrap().model, "openai/gpt-4.1");
}

#[test]
fn load_config_rejects_invalid_preset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, r#"{"agents":{"defaults":{"modelPreset":"nope"}}}"#).unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(err.to_string().contains("配置校验失败"));
}
