//! `/api/settings/*/update` 写入语义：把前端设置页的变更映射回 lure [`Config`]。
//!
//! 覆盖已被 lure Config 建模的三个配置面（对齐上游 nanobot webui 的 query 契约，
//! 前端以 `GET .../update?a=b` 形式携带 snake_case 参数）：
//! - `apply_agent_update`：`/api/settings/update` —— 编辑 `agents.defaults`
//!   （model / provider / contextWindowTokens）与生效 preset 指针（modelPreset）。
//! - `apply_provider_update`：`/api/settings/provider/update` —— provider 的
//!   api_key / api_base 覆盖（写入 `providers.<name>`）。
//! - `create_model_configuration` / `update_model_configuration`：
//!   `/api/settings/model-configurations/{create,update}` —— 命名 `model_presets`。
//!
//! 每个函数**返回新 [`Config`]**（不就地改原值），并在返回前跑 [`Config::validate`]，
//! 校验失败即回 `Err(message)`，调用方据此回 400 且不落盘——避免半成品状态污染配置。
//! lure 尚未建模的参数（timezone / bot_name / bot_icon / tool_hint_max_length /
//! api_type 等）静默忽略：接受但不报错，让页面平稳保存。

use std::collections::HashMap;

use crate::config::{
    Config, ModelPresetConfig, DEFAULT_CONTEXT_WINDOW_TOKENS, DEFAULT_MAX_TOKENS,
    DEFAULT_PRESET_NAME, DEFAULT_PROVIDER, DEFAULT_TEMPERATURE,
};

/// 已解码的 query 参数表（键值均为 snake_case 明文）。
pub type Params = HashMap<String, String>;

/// 取非空必填参数。
fn require<'a>(params: &'a Params, key: &str) -> Result<&'a str, String> {
    match params.get(key) {
        Some(v) if !v.is_empty() => Ok(v.as_str()),
        _ => Err(format!("缺少必填参数: {key}")),
    }
}

/// 取非空可选参数（空串视为未提供）。
fn optional<'a>(params: &'a Params, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
}

/// 解析 token 数参数；非法数字回 `Err`。
fn parse_tokens(value: &str, key: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("参数 {key} 需为非负整数，得到: {value}"))
}

/// preset 指针归一：空 / `default` → `None`（隐式默认），否则 `Some(name)`。
fn normalize_preset(value: &str) -> Option<String> {
    match value {
        "" | DEFAULT_PRESET_NAME => None,
        other => Some(other.to_string()),
    }
}

/// 应用 `/api/settings/update`：编辑默认 agent 与生效 preset 指针。
///
/// 只覆盖 lure 已建模的字段；display 类字段（timezone/bot_*）静默忽略。
pub fn apply_agent_update(config: &Config, params: &Params) -> Result<Config, String> {
    let mut next = config.clone();
    {
        let d = &mut next.agents.defaults;
        if let Some(model) = optional(params, "model") {
            d.model = model.to_string();
        }
        if let Some(provider) = optional(params, "provider") {
            d.provider = provider.to_string();
        }
        if let Some(v) = params.get("context_window_tokens") {
            d.context_window_tokens = parse_tokens(v, "context_window_tokens")?;
        }
        if let Some(preset) = params.get("model_preset") {
            d.model_preset = normalize_preset(preset);
        }
    }
    next.validate()?;
    Ok(next)
}

/// 应用 `/api/settings/provider/update`：写入 provider 的 api_key / api_base 覆盖。
///
/// 空串表示清除该覆盖（回落 registry 默认 / 环境变量）。缺失的 provider 条目按需创建
/// （`enabled` 取默认 `true`）。`api_type` 尚未建模，忽略。
pub fn apply_provider_update(config: &Config, params: &Params) -> Result<Config, String> {
    let name = require(params, "provider")?.to_string();
    let mut next = config.clone();
    let entry = next.providers.entry(name).or_default();
    if let Some(v) = params.get("api_key") {
        entry.api_key = if v.is_empty() { None } else { Some(v.clone()) };
    }
    if let Some(v) = params.get("api_base") {
        entry.api_base = if v.is_empty() { None } else { Some(v.clone()) };
    }
    next.validate()?;
    Ok(next)
}

/// 应用 `/api/settings/model-configurations/create`：新增命名 preset。
///
/// 前端只传 name / label / provider / model；其余生成参数取 lure 默认值。
/// 保留名 `default` 由 [`Config::validate`] 拒绝。
pub fn create_model_configuration(config: &Config, params: &Params) -> Result<Config, String> {
    let name = require(params, "name")?.to_string();
    let model = require(params, "model")?.to_string();
    let mut next = config.clone();
    let preset = ModelPresetConfig {
        label: optional(params, "label").map(str::to_string),
        model,
        provider: optional(params, "provider")
            .unwrap_or(DEFAULT_PROVIDER)
            .to_string(),
        max_tokens: DEFAULT_MAX_TOKENS,
        context_window_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
        temperature: DEFAULT_TEMPERATURE,
        reasoning_effort: None,
    };
    next.model_presets.insert(name, preset);
    next.validate()?;
    Ok(next)
}

