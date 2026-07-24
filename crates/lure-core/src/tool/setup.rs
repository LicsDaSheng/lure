//! config 驱动的工具注册。
//!
//! 由 [`Config`] 与 workspace 路径构建一个 [`ToolRegistry`]：workspace 绑定的文件工具
//! 默认注册；shell exec 工具按 `tools.exec` 开关 + allow/deny 门禁注册。CLI/WebUI 等
//! 前端共用同一注册策略，避免各自拼装工具集出现漂移。

use std::fmt;
use std::path::Path;

use crate::config::Config;
use crate::tool::file::{ReadFileTool, WriteFileTool};
use crate::tool::registry::ToolRegistry;
use crate::tool::shell::{ExecPolicy, ExecTool};

/// 工具注册失败的结构化错误。
#[derive(Debug)]
pub enum ToolSetupError {
    /// exec allow/deny 正则编译失败。
    ExecPattern(regex::Error),
}

impl fmt::Display for ToolSetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolSetupError::ExecPattern(e) => write!(f, "exec 策略正则无效: {e}"),
        }
    }
}

impl std::error::Error for ToolSetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ToolSetupError::ExecPattern(e) => Some(e),
        }
    }
}

/// 由 config + workspace 构建工具注册表。
///
/// - 始终注册 workspace 绑定的 `read_file`/`write_file`（越界访问由工具层拒绝）。
/// - `tools.exec.enabled` 时注册 `exec`，策略取 `tools.exec.allow`/`deny`；正则非法即报错。
pub fn registry_from_config(
    config: &Config,
    workspace: &Path,
) -> Result<ToolRegistry, ToolSetupError> {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ReadFileTool::new(workspace)));
    registry.register(Box::new(WriteFileTool::new(workspace)));

    let exec = &config.tools.exec;
    if exec.enabled {
        let allow: Vec<&str> = exec.allow.iter().map(String::as_str).collect();
        let deny: Vec<&str> = exec.deny.iter().map(String::as_str).collect();
        let policy = ExecPolicy::new(&allow, &deny).map_err(ToolSetupError::ExecPattern)?;
        registry.register(Box::new(ExecTool::new(policy, workspace)));
    }

    Ok(registry)
}
