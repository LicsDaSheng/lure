//! 内置 slash 命令注册表与处理器。
//!
//! 对齐上游 `nanobot/command/builtin.py`：命令规格表（供帮助文本与 UI palette）、
//! `register_builtin_commands` 把全部命令名注册进三层路由。
//!
//! 本阶段行为完整的处理器：`/help`（静态帮助）、`/pairing`（委托 [`PairingStore`]）。
//! 运行时依赖型命令（/new /status /model /history /goal /trigger /dream* /evaluator-prompt
//! /skill /stop /restart）已按上游层级登记命令名，使 `is_priority`/`is_dispatchable_command`
//! 谓词与上游对齐；其处理器随对应子系统（Skills/Subagent/dream 运行时等）逐步接线，当前返回
//! 明确的“尚未接入”提示而非臆造行为。

use std::sync::Arc;

use serde_json::Value;

use crate::agent::skills::SkillsLoader;
use crate::command::router::{CommandContext, CommandHandler, CommandOutput, CommandRouter};
use crate::pairing::{PairingStore, PAIRING_COMMAND_META_KEY};

/// 命令生命周期语义（对齐上游 `CommandLifecycle`，供后续 palette/网关调度使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandLifecycle {
    /// 侧信道回复，不影响活动 turn。
    SideChannel,
    /// 先收尾当前活动 turn。
    FinalizeActiveTurn,
    /// 取消当前活动 turn。
    StopActiveTurn,
    /// 以参数发起一个 agent turn。
    AgentTurnWithArgs,
}

/// 单条内置命令规格（对齐上游 `BuiltinCommandSpec`）。
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub command: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub arg_hint: &'static str,
    pub lifecycle: CommandLifecycle,
    pub accepts_args: bool,
}

use CommandLifecycle::*;

/// 内置命令规格表（顺序对齐上游 `BUILTIN_COMMAND_SPECS`，帮助文本据此生成）。
pub fn builtin_command_specs() -> &'static [CommandSpec] {
    &SPECS
}

const SPECS: [CommandSpec; 15] = [
    spec(
        "/new",
        "New chat",
        "Reset this chat and start a fresh conversation.",
        "square-pen",
        "",
        FinalizeActiveTurn,
        false,
    ),
    spec(
        "/stop",
        "Stop current task",
        "Cancel the active agent turn for this chat.",
        "square",
        "",
        StopActiveTurn,
        false,
    ),
    spec(
        "/restart",
        "Restart nanobot",
        "Restart the bot process.",
        "rotate-cw",
        "",
        SideChannel,
        false,
    ),
    spec(
        "/status",
        "Show status",
        "Display runtime, provider, and channel status.",
        "activity",
        "",
        SideChannel,
        false,
    ),
    spec(
        "/model",
        "Switch model preset",
        "Show or switch the active model preset.",
        "brain",
        "[preset]",
        SideChannel,
        true,
    ),
    spec(
        "/history",
        "Show conversation history",
        "Print the last N persisted conversation messages.",
        "history",
        "[n]",
        SideChannel,
        true,
    ),
    spec(
        "/goal",
        "Start long-running goal",
        "Tell the agent to treat the request as a long-running goal.",
        "activity",
        "<goal>",
        AgentTurnWithArgs,
        true,
    ),
    spec(
        "/trigger",
        "Create named local trigger",
        "Create a named CLI trigger bound to this chat session.",
        "zap",
        "<name>",
        SideChannel,
        true,
    ),
    spec(
        "/dream",
        "Run Dream",
        "Manually trigger memory consolidation.",
        "sparkles",
        "",
        SideChannel,
        false,
    ),
    spec(
        "/dream-log",
        "Show Dream log",
        "Show what the last Dream consolidation changed.",
        "book-open",
        "",
        SideChannel,
        true,
    ),
    spec(
        "/dream-restore",
        "Restore memory",
        "Revert memory to a previous Dream snapshot.",
        "undo-2",
        "",
        SideChannel,
        true,
    ),
    spec(
        "/dream-prompt",
        "Dream memory",
        "Tell Dream how to organize this workspace's memory.",
        "file-text",
        "[init]",
        SideChannel,
        true,
    ),
    spec(
        "/evaluator-prompt",
        "Heartbeat evaluator",
        "Customize the heartbeat notification gate prompt for this workspace.",
        "file-text",
        "[init]",
        SideChannel,
        true,
    ),
    spec(
        "/skill",
        "List skills",
        "List all enabled skills available to the agent.",
        "wrench",
        "",
        SideChannel,
        false,
    ),
    spec(
        "/help",
        "Show help",
        "List available slash commands.",
        "circle-help",
        "",
        SideChannel,
        false,
    ),
];

// `/pairing` 规格单列（数组长度对齐后追加，避免 const 数组过长影响可读）。
const PAIRING_SPEC: CommandSpec = spec(
    "/pairing",
    "Manage pairing",
    "List, approve, deny or revoke pairing requests.",
    "shield",
    "[list|approve <code>|deny <code>|revoke <user_id>]",
    SideChannel,
    true,
);

const fn spec(
    command: &'static str,
    title: &'static str,
    description: &'static str,
    icon: &'static str,
    arg_hint: &'static str,
    lifecycle: CommandLifecycle,
    accepts_args: bool,
) -> CommandSpec {
    CommandSpec {
        command,
        title,
        description,
        icon,
        arg_hint,
        lifecycle,
        accepts_args,
    }
}

