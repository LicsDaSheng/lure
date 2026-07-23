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

use serde::{Deserialize, Serialize};

/// 默认模型标识。
pub const DEFAULT_MODEL: &str = "anthropic/claude-opus-4-5";
/// 默认 workspace 路径（未展开 `~`）。
pub const DEFAULT_WORKSPACE: &str = "~/.nanobot/workspace";
/// 默认 provider 选择策略。
pub const DEFAULT_PROVIDER: &str = "auto";
/// 默认单次生成 max tokens。
pub const DEFAULT_MAX_TOKENS: u32 = 8192;
/// 默认采样温度。
pub const DEFAULT_TEMPERATURE: f64 = 0.1;

/// 顶层配置对象。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// agent 相关配置。
    pub agents: AgentsConfig,
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
    /// 单次生成 max tokens；同时接受 snake_case 键。
    #[serde(alias = "max_tokens")]
    pub max_tokens: u32,
    /// 采样温度。
    pub temperature: f64,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            workspace: DEFAULT_WORKSPACE.to_string(),
            model: DEFAULT_MODEL.to_string(),
            provider: DEFAULT_PROVIDER.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: DEFAULT_TEMPERATURE,
        }
    }
}
