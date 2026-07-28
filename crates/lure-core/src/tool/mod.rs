//! 工具子系统：trait、注册表、schema 校验、结果截断，及文件/shell 工具。
//!
//! Phase 5 覆盖 tool trait/registry、参数校验、结果截断、workspace 约束的文件工具
//! （read/write/edit/list_dir/grep）与 shell allow/deny 策略。Phase 8 补 `cron` 工具
//! （add/list/remove 排期，绑定会话 origin）。apply_patch/find_files/web/mcp、async 执行、
//! 上下文变量注入的完整链路留待后续。

mod cron;
mod file;
mod registry;
mod result;
mod schema;
mod search;
mod setup;
mod shell;

pub use cron::{CronTool, CronToolOrigin};
pub use file::{find_match, EditFileTool, ReadFileTool, WriteFileTool};
pub use registry::{Tool, ToolError, ToolRegistry};
pub use result::{truncate_result, ToolResult};
pub use schema::validate_value;
pub use search::{GrepTool, ListDirTool};
pub use setup::{registry_from_config, ToolSetupError};
pub use shell::{ExecPolicy, ExecTool};