/// 应用 `/api/settings/model-configurations/update`：编辑既有命名 preset。
///
/// `name` 定位目标 preset，不存在即回 `Err`。仅覆盖提供的字段。
pub fn update_model_configuration(config: &Config, params: &Params) -> Result<Config, String> {
    let name = require(params, "name")?;
    let mut next = config.clone();
    let preset = next
        .model_presets
        .get_mut(name)
        .ok_or_else(|| format!("model preset '{name}' 不存在"))?;
    if let Some(v) = params.get("label") {
        preset.label = if v.is_empty() { None } else { Some(v.clone()) };
    }
    if let Some(provider) = optional(params, "provider") {
        preset.provider = provider.to_string();
    }
    if let Some(model) = optional(params, "model") {
        preset.model = model.to_string();
    }
    if let Some(v) = params.get("context_window_tokens") {
        preset.context_window_tokens = parse_tokens(v, "context_window_tokens")?;
    }
    next.validate()?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Params {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn agent_update_sets_model_provider_and_context_window() {
        let config = Config::default();
        let next = apply_agent_update(
            &config,
            &params(&[
                ("model", "deepseek/deepseek-chat"),
                ("provider", "deepseek"),
                ("context_window_tokens", "128000"),
            ]),
        )
        .unwrap();
        assert_eq!(next.agents.defaults.model, "deepseek/deepseek-chat");
        assert_eq!(next.agents.defaults.provider, "deepseek");
        assert_eq!(next.agents.defaults.context_window_tokens, 128000);
    }

    #[test]
    fn agent_update_normalizes_default_preset_pointer_to_none() {
        let config = Config::default();
        let next = apply_agent_update(&config, &params(&[("model_preset", "default")])).unwrap();
        assert_eq!(next.agents.defaults.model_preset, None);
    }

    #[test]
    fn agent_update_rejects_unknown_preset_pointer() {
        let config = Config::default();
        let err = apply_agent_update(&config, &params(&[("model_preset", "ghost")])).unwrap_err();
        assert!(err.contains("ghost"), "err={err}");
    }

    #[test]
    fn agent_update_rejects_non_numeric_context_window() {
        let config = Config::default();
        let err =
            apply_agent_update(&config, &params(&[("context_window_tokens", "lots")])).unwrap_err();
        assert!(err.contains("context_window_tokens"), "err={err}");
    }

    #[test]
    fn agent_update_ignores_unmodeled_display_fields() {
        let config = Config::default();
        // timezone/bot_name 等 lure 未建模：接受但不报错，其余字段不变。
        let next = apply_agent_update(
            &config,
            &params(&[("timezone", "Asia/Shanghai"), ("bot_name", "lure")]),
        )
        .unwrap();
        assert_eq!(next.agents.defaults.model, config.agents.defaults.model);
    }

    #[test]
    fn provider_update_writes_key_and_base() {
        let config = Config::default();
        let next = apply_provider_update(
            &config,
            &params(&[
                ("provider", "deepseek"),
                ("api_key", "sk-abc"),
                ("api_base", "https://api.deepseek.com"),
            ]),
        )
        .unwrap();
        let pc = next.providers.get("deepseek").unwrap();
        assert_eq!(pc.api_key.as_deref(), Some("sk-abc"));
        assert_eq!(pc.api_base.as_deref(), Some("https://api.deepseek.com"));
        assert!(pc.enabled, "新建 provider 条目应默认启用");
    }

    #[test]
    fn provider_update_empty_key_clears_override() {
        let mut config = Config::default();
        config.providers.insert(
            "deepseek".to_string(),
            crate::config::ProviderConfig {
                api_key: Some("sk-old".to_string()),
                api_base: None,
                enabled: true,
            },
        );
        let next = apply_provider_update(
            &config,
            &params(&[("provider", "deepseek"), ("api_key", "")]),
        )
        .unwrap();
        assert_eq!(next.providers.get("deepseek").unwrap().api_key, None);
    }

    #[test]
    fn provider_update_requires_provider_name() {
        let config = Config::default();
        let err = apply_provider_update(&config, &params(&[("api_key", "x")])).unwrap_err();
        assert!(err.contains("provider"), "err={err}");
    }

    #[test]
    fn create_preset_inserts_named_row_with_defaults() {
        let config = Config::default();
        let next = create_model_configuration(
            &config,
            &params(&[
                ("name", "fast"),
                ("label", "Fast"),
                ("provider", "deepseek"),
                ("model", "deepseek/deepseek-chat"),
            ]),
        )
        .unwrap();
        let preset = next.model_presets.get("fast").unwrap();
        assert_eq!(preset.label.as_deref(), Some("Fast"));
        assert_eq!(preset.model, "deepseek/deepseek-chat");
        assert_eq!(preset.provider, "deepseek");
        assert_eq!(preset.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn create_preset_rejects_reserved_default_name() {
        let config = Config::default();
        let err =
            create_model_configuration(&config, &params(&[("name", "default"), ("model", "x/y")]))
                .unwrap_err();
        assert!(err.contains("default"), "err={err}");
    }

    #[test]
    fn update_preset_edits_existing_fields() {
        let base = create_model_configuration(
            &Config::default(),
            &params(&[("name", "fast"), ("model", "a/b"), ("provider", "auto")]),
        )
        .unwrap();
        let next = update_model_configuration(
            &base,
            &params(&[
                ("name", "fast"),
                ("model", "c/d"),
                ("context_window_tokens", "64000"),
            ]),
        )
        .unwrap();
        let preset = next.model_presets.get("fast").unwrap();
        assert_eq!(preset.model, "c/d");
        assert_eq!(preset.context_window_tokens, 64000);
    }

    #[test]
    fn update_preset_missing_target_errors() {
        let config = Config::default();
        let err =
            update_model_configuration(&config, &params(&[("name", "ghost"), ("model", "x/y")]))
                .unwrap_err();
        assert!(err.contains("ghost"), "err={err}");
    }
}
