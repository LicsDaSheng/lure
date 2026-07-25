//! 受 workspace 约束的文件工具。
//!
//! 对齐上游文件工具的安全边界：读写路径必须落在 workspace 内，越界返回结构化错误
//! （而非静默或 shell 绕过）。含 read/write/edit；search/apply_patch 留待后续。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::security::resolve_allowed_path;
use crate::tool::registry::Tool;
use crate::tool::result::{truncate_result, ToolResult};

/// 文件读取结果最大字符数。
const MAX_READ_CHARS: usize = 50_000;

/// 解析并强制路径落在 workspace 内。
fn resolve_in_workspace(path: &str, workspace: &Path) -> Result<PathBuf, ToolResult> {
    resolve_allowed_path(Path::new(path), Some(workspace), Some(workspace), &[], &[])
        .map_err(|e| ToolResult::error(e.to_string()))
}

/// 读取 workspace 内文件。
pub struct ReadFileTool {
    workspace: PathBuf,
}

impl ReadFileTool {
    /// 绑定 workspace。
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "读取 workspace 内的文本文件。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {"path": {"type": "string", "description": "相对 workspace 的路径"}},
            "required": ["path"]
        })
    }

    fn read_only(&self) -> bool {
        true
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return ToolResult::error("缺少 path 参数");
        };
        let resolved = match resolve_in_workspace(path, &self.workspace) {
            Ok(p) => p,
            Err(result) => return result,
        };
        match fs::read_to_string(&resolved) {
            Ok(text) => ToolResult::ok(truncate_result(&text, MAX_READ_CHARS)),
            Err(e) => ToolResult::error(format!("读取失败 {}: {e}", resolved.display())),
        }
    }
}

/// 写入 workspace 内文件。
pub struct WriteFileTool {
    workspace: PathBuf,
}

impl WriteFileTool {
    /// 绑定 workspace。
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "写入 workspace 内的文本文件（必要时创建父目录）。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "相对 workspace 的路径"},
                "content": {"type": "string", "description": "写入内容"}
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return ToolResult::error("缺少 path 参数");
        };
        let Some(content) = args.get("content").and_then(Value::as_str) else {
            return ToolResult::error("缺少 content 参数");
        };
        let resolved = match resolve_in_workspace(path, &self.workspace) {
            Ok(p) => p,
            Err(result) => return result,
        };
        if let Some(parent) = resolved.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult::error(format!("创建目录失败 {}: {e}", parent.display()));
            }
        }
        match fs::write(&resolved, content) {
            Ok(()) => ToolResult::ok(format!("已写入 {} 字节到 {}", content.len(), path)),
            Err(e) => ToolResult::error(format!("写入失败 {}: {e}", resolved.display())),
        }
    }
}

/// 在 `content` 中定位 `old_text`，返回 `(实际匹配文本, 匹配数)`。
///
/// 对齐上游 `_find_match`：先尝试精确子串匹配；无精确命中时退回**逐行 trim** 匹配
/// （忽略每行首尾空白，按行窗口比较），返回的匹配文本保留原始缩进。空 `old_text`
/// 视为总能精确命中（返回空串）。
pub fn find_match(content: &str, old_text: &str) -> (Option<String>, usize) {
    if old_text.is_empty() {
        return (Some(String::new()), 1);
    }

    // 精确子串（非重叠计数）。
    let exact = content.matches(old_text).count();
    if exact > 0 {
        return (Some(old_text.to_string()), exact);
    }

    // 逐行 trim 回退：按行窗口比较去空白后的内容。
    let stripped_old: Vec<&str> = old_text.lines().map(str::trim).collect();
    if stripped_old.is_empty() {
        return (None, 0);
    }
    let content_lines: Vec<&str> = content.lines().collect();
    let window = stripped_old.len();
    if content_lines.len() < window {
        return (None, 0);
    }

    let mut count = 0;
    let mut first_match = None;
    for start in 0..=content_lines.len() - window {
        let comparable: Vec<&str> = content_lines[start..start + window]
            .iter()
            .map(|l| l.trim())
            .collect();
        if comparable == stripped_old {
            count += 1;
            if first_match.is_none() {
                first_match = Some(content_lines[start..start + window].join("\n"));
            }
        }
    }
    if count == 0 {
        (None, 0)
    } else {
        (first_match, count)
    }
}

/// 就地编辑 workspace 内文件：以 `old_text` 定位并替换为 `new_text`。
///
/// 对齐上游 `EditFileTool`：CRLF 归一后匹配、写回时保留原行尾；`old_text` 多处命中且未
/// 指定 `replace_all` 时告警不写；未命中 / 缺 `new_text` 返回明确错误。
pub struct EditFileTool {
    workspace: PathBuf,
}

impl EditFileTool {
    /// 绑定 workspace。
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "编辑 workspace 内文本文件：定位 old_text 并替换为 new_text。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "相对 workspace 的路径"},
                "old_text": {"type": "string", "description": "要替换的原文"},
                "new_text": {"type": "string", "description": "替换后的新文本"},
                "replace_all": {"type": "boolean", "description": "替换全部匹配（默认否）"}
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return ToolResult::error("缺少 path 参数");
        };
        let Some(old_text) = args.get("old_text").and_then(Value::as_str) else {
            return ToolResult::error("Error editing file: Unknown old_text");
        };
        let Some(new_text) = args.get("new_text").and_then(Value::as_str) else {
            return ToolResult::error("Error editing file: Unknown new_text");
        };
        let replace_all = args
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let resolved = match resolve_in_workspace(path, &self.workspace) {
            Ok(p) => p,
            Err(result) => return result,
        };
        let raw = match fs::read_to_string(&resolved) {
            Ok(text) => text,
            Err(e) => return ToolResult::error(format!("读取失败 {}: {e}", resolved.display())),
        };

        // CRLF 归一后匹配；写回时按原行尾还原。
        let had_crlf = raw.contains("\r\n");
        let normalized = raw.replace("\r\n", "\n");

        let (matched, count) = find_match(&normalized, old_text);
        let Some(matched) = matched.filter(|_| count > 0) else {
            return ToolResult::error("Error editing file: old_text not found");
        };
        if count > 1 && !replace_all {
            return ToolResult::error(format!(
                "Warning: old_text appears {count} times; pass replace_all=true or add more context"
            ));
        }

        let edited = if replace_all {
            normalized.replace(&matched, new_text)
        } else {
            normalized.replacen(&matched, new_text, 1)
        };
        let output = if had_crlf {
            edited.replace('\n', "\r\n")
        } else {
            edited
        };

        match fs::write(&resolved, output) {
            Ok(()) => ToolResult::ok(format!("Successfully edited {path}")),
            Err(e) => ToolResult::error(format!("写入失败 {}: {e}", resolved.display())),
        }
    }
}
