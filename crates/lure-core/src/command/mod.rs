//! 命令路由子系统：slash 命令的归一、三层派发与内置命令注册。
//!
//! 对齐上游 `nanobot/command/`：[`CommandRouter`] 提供 priority/exact/prefix 三层派发与
//! `is_priority`/`is_dispatchable_command` 谓词，[`register_builtin_commands`] 注册内置命令。
//! `/help`、`/pairing` 行为完整；其余运行时依赖型命令已登记命令名并随对应子系统接线。

mod builtin;
mod router;

pub use builtin::{
    build_help_text, builtin_command_specs, register_builtin_commands, BuiltinDeps,
    CommandLifecycle, CommandSpec,
};
pub use router::{
    normalize_command_text, CommandContext, CommandHandler, CommandOutput, CommandRouter,
};