/// 生成跨 channel 共享的帮助文本（对齐上游 `build_help_text`）。
pub fn build_help_text() -> String {
    let mut lines = vec!["🐈 nanobot commands:".to_string()];
    for s in SPECS.iter().chain(std::iter::once(&PAIRING_SPEC)) {
        let command = if s.arg_hint.is_empty() {
            s.command.to_string()
        } else {
            format!("{} {}", s.command, s.arg_hint)
        };
        lines.push(format!("{command} — {}", s.description));
    }
    lines.join("\n")
}

/// 内置命令注册所需的共享依赖。
pub struct BuiltinDeps {
    /// 配对码存储（`/pairing` 使用）。
    pub pairing: Arc<PairingStore>,
    /// 墙钟秒时钟（可注入，便于测试；生产用 `pairing::now_secs`）。
    pub clock: Arc<dyn Fn() -> f64 + Send + Sync>,
    /// skills 装载器（`/skill` 使用）；`None` 时 `/skill` 回落占位处理器。
    pub skills: Option<Arc<SkillsLoader>>,
}

/// 注册默认 slash 命令集合（层级顺序对齐上游 `register_builtin_commands`）。
pub fn register_builtin_commands(router: &mut CommandRouter, deps: BuiltinDeps) {
    let BuiltinDeps {
        pairing,
        clock,
        skills,
    } = deps;

    // priority 层：锁前处理。
    router.priority("/stop", pending_handler("/stop"));
    router.priority("/restart", pending_handler("/restart"));
    router.priority("/status", pending_handler("/status"));

    // exact / prefix 层。
    router.exact("/new", pending_handler("/new"));
    router.exact("/status", pending_handler("/status"));
    router.exact("/model", pending_handler("/model"));
    router.prefix("/model ", pending_handler("/model"));
    router.exact("/history", pending_handler("/history"));
    router.prefix("/history ", pending_handler("/history"));
    router.exact("/goal", pending_handler("/goal"));
    router.prefix("/goal ", pending_handler("/goal"));
    router.exact("/trigger", pending_handler("/trigger"));
    router.prefix("/trigger ", pending_handler("/trigger"));
    router.exact("/dream", pending_handler("/dream"));
    router.exact("/dream-log", pending_handler("/dream-log"));
    router.prefix("/dream-log ", pending_handler("/dream-log"));
    router.exact("/dream-restore", pending_handler("/dream-restore"));
    router.prefix("/dream-restore ", pending_handler("/dream-restore"));
    router.exact("/dream-prompt", pending_handler("/dream-prompt"));
    router.prefix("/dream-prompt ", pending_handler("/dream-prompt"));
    router.exact("/evaluator-prompt", pending_handler("/evaluator-prompt"));
    router.prefix("/evaluator-prompt ", pending_handler("/evaluator-prompt"));
    match skills {
        Some(loader) => router.exact("/skill", skill_handler(loader)),
        None => router.exact("/skill", pending_handler("/skill")),
    }
    router.exact("/help", help_handler());
    router.exact("/pairing", pairing_handler(pairing.clone(), clock.clone()));
    router.prefix("/pairing ", pairing_handler(pairing, clock));
}

/// `/help` 处理器：返回帮助文本，metadata `render_as=text`。
fn help_handler() -> CommandHandler {
    Box::new(|ctx: &CommandContext| {
        Some(
            CommandOutput::text(ctx, build_help_text()).with_meta("render_as", Value::from("text")),
        )
    })
}

/// `/pairing` 处理器：委托 [`PairingStore::handle_pairing_command`]，metadata 打配对命令标记。
fn pairing_handler(
    pairing: Arc<PairingStore>,
    clock: Arc<dyn Fn() -> f64 + Send + Sync>,
) -> CommandHandler {
    Box::new(move |ctx: &CommandContext| {
        let now = clock();
        let content = match pairing.handle_pairing_command(&ctx.channel, &ctx.args, now) {
            Ok(reply) => reply,
            Err(err) => format!("Pairing store error: {err}"),
        };
        Some(
            CommandOutput::text(ctx, content)
                .with_meta(PAIRING_COMMAND_META_KEY, Value::Bool(true)),
        )
    })
}

/// `/skill` 处理器：列出可用 skills（名称 + 描述），对齐上游 `cmd_skill`。
fn skill_handler(skills: Arc<SkillsLoader>) -> CommandHandler {
    Box::new(move |ctx: &CommandContext| {
        let list = skills.list_skills(false);
        let content = if list.is_empty() {
            "No skills available.".to_string()
        } else {
            let mut lines = vec![format!("Available skills ({}):", list.len()), String::new()];
            for entry in &list {
                lines.push(format!(
                    "- **{}** — {}",
                    entry.name,
                    skills.skill_description(&entry.name)
                ));
            }
            lines.join("\n")
        };
        Some(CommandOutput::text(ctx, content))
    })
}

/// 运行时依赖型命令的占位处理器：命令名已登记（使谓词对齐），行为待对应子系统接入。
fn pending_handler(cmd: &'static str) -> CommandHandler {
    Box::new(move |ctx: &CommandContext| {
        Some(CommandOutput::text(
            ctx,
            format!("命令 `{cmd}` 已登记但尚未在 lure 接入（随对应运行时/子系统落地）。"),
        ))
    })
}
