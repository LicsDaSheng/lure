//! 受 workspace 约束的文件工具。
//!
//! 对齐上游文件工具的安全边界：读写路径必须落在 workspace 内，越界返回结构化错误
//! （而非静默或 shell 绕过）。Phase 5 只做 read/write 最小集；edit/search/apply_patch
//! 留待后续。

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
