//! 最小 slash 命令路由表。
//!
//! 对齐上游 `nanobot/command/router.py`：三层派发（priority / exact / prefix，最长前缀优先）
//! 加 `normalize_command_text`（剥离 `@bot` 传输后缀）。lure 为同步实现（上游 async），
//! handler 为 `Fn(&CommandContext) -> Option<CommandOutput>` 的装箱闭包。

use std::collections::BTreeMap;

use serde_json::Value;

/// 命令处理器：读取上下文，产出可选回复。
pub type CommandHandler = Box<dyn Fn(&CommandContext) -> Option<CommandOutput> + Send + Sync>;

/// 一条命令处理所需的上下文（对齐上游 `CommandContext` 的传输相关字段）。
///
/// 运行时相关字段（session/loop/runtime）随对应子系统接入时再扩展，保持公开面最小。
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// 来源 channel。
    pub channel: String,
    /// 来源会话 chat_id。
    pub chat_id: String,
    /// 会话 key（`channel:chat_id`）。
    pub key: String,
    /// 归一后的原始命令文本（dispatch 时就地归一）。
    pub raw: String,
    /// 前缀命令的参数尾串（exact 命中时为空）。
    pub args: String,
}

impl CommandContext {
    /// 构造上下文（`raw` 尚未归一，`args` 初始为空，由 dispatch 填充）。
    pub fn new(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        key: impl Into<String>,
        raw: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            chat_id: chat_id.into(),
            key: key.into(),
            raw: raw.into(),
            args: String::new(),
        }
    }
}

/// 命令处理器的产出（对齐上游 `OutboundMessage` 的命令回复子集）。
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutput {
    pub channel: String,
    pub chat_id: String,
    pub content: String,
    pub metadata: BTreeMap<String, Value>,
}

impl CommandOutput {
    /// 以 ctx 的 channel/chat_id 构造纯文本回复。
    pub fn text(ctx: &CommandContext, content: impl Into<String>) -> Self {
        Self {
            channel: ctx.channel.clone(),
            chat_id: ctx.chat_id.clone(),
            content: content.into(),
            metadata: BTreeMap::new(),
        }
    }

    /// 附加一条 metadata 键值。
    pub fn with_meta(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// 纯字典式命令派发，三层按序检查（priority → exact → prefix）。
#[derive(Default)]
pub struct CommandRouter {
    priority: BTreeMap<String, CommandHandler>,
    exact: BTreeMap<String, CommandHandler>,
    prefix: Vec<(String, CommandHandler)>,
}

impl CommandRouter {
    /// 空路由表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册 priority 命令（锁前处理，如 /stop、/restart）。
    pub fn priority(&mut self, cmd: &str, handler: CommandHandler) {
        self.priority.insert(cmd.to_lowercase(), handler);
    }

    /// 注册 exact 命令。
    pub fn exact(&mut self, cmd: &str, handler: CommandHandler) {
        self.exact.insert(cmd.to_lowercase(), handler);
    }

    /// 注册 prefix 命令（保持按前缀长度降序，最长优先）。
    pub fn prefix(&mut self, pfx: &str, handler: CommandHandler) {
        self.prefix.push((pfx.to_lowercase(), handler));
        self.prefix.sort_by_key(|k| std::cmp::Reverse(k.0.len()));
    }

    /// 归一后是否命中 priority 层。
    pub fn is_priority(&self, text: &str) -> bool {
        self.priority
            .contains_key(&normalize_command_text(text).to_lowercase())
    }

    /// 归一后是否命中非 priority 层（exact 或 prefix）。命中即保证 `dispatch` 有处理器。
    pub fn is_dispatchable_command(&self, text: &str) -> bool {
        let cmd = normalize_command_text(text).to_lowercase();
        if self.exact.contains_key(&cmd) {
            return true;
        }
        self.prefix.iter().any(|(pfx, _)| cmd.starts_with(pfx))
    }

    /// 派发 priority 命令（run 循环在锁外调用）。
    pub fn dispatch_priority(&self, ctx: &mut CommandContext) -> Option<CommandOutput> {
        ctx.raw = normalize_command_text(&ctx.raw);
        let handler = self.priority.get(&ctx.raw.to_lowercase())?;
        handler(ctx)
    }

    /// 先 exact 后 prefix 派发；未命中返回 `None`。
    pub fn dispatch(&self, ctx: &mut CommandContext) -> Option<CommandOutput> {
        ctx.raw = normalize_command_text(&ctx.raw);
        let cmd = ctx.raw.to_lowercase();

        if let Some(handler) = self.exact.get(&cmd) {
            return handler(ctx);
        }

        for (pfx, handler) in &self.prefix {
            if cmd.starts_with(pfx) {
                ctx.args = ctx.raw[pfx.len()..].to_string();
                return handler(ctx);
            }
        }
        None
    }
}

/// 归一 slash 命令的传输变体：剥离 `/cmd@bot args` 中属于传输层的 `@bot` 后缀。
///
/// 对齐上游：仅当首段含 `@` 且后缀为 `[A-Za-z0-9_]+` 时剥离，参数逐字保留；非命令仅去首尾空白。
pub fn normalize_command_text(text: &str) -> String {
    let stripped = text.trim();
    if !stripped.starts_with('/') {
        return stripped.to_string();
    }
    let (first, sep, rest) = match stripped.split_once(' ') {
        Some((f, r)) => (f, true, r),
        None => (stripped, false, ""),
    };
    if !first.contains('@') {
        return stripped.to_string();
    }
    let Some((command, suffix)) = first.rsplit_once('@') else {
        return stripped.to_string();
    };
    let suffix_ok = !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !command.is_empty() && suffix_ok {
        if sep {
            format!("{command} {rest}")
        } else {
            command.to_string()
        }
    } else {
        stripped.to_string()
    }
}
