//! `/api/settings` 载荷派生。
//!
//! 从 lure [`Config`] 构造前端设置页所需的 `settings_payload`，顶层结构对齐上游
//! `nanobot/webui/settings_api.py::settings_payload`（保证前端各面板绑定的键都在，
//! 不因缺字段崩溃）。lure Config 已建模的段（`agent` / `model_presets` / `providers` /
//! `advanced.exec`）填真实值；未建模的段（web/api/gateway/image/transcription 等）给
//! 结构完整的合理默认值——待各自能力落地后再填真。secret 只回显是否已配置，不泄明文。

use serde_json::{json, Value};

use crate::config::{AgentDefaults, Config, ModelPresetConfig};
use crate::provider::registry::find_by_name;

/// 隐式默认 preset 名。
const DEFAULT_PRESET: &str = "default";

/// 构造 `/api/settings` 载荷。
pub fn settings_payload(config: &Config) -> Value {
    let d = &config.agents.defaults;
    let active = active_preset_name(d);
    let resolved_provider = d.provider.clone();

    json!({
        "agent": {
            "model": d.model,
            "provider": d.provider,
            "resolved_provider": resolved_provider,
            "has_api_key": config.provider_api_key(&d.provider).is_some(),
            "model_preset": active,
            "max_tokens": d.max_tokens,
            "context_window_tokens": d.context_window_tokens,
            "temperature": d.temperature,
            "reasoning_effort": d.reasoning_effort,
            // 以下字段 lure Config 暂未建模，给上游默认值占位。
            "timezone": "UTC",
            "bot_name": "lure",
            "bot_icon": "🐟",
            "tool_hint_max_length": 40,
        },
        "model_presets": model_presets(config, &active),
        "providers": providers(config),
        "web_search": {
            "provider": "duckduckgo",
            "api_key_hint": Value::Null,
            "base_url": Value::Null,
            "max_results": 5,
            "timeout": 30,
            "providers": ["duckduckgo"],
        },
        "web": {
            "enable": true,
            "proxy": Value::Null,
            "user_agent": Value::Null,
            "search": {"max_results": 5, "timeout": 30},
            "fetch": {"use_jina_reader": true},
        },
        "api": {
            "host": "127.0.0.1",
            "port": 8900,
            "timeout": 120.0,
            "api_key_hint": Value::Null,
        },
        "observability": {
            "provider": "langfuse",
            "configured": false,
            "base_url": "https://cloud.langfuse.com",
        },
        "image_generation": {
            "enabled": false,
            "provider": "",
            "provider_configured": false,
            "model": "",
            "default_aspect_ratio": "1:1",
            "default_image_size": "1K",
            "max_images_per_turn": 4,
            "save_dir": "generated",
            "providers": [],
        },
        "transcription": {
            "enabled": false,
            "provider": Value::Null,
            "provider_configured": false,
            "model": Value::Null,
            "language": Value::Null,
            "max_duration_sec": 120,
            "max_upload_mb": 25,
            "providers": [],
        },
        "runtime": {
            "config_path": "",
            "workspace_path": d.workspace,
            "gateway_host": "127.0.0.1",
            "gateway_port": 18790,
            "heartbeat": {"enabled": true, "interval_s": 1800, "keep_recent_messages": 8},
            "dream": {"schedule": "every 2h"},
            "unified_session": false,
        },
        "usage": usage_payload(),
        "advanced": {
            "restrict_to_workspace": false,
            "workspace_sandbox": {},
            "webui_allow_local_service_access": true,
            "allow_local_preview_access": true,
            "webui_default_access_mode": "default",
            "private_service_protection_enabled": true,
            "ssrf_whitelist_count": 0,
            "mcp_server_count": 0,
            "exec_enabled": config.tools.exec.enabled,
            "exec_sandbox": Value::Null,
            "exec_path_prepend_set": false,
            "exec_path_append_set": false,
        },
        "requires_restart": false,
        "version": {"current": crate::version()},
        "docs": {},
        // decorate_settings_payload 附加的运行时 UI 提示（给保守默认）。
        "surface": "desktop",
        "runtime_surface": "desktop",
        "runtime_capabilities": {},
        "restart_required_sections": [],
        "apply_state": {"status": "idle"},
    })
}

/// 生效 preset 名：`None`/空/`default` 归一为 `default`。
fn active_preset_name(d: &AgentDefaults) -> String {
    match d.model_preset.as_deref() {
        None | Some("") | Some(DEFAULT_PRESET) => DEFAULT_PRESET.to_string(),
        Some(name) => name.to_string(),
    }
}

/// 隐式 `default`（取 `agents.defaults`）+ 命名 presets。
fn model_presets(config: &Config, active: &str) -> Vec<Value> {
    let d = &config.agents.defaults;
    let mut rows = vec![preset_row(
        DEFAULT_PRESET,
        "Default",
        active == DEFAULT_PRESET,
        true,
        &d.model,
        &d.provider,
        d.max_tokens,
        d.context_window_tokens,
        d.temperature,
        d.reasoning_effort.as_deref(),
    )];
    for (name, p) in &config.model_presets {
        rows.push(named_preset_row(name, active == name, p));
    }
    rows
}

fn named_preset_row(name: &str, is_active: bool, p: &ModelPresetConfig) -> Value {
    preset_row(
        name,
        p.label.as_deref().unwrap_or(name),
        is_active,
        false,
        &p.model,
        &p.provider,
        p.max_tokens,
        p.context_window_tokens,
        p.temperature,
        p.reasoning_effort.as_deref(),
    )
}

#[allow(clippy::too_many_arguments)]
fn preset_row(
    name: &str,
    label: &str,
    active: bool,
    is_default: bool,
    model: &str,
    provider: &str,
    max_tokens: u32,
    context_window_tokens: u32,
    temperature: f64,
    reasoning_effort: Option<&str>,
) -> Value {
    json!({
        "name": name,
        "label": label,
        "active": active,
        "is_default": is_default,
        "model": model,
        "provider": provider,
        "max_tokens": max_tokens,
        "context_window_tokens": context_window_tokens,
        "temperature": temperature,
        "reasoning_effort": reasoning_effort,
        "reasoning_effort_values": [],
    })
}

/// 由 `config.providers` 构造 provider 行；`default_api_base` 取 registry 默认。
fn providers(config: &Config) -> Vec<Value> {
    config
        .providers
        .iter()
        .map(|(name, pc)| {
            let configured = pc.api_key.as_deref().is_some_and(|k| !k.is_empty());
            json!({
                "name": name,
                "label": name,
                "configured": configured,
                "auth_type": "api_key",
                "api_key_required": true,
                "api_key_hint": if configured { json!("••••") } else { Value::Null },
                "api_base": pc.api_base,
                "default_api_base": find_by_name(name).map(|s| s.default_api_base),
                "model_selectable": true,
                "model_catalog": "auto",
                "enabled": pc.enabled,
            })
        })
        .collect()
}

/// `usage`/`/api/settings/usage` 的轻量 token 用量切片（lure 暂无统计，回零）。
pub fn usage_payload() -> Value {
    json!({
        "days": [],
        "total_tokens": 0,
        "peak_day_tokens": 0,
        "current_streak_days": 0,
        "longest_streak_days": 0,
        "updated_at": Value::Null,
    })
}
