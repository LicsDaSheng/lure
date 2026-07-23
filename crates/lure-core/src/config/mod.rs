//! 配置子系统：schema、路径解析与读写。
//!
//! Phase 1 只覆盖 config load/save、路径解析、camelCase/snake_case 兼容与结构化
//! 错误。providers / gateway / tools / security 等字段与 onboard 初始化留待
//! 对应 phase 按上游契约补齐。

mod loader;
mod paths;
mod schema;

pub use loader::{load_config, save_config, ConfigError};
pub use paths::{
    default_config_path, default_workspace, expand_user, home_dir, is_default_workspace,
    resolve_workspace,
};
pub use schema::{
    AgentDefaults, AgentsConfig, Config, ModelPresetConfig, PresetError,
    DEFAULT_CONTEXT_WINDOW_TOKENS, DEFAULT_MAX_TOKENS, DEFAULT_MODEL, DEFAULT_PRESET_NAME,
    DEFAULT_PROVIDER, DEFAULT_TEMPERATURE, DEFAULT_WORKSPACE,
};
