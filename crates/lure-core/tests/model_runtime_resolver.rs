//! 映射上游 `tests/agent/test_model_runtime_resolver.py` 的 stateful resolver 语义：
//! preset → 不可变 `LlmRuntime`/`ProviderSnapshot` 的解析、缓存（preset tracking）、
//! `admit`/`refresh`/`invalidate` 生命周期与 provider 匹配。
//!
//! 上游源码未随仓库 vendored，本文件按 upstream-test-ledger 记录的生命周期语义
//! （refresh/admit/invalidate + preset tracking + 不可变快照）建立事实来源；真实网络
//! 调用不在此覆盖（provider 传输由 Phase 4 假传输/opt-in smoke 负责）。

use lure_core::config::{Config, ModelPresetConfig, PresetError};
use lure_core::provider::{GenerationSettings, ModelRuntimeResolver, RuntimeError};

/// 构造一个仅含单个命名 preset 的 config（其余走默认）。
fn config_with_preset(name: &str, preset: ModelPresetConfig) -> Config {
    let mut config = Config::default();
    config.model_presets.insert(name.to_string(), preset);
    config
}

/// 一个显式全字段的 preset，避免依赖 serde 默认。
fn preset(model: &str, provider: &str) -> ModelPresetConfig {
    ModelPresetConfig {
        label: None,
        model: model.to_string(),
        provider: provider.to_string(),
        max_tokens: 1024,
        context_window_tokens: 64_000,
        temperature: 0.5,
        reasoning_effort: None,
    }
}

#[test]
fn admit_default_resolves_runtime_from_agent_defaults() {
    let mut resolver = ModelRuntimeResolver::new(Config::default());
    assert!(resolver.active().is_none());

    let runtime = resolver.admit(None).unwrap();

    assert_eq!(runtime.preset_name, "default");
    assert_eq!(runtime.provider.provider_name, "anthropic");
    assert_eq!(runtime.provider.model, "anthropic/claude-opus-4-5");
    assert_eq!(runtime.provider.api_base, "https://api.anthropic.com/v1");
    assert_eq!(
        runtime.settings,
        GenerationSettings {
            temperature: 0.1,
            max_tokens: 8192,
            reasoning_effort: None,
        }
    );
    assert_eq!(runtime.generation, 1);

    let active = resolver.active().unwrap();
    assert_eq!(active.preset_name, "default");
    assert_eq!(active.generation, 1);
}

#[test]
fn admit_is_idempotent_and_returns_cached_generation() {
    let mut resolver = ModelRuntimeResolver::new(Config::default());

    let first = resolver.admit(None).unwrap();
    let second = resolver.admit(None).unwrap();

    assert_eq!(first.generation, 1);
    assert_eq!(second.generation, 1);
}

#[test]
fn admit_named_preset_tracks_provider_and_settings() {
    let mut fast = preset("deepseek-chat", "auto");
    fast.label = Some("Fast".to_string());
    fast.max_tokens = 1024;
    fast.temperature = 0.5;
    fast.reasoning_effort = Some("low".to_string());

    let mut resolver = ModelRuntimeResolver::new(config_with_preset("fast", fast));
    let runtime = resolver.admit(Some("fast")).unwrap();

    assert_eq!(runtime.preset_name, "fast");
    assert_eq!(runtime.provider.provider_name, "deepseek");
    assert_eq!(runtime.provider.api_base, "https://api.deepseek.com");
    assert_eq!(
        runtime.settings,
        GenerationSettings {
            temperature: 0.5,
            max_tokens: 1024,
            reasoning_effort: Some("low".to_string()),
        }
    );
}

#[test]
fn refresh_rebuilds_and_bumps_generation() {
    let mut resolver = ModelRuntimeResolver::new(Config::default());

    let first = resolver.admit(None).unwrap();
    assert_eq!(first.generation, 1);

    let refreshed = resolver.refresh(None).unwrap();
    assert_eq!(refreshed.preset_name, "default");
    assert_eq!(refreshed.generation, 2);
    assert_eq!(resolver.active().unwrap().generation, 2);
}

