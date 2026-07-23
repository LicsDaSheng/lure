//! shell 执行策略与工具。
//!
//! 对齐上游 `ExecTool._guard_command` 的 allow/deny 语义：
//! - `allow_patterns` 优先：每个顶层 shell 段都匹配某个 allow → 放行（跳过 deny）。
//! - deny 在**原始命令**上匹配（内建 + 用户追加）→ "deny pattern filter" 拦截。
//! - allowlist 模式下未全部匹配 allow → "allowlist" 拦截。
//!
//! 顶层分段按 `;`、`&&`、`||`、`|`、后台 `&` 切分，但保留 fd 重定向里的 `&`（如 `2>&1`）。

use std::process::Command;

use regex::Regex;
use serde_json::{json, Value};

use crate::tool::registry::Tool;
use crate::tool::result::{truncate_result, ToolResult};

/// 内建 deny 模式（代表性子集）。
const BUILTIN_DENY: &[&str] = &[r"\brm\s+-rf\b", r"\bmkfs\b", r"\bdd\s+if=", r">\s*/dev/sd"];

/// 工具结果最大字符数。
const MAX_EXEC_RESULT_CHARS: usize = 16_000;

/// shell 命令 allow/deny 策略。
pub struct ExecPolicy {
    allow: Vec<Regex>,
    deny: Vec<Regex>,
}

impl ExecPolicy {
    /// 以 allow/额外 deny 模式构造（额外 deny 追加到内建之后）。
    pub fn new(
        allow_patterns: &[&str],
        extra_deny_patterns: &[&str],
    ) -> Result<Self, regex::Error> {
        let allow = compile(allow_patterns)?;
        let mut deny = compile(BUILTIN_DENY)?;
        deny.extend(compile(extra_deny_patterns)?);
        Ok(Self { allow, deny })
    }

    /// 校验命令；`None` 表示放行，`Some(reason)` 表示拦截原因。
    pub fn guard_command(&self, command: &str) -> Option<String> {
        let segments = split_top_level_segments(command);

        // 1) allowlist 优先：每段都匹配 allow → 放行（跳过 deny）。
        if !self.allow.is_empty() {
            let all_allowed = segments
                .iter()
                .all(|segment| self.allow.iter().any(|re| re.is_match(segment)));
            if all_allowed {
                return None;
            }
        }

        // 2) deny 在原始命令上匹配。
        if self.deny.iter().any(|re| re.is_match(command)) {
            return Some(format!("命令被 deny pattern filter 拦截: {command}"));
        }

        // 3) allowlist 模式但未全部匹配。
        if !self.allow.is_empty() {
            return Some(format!("命令未通过 allowlist: {command}"));
        }

        None
    }
}

fn compile(patterns: &[&str]) -> Result<Vec<Regex>, regex::Error> {
    patterns.iter().map(|p| Regex::new(p)).collect()
}

/// 按顶层 shell 操作符切分命令，保留 fd 重定向中的 `&`。
fn split_top_level_segments(command: &str) -> Vec<String> {
    let chars: Vec<char> = command.chars().collect();
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if in_single {
            current.push(c);
            if c == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            current.push(c);
            if c == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }

        match c {
            '\'' => {
                in_single = true;
                current.push(c);
                i += 1;
            }
            '"' => {
                in_double = true;
                current.push(c);
                i += 1;
            }
            ';' => {
                segments.push(std::mem::take(&mut current));
                i += 1;
            }
            '|' => {
                segments.push(std::mem::take(&mut current));
                i += if chars.get(i + 1) == Some(&'|') { 2 } else { 1 };
            }
            '&' => {
                if chars.get(i + 1) == Some(&'&') {
                    segments.push(std::mem::take(&mut current));
                    i += 2;
                } else if is_fd_redirection(&chars, i) {
                    current.push(c);
                    i += 1;
                } else {
                    segments.push(std::mem::take(&mut current));
                    i += 1;
                }
            }
            _ => {
                current.push(c);
                i += 1;
            }
        }
    }
    segments.push(current);

    segments.into_iter().map(|s| s.trim().to_string()).collect()
}

/// `&` 是否属于 fd 重定向（前一个非空白是 `>`，或后一个非空白是 `>`）。
fn is_fd_redirection(chars: &[char], index: usize) -> bool {
    let prev = chars[..index].iter().rev().find(|c| !c.is_whitespace());
    let next = chars[index + 1..].iter().find(|c| !c.is_whitespace());
    prev == Some(&'>') || next == Some(&'>')
}

/// shell exec 工具：按策略门禁后在 workspace 内执行命令。
pub struct ExecTool {
    policy: ExecPolicy,
    workspace: std::path::PathBuf,
}

impl ExecTool {
    /// 绑定策略与执行目录。
    pub fn new(policy: ExecPolicy, workspace: impl Into<std::path::PathBuf>) -> Self {
        Self {
            policy,
            workspace: workspace.into(),
        }
    }
}

impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "在 workspace 内执行 shell 命令，受 allow/deny 策略约束。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "要执行的 shell 命令"}
            },
            "required": ["command"]
        })
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let Some(command) = args.get("command").and_then(Value::as_str) else {
            return ToolResult::error("缺少 command 参数");
        };

        if let Some(reason) = self.policy.guard_command(command) {
            return ToolResult::error(reason);
        }

        match Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.workspace)
            .output()
        {
            Ok(output) => {
                let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    combined.push_str(&stderr);
                }
                let content = truncate_result(&combined, MAX_EXEC_RESULT_CHARS);
                if output.status.success() {
                    ToolResult::ok(content)
                } else {
                    ToolResult::error(format!("命令退出码非零: {content}"))
                }
            }
            Err(e) => ToolResult::error(format!("命令执行失败: {e}")),
        }
    }
}
