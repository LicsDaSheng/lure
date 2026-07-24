//! config 驱动的 provider 匹配（`ProvidersConfig` + `Config::resolve_provider`）。
//!
//! 覆盖上游 `_match_provider`/`get_provider_name` 的 config 驱动切片：api_base 覆盖、
//! auto 匹配跳过禁用 provider、forced 显式选择、api_key 解析。OAuth 与 local fallback
//! 属更完整的 provider 配置，明确后延（见 upstream-test-ledger）。
//!
//! 上游源码未随仓库 vendored，本文件按 ledger 记录语义建立事实来源。

use lure_core::config::{Config, ProviderConfig};

/// 构造带单个 provider 覆盖的 config。
fn config_with_provider(name: &str, provider: ProviderConfig) -> Config {
    let mut config = Config::default();
    config.providers.insert(name.to_string(), provider);
    config
}

#[test]
fn resolve_provider_applies_api_base_override() {
    let config = config_with_provider(
        "deepseek",
        ProviderConfig {
            api_base: Some("https://proxy.example/v1".to_string()),
            ..ProviderConfig::default()
        },
    );

    let resolved = config.resolve_provider("deepseek-chat", "auto").unwrap();
    assert_eq!(resolved.name, "deepseek");
    assert_eq!(resolved.api_base, "https://proxy.example/v1");
}

#[test]
fn resolve_provider_falls_back_to_registry_default_api_base() {
    let resolved = Config::default()
        .resolve_provider("deepseek-chat", "auto")
        .unwrap();
    assert_eq!(resolved.name, "deepseek");
    assert_eq!(resolved.api_base, "https://api.deepseek.com");
}

#[test]
fn resolve_provider_skips_disabled_in_auto() {
    let config = config_with_provider(
        "openai",
        ProviderConfig {
            enabled: false,
            ..ProviderConfig::default()
        },
    );
    // gpt-4o 仅匹配 openai，被禁用后 auto 无候选。
    assert!(config.resolve_provider("gpt-4o", "auto").is_none());
}

#[test]
fn resolve_provider_honors_forced_even_if_disabled() {
    let config = config_with_provider(
        "anthropic",
        ProviderConfig {
            enabled: false,
            ..ProviderConfig::default()
        },
    );
    // forced 是显式意图：即使禁用也按名解析。
    let resolved = config
        .resolve_provider("some-unmatched-model", "anthropic")
        .unwrap();
    assert_eq!(resolved.name, "anthropic");
}

#[test]
fn resolve_provider_unknown_forced_is_none() {
    assert!(Config::default()
        .resolve_provider("gpt-4o", "nonexistent")
        .is_none());
}

#[test]
fn provider_api_key_reads_config_entry() {
    let config = config_with_provider(
        "deepseek",
        ProviderConfig {
            api_key: Some("sk-from-config".to_string()),
            ..ProviderConfig::default()
        },
    );
    assert_eq!(
        config.provider_api_key("deepseek").as_deref(),
        Some("sk-from-config")
    );
    assert_eq!(config.provider_api_key("openai"), None);
}

#[test]
fn providers_config_deserializes_camel_and_defaults_enabled() {
    let json = r#"{"providers":{"deepseek":{"apiKey":"k","apiBase":"https://b/v1"}}}"#;
    let config: Config = serde_json::from_str(json).unwrap();
    let entry = config.providers.get("deepseek").unwrap();
    assert_eq!(entry.api_key.as_deref(), Some("k"));
    assert_eq!(entry.api_base.as_deref(), Some("https://b/v1"));
    // enabled 省略时默认 true。
    assert!(entry.enabled);
}
