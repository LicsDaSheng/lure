//! 配置 schema：Phase 1 的最小 typed config。
//!
//! 字段与默认值以上游 `nanobot/config/schema.py` 为事实来源：
//! - `AgentDefaults.workspace = "~/.nanobot/workspace"`
//! - `AgentDefaults.model = "anthropic/claude-opus-4-5"`
//! - `AgentDefaults.provider = "auto"`
//! - `AgentDefaults.max_tokens = 8192`
//! - `AgentDefaults.temperature = 0.1`
//!
//! 别名策略同样对齐上游 `config_base.Base`（`alias_generator=to_camel`,
//! `populate_by_name=True`）：序列化输出 camelCase；反序列化同时接受 camelCase
//! 与 snake_case。缺失字段回落到默认值；未知字段忽略。
//!
//! providers / modelPresets / gateway / tools / security 等字段暂不建模，
//! 待对应 phase 确认上游契约后再收敛扩展。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 默认模型标识。
pub const DEFAULT_MODEL: &str = "anthropic/claude-opus-4-5";
/// 默认 workspace 路径（未展开 `~`）。
pub const DEFAULT_WORKSPACE: &str = "~/.nanobot/workspace";
/// 默认 provider 选择策略。
pub const DEFAULT_PROVIDER: &str = "auto";
/// 默认单次生成 max tokens。
pub const DEFAULT_MAX_TOKENS: u32 = 8192;
/// 默认上下文窗口 token 数。
pub const DEFAULT_CONTEXT_WINDOW_TOKENS: u32 = 200_000;
/// 默认采样温度。
pub const DEFAULT_TEMPERATURE: f64 = 0.1;
/// 保留的隐式默认 preset 名。
pub const DEFAULT_PRESET_NAME: &str = "default";

/// preset 解析错误。
#[derive(Debug, Clone, PartialEq)]
pub enum PresetError {
    /// 显式请求的 preset 名不存在。
    NotFound(String),
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::NotFound(name) => {
                write!(f, "model_preset '{name}' not found in model_presets")
            }
        }
    }
}

impl std::error::Error for PresetError {}

/// 顶层配置对象。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// agent 相关配置。
    pub agents: AgentsConfig,
    /// 命名 model preset 集合。
    #[serde(alias = "model_presets")]
    pub model_presets: BTreeMap<String, ModelPresetConfig>,
}

impl Config {
    /// 校验配置的 preset 约束，对齐上游 `_validate_model_preset`。
    ///
    /// - `default` 为保留名，不能作为 preset。
    /// - `agents.defaults.model_preset` 若设置，必须存在于 `model_presets`。
    pub fn validate(&self) -> Result<(), String> {
        if self.model_presets.contains_key(DEFAULT_PRESET_NAME) {
            return Err(format!(
                "model_preset name '{DEFAULT_PRESET_NAME}' is reserved for agents.defaults"
            ));
        }
        if let Some(name) = &self.agents.defaults.model_preset {
            if !name.is_empty()
                && name != DEFAULT_PRESET_NAME
                && !self.model_presets.contains_key(name)
            {
                return Err(format!("model_preset '{name}' not found in model_presets"));
            }
        }
        Ok(())
    }

    /// 由 `agents.defaults` 字段构造隐式 `default` preset。
    pub fn resolve_default_preset(&self) -> ModelPresetConfig {
        let d = &self.agents.defaults;
        ModelPresetConfig {
            label: None,
            model: d.model.clone(),
            provider: d.provider.clone(),
            max_tokens: d.max_tokens,
            context_window_tokens: d.context_window_tokens,
            temperature: d.temperature,
            reasoning_effort: d.reasoning_effort.clone(),
        }
    }

    /// 解析生效的 model 参数：命名 preset 或隐式默认。
    ///
    /// `name` 为 `None` 时回落到 `agents.defaults.model_preset`；空或 `default`
    /// 返回隐式默认 preset；否则查表，缺失返回 [`PresetError::NotFound`]。
    pub fn resolve_preset(&self, name: Option<&str>) -> Result<ModelPresetConfig, PresetError> {
        let resolved_name = match name {
            Some(explicit) => Some(explicit.to_string()),
            None => self.agents.defaults.model_preset.clone(),
        };
        match resolved_name.as_deref() {
            None | Some("") | Some(DEFAULT_PRESET_NAME) => Ok(self.resolve_default_preset()),
            Some(other) => self
                .model_presets
                .get(other)
                .cloned()
                .ok_or_else(|| PresetError::NotFound(other.to_string())),
        }
    }
}

/// agents 配置分组。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentsConfig {
    /// 默认 agent 配置。
    pub defaults: AgentDefaults,
}

/// 默认 agent 配置字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentDefaults {
    /// agent workspace 路径（可含 `~`）。
    pub workspace: String,
    /// 模型标识。
    pub model: String,
    /// provider 名称或 `auto`。
    pub provider: String,
    /// 生效的 preset 名；优先于下面的裸字段。
    #[serde(alias = "model_preset", skip_serializing_if = "Option::is_none")]
    pub model_preset: Option<String>,
    /// 单次生成 max tokens；同时接受 snake_case 键。
    #[serde(alias = "max_tokens")]
    pub max_tokens: u32,
    /// 上下文窗口 token 数。
    #[serde(alias = "context_window_tokens")]
    pub context_window_tokens: u32,
    /// 采样温度。
    pub temperature: f64,
    /// 推理力度（部分模型支持）。
    #[serde(alias = "reasoning_effort", skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            workspace: DEFAULT_WORKSPACE.to_string(),
            model: DEFAULT_MODEL.to_string(),
            provider: DEFAULT_PROVIDER.to_string(),
            model_preset: None,
            max_tokens: DEFAULT_MAX_TOKENS,
            context_window_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
            reasoning_effort: None,
        }
    }
}

/// 命名 model preset：一组模型 + 生成参数，对齐上游 `ModelPresetConfig`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPresetConfig {
    /// 可选展示标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 模型标识（必填）。
    pub model: String,
    /// provider 名称或 `auto`。
    #[serde(default = "default_preset_provider")]
    pub provider: String,
    /// 单次生成 max tokens。
    #[serde(default = "default_preset_max_tokens", alias = "max_tokens")]
    pub max_tokens: u32,
    /// 上下文窗口 token 数。
    #[serde(
        default = "default_preset_context_window",
        alias = "context_window_tokens"
    )]
    pub context_window_tokens: u32,
    /// 采样温度。
    #[serde(default = "default_preset_temperature")]
    pub temperature: f64,
    /// 推理力度。
    #[serde(
        default,
        alias = "reasoning_effort",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_effort: Option<String>,
}

fn default_preset_provider() -> String {
    DEFAULT_PROVIDER.to_string()
}

fn default_preset_max_tokens() -> u32 {
    DEFAULT_MAX_TOKENS
}

fn default_preset_context_window() -> u32 {
    DEFAULT_CONTEXT_WINDOW_TOKENS
}

fn default_preset_temperature() -> f64 {
    DEFAULT_TEMPERATURE
}
