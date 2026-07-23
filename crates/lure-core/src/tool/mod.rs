//! 工具子系统：trait、注册表、schema 校验、结果截断，及文件/shell 工具。
//!
//! Phase 5 覆盖 tool trait/registry、参数校验、结果截断、workspace 约束的文件工具与
//! shell allow/deny 策略。apply_patch/search/web/mcp、async 执行、上下文变量注入的
//! 完整链路留待后续。

mod file;
mod registry;
mod result;
mod schema;
mod shell;

pub use file::{ReadFileTool, WriteFileTool};
pub use registry::{Tool, ToolError, ToolRegistry};
pub use result::{truncate_result, ToolResult};
pub use schema::validate_value;
pub use shell::{ExecPolicy, ExecTool};