#[test]
fn invalidate_clears_active_and_forces_rebuild() {
    let mut resolver = ModelRuntimeResolver::new(Config::default());

    let first = resolver.admit(None).unwrap();
    assert_eq!(first.generation, 1);

    resolver.invalidate("default");
    assert!(resolver.active().is_none());

    let rebuilt = resolver.admit(None).unwrap();
    assert_eq!(rebuilt.generation, 2);
}

#[test]
fn admit_unknown_preset_errors_and_leaves_active_untouched() {
    let mut resolver = ModelRuntimeResolver::new(Config::default());

    let err = resolver.admit(Some("missing")).unwrap_err();
    assert!(
        matches!(&err, RuntimeError::Preset(PresetError::NotFound(name)) if name == "missing"),
        "unexpected error: {err:?}"
    );
    assert!(resolver.active().is_none());
}

#[test]
fn admit_errors_when_provider_cannot_be_matched() {
    let mut resolver =
        ModelRuntimeResolver::new(config_with_preset("weird", preset("mystery-model", "auto")));

    let err = resolver.admit(Some("weird")).unwrap_err();
    assert!(
        matches!(&err, RuntimeError::ProviderNotFound { model, provider }
            if model == "mystery-model" && provider == "auto"),
        "unexpected error: {err:?}"
    );
}

#[test]
fn admit_honors_forced_provider_name() {
    let mut resolver = ModelRuntimeResolver::new(config_with_preset(
        "forced",
        preset("some-custom-model", "groq"),
    ));

    let runtime = resolver.admit(Some("forced")).unwrap();

    assert_eq!(runtime.provider.provider_name, "groq");
    assert_eq!(runtime.provider.api_base, "https://api.groq.com/openai/v1");
}

#[test]
fn switching_presets_tracks_active_and_caches_each() {
    let mut resolver =
        ModelRuntimeResolver::new(config_with_preset("smart", preset("gpt-4o", "auto")));

    let default_rt = resolver.admit(None).unwrap();
    assert_eq!(resolver.active().unwrap().preset_name, "default");

    let smart_rt = resolver.admit(Some("smart")).unwrap();
    assert_eq!(smart_rt.provider.provider_name, "openai");
    assert_eq!(resolver.active().unwrap().preset_name, "smart");

    // 回到 default 命中缓存，generation 不回退。
    let default_again = resolver.admit(None).unwrap();
    assert_eq!(default_again.generation, default_rt.generation);
    assert_eq!(resolver.active().unwrap().preset_name, "default");
}

#[test]
fn admit_applies_provider_api_base_override_from_config() {
    let mut config = config_with_preset("fast", preset("deepseek-chat", "auto"));
    config.providers.insert(
        "deepseek".to_string(),
        lure_core::config::ProviderConfig {
            api_base: Some("https://proxy.example/v1".to_string()),
            ..lure_core::config::ProviderConfig::default()
        },
    );

    let mut resolver = ModelRuntimeResolver::new(config);
    let runtime = resolver.admit(Some("fast")).unwrap();

    assert_eq!(runtime.provider.provider_name, "deepseek");
    assert_eq!(runtime.provider.api_base, "https://proxy.example/v1");
}

#[test]
fn admit_errors_when_matched_provider_disabled() {
    let mut config = config_with_preset("smart", preset("gpt-4o", "auto"));
    config.providers.insert(
        "openai".to_string(),
        lure_core::config::ProviderConfig {
            enabled: false,
            ..lure_core::config::ProviderConfig::default()
        },
    );

    let mut resolver = ModelRuntimeResolver::new(config);
    let err = resolver.admit(Some("smart")).unwrap_err();
    assert!(
        matches!(&err, RuntimeError::ProviderNotFound { .. }),
        "unexpected error: {err:?}"
    );
}

#[test]
fn admit_none_follows_agent_default_preset_pointer() {
    let mut config = config_with_preset("fast", preset("deepseek-chat", "auto"));
    config.agents.defaults.model_preset = Some("fast".to_string());

    let mut resolver = ModelRuntimeResolver::new(config);
    let runtime = resolver.admit(None).unwrap();

    assert_eq!(runtime.preset_name, "fast");
    assert_eq!(runtime.provider.provider_name, "deepseek");
}
