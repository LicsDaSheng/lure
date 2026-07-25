//! workspace 内的搜索工具：`list_dir`（目录列举）与 `grep`（内容搜索）。
//!
//! 对齐上游 `nanobot/agent/tools/filesystem.py::ListDirTool` 与
//! `nanobot/agent/tools/search.py::GrepTool` 的核心行为：忽略噪声目录、workspace 相对
//! 路径展示、files_with_matches（按 mtime 降序）/ content（带上下文）两种输出、glob/type
//! 过滤、pagination。context 计数/二进制跳过/size 截断等留待后续。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use regex::RegexBuilder;
use serde_json::{json, Value};

use crate::security::resolve_allowed_path;
use crate::tool::registry::Tool;
use crate::tool::result::ToolResult;

/// 自动忽略的噪声目录（对齐上游 `_IGNORE_DIRS`）。
const IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "htmlcov",
];

/// list_dir 默认最大条目数。
const LIST_DEFAULT_MAX: usize = 200;
/// grep 默认分页上限。
const GREP_DEFAULT_HEAD_LIMIT: usize = 250;

fn resolve_in_workspace(path: &str, workspace: &Path) -> Result<PathBuf, ToolResult> {
    resolve_allowed_path(Path::new(path), Some(workspace), Some(workspace), &[], &[])
        .map_err(|e| ToolResult::error(e.to_string()))
}

fn is_ignored_dir(name: &str) -> bool {
    IGNORE_DIRS.contains(&name)
}

/// 列举 workspace 内目录内容。
pub struct ListDirTool {
    workspace: PathBuf,
}

impl ListDirTool {
    /// 绑定 workspace。
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "列举目录内容；recursive=true 递归。自动忽略 .git/node_modules 等噪声目录。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "相对 workspace 的目录路径"},
                "recursive": {"type": "boolean", "description": "递归列举（默认否）"},
                "max_entries": {"type": "integer", "description": "最大条目数（默认 200）"}
            },
            "required": ["path"]
        })
    }

    fn read_only(&self) -> bool {
        true
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let Some(path) = args.get("path").and_then(Value::as_str) else {
            return ToolResult::error("Error listing directory: Unknown path");
        };
        let recursive = args
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let cap = args
            .get("max_entries")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .filter(|n| *n > 0)
            .unwrap_or(LIST_DEFAULT_MAX);

        let resolved = match resolve_in_workspace(path, &self.workspace) {
            Ok(p) => p,
            Err(result) => return result,
        };
        if !resolved.exists() {
            return ToolResult::error(format!("Error: Directory not found: {path}"));
        }
        if !resolved.is_dir() {
            return ToolResult::error(format!("Error: Not a directory: {path}"));
        }

        let mut entries = Vec::new();
        let mut total = 0usize;
        if recursive {
            let mut collected = Vec::new();
            collect_recursive(&resolved, &resolved, &mut collected);
            collected.sort();
            for entry in collected {
                total += 1;
                if entries.len() < cap {
                    entries.push(entry);
                }
            }
        } else {
            let mut items = match read_sorted_dir(&resolved) {
                Ok(items) => items,
                Err(e) => return ToolResult::error(format!("Error listing directory: {e}")),
            };
            items.retain(|(name, _)| !is_ignored_dir(name));
            for (name, is_dir) in items {
                total += 1;
                if entries.len() < cap {
                    let prefix = if is_dir { "📁 " } else { "📄 " };
                    entries.push(format!("{prefix}{name}"));
                }
            }
        }

        if entries.is_empty() && total == 0 {
            return ToolResult::ok(format!("Directory {path} is empty"));
        }

        let mut result = entries.join("\n");
        if total > cap {
            result.push_str(&format!(
                "\n\n(truncated, showing first {cap} of {total} entries)"
            ));
        }
        ToolResult::ok(result)
    }
}

/// 读取目录并按名排序，返回 `(name, is_dir)`。
fn read_sorted_dir(dir: &Path) -> std::io::Result<Vec<(String, bool)>> {
    let mut items: Vec<(String, bool)> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            (e.file_name().to_string_lossy().into_owned(), is_dir)
        })
        .collect();
    items.sort();
    items
        .into_iter()
        .map(Ok)
        .collect::<std::io::Result<Vec<_>>>()
}

/// 递归收集 `dir` 下所有条目，输出相对 `root` 的 posix 路径（目录带尾 `/`）；跳过噪声目录。
fn collect_recursive(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(items) = read_sorted_dir(dir) else {
        return;
    };
    for (name, is_dir) in items {
        if is_ignored_dir(&name) {
            continue;
        }
        let child = dir.join(&name);
        let rel = child
            .strip_prefix(root)
            .unwrap_or(&child)
            .to_string_lossy()
            .replace('\\', "/");
        if is_dir {
            out.push(format!("{rel}/"));
            collect_recursive(&child, root, out);
        } else {
            out.push(rel);
        }
    }
}

/// 在 workspace 内按正则搜索文件内容。
pub struct GrepTool {
    workspace: PathBuf,
}

impl GrepTool {
    /// 绑定 workspace。
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "按正则搜索文件内容。默认 output_mode=files_with_matches（仅路径），content 模式返回带上下文的匹配行。支持 glob/type 过滤。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {"type": "string", "description": "正则或纯文本模式"},
                "path": {"type": "string", "description": "搜索的文件或目录（默认 '.'）"},
                "glob": {"type": "string", "description": "文件过滤，如 '*.py'"},
                "type": {"type": "string", "description": "文件类型简写，如 'py'"},
                "case_insensitive": {"type": "boolean"},
                "fixed_strings": {"type": "boolean", "description": "按纯文本处理（默认否）"},
                "output_mode": {"type": "string", "enum": ["content", "files_with_matches"]},
                "context_before": {"type": "integer"},
                "context_after": {"type": "integer"},
                "head_limit": {"type": "integer"},
                "offset": {"type": "integer"}
            },
            "required": ["pattern"]
        })
    }

    fn read_only(&self) -> bool {
        true
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let Some(pattern) = args.get("pattern").and_then(Value::as_str) else {
            return ToolResult::error("Error searching files: Unknown pattern");
        };
        let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
        let glob = args.get("glob").and_then(Value::as_str);
        let file_type = args.get("type").and_then(Value::as_str);
        let case_insensitive = args
            .get("case_insensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let fixed_strings = args
            .get("fixed_strings")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let output_mode = args
            .get("output_mode")
            .and_then(Value::as_str)
            .unwrap_or("files_with_matches");
        let context_before = args
            .get("context_before")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let context_after = args
            .get("context_after")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        // head_limit=0 表示不限；缺省用默认上限。
        let limit = match args.get("head_limit").and_then(Value::as_u64) {
            Some(0) => None,
            Some(n) => Some(n as usize),
            None => Some(GREP_DEFAULT_HEAD_LIMIT),
        };

        let resolved = match resolve_in_workspace(path, &self.workspace) {
            Ok(p) => p,
            Err(result) => return result,
        };
        if !resolved.exists() {
            return ToolResult::error(format!("Error: Path not found: {path}"));
        }

        let needle = if fixed_strings {
            regex::escape(pattern)
        } else {
            pattern.to_string()
        };
        let regex = match RegexBuilder::new(&needle)
            .case_insensitive(case_insensitive)
            .build()
        {
            Ok(r) => r,
            Err(e) => return ToolResult::error(format!("Error: invalid regex pattern: {e}")),
        };

        // 展示路径基准：规范化的 workspace 根（resolved 已规范化，需同基准才能 strip）。
        let ws_root = fs::canonicalize(&self.workspace).unwrap_or_else(|_| self.workspace.clone());

        // 遍历候选文件（相对 workspace 的 posix 展示路径 + mtime）。
        let mut files = Vec::new();
        collect_files(&resolved, &mut files);
        files.sort();

        let mut matching: Vec<(String, u64)> = Vec::new(); // (display, mtime_secs)
        let mut blocks: Vec<String> = Vec::new();
        let mut seen_content = 0usize;
        let mut content_truncated = false;

        for file in &files {
            let display = display_path(file, &ws_root);
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Some(g) = glob {
                if !match_glob(&display, &name, g) {
                    continue;
                }
            }
            if !matches_type(&name, file_type) {
                continue;
            }
            let Ok(content) = fs::read_to_string(file) else {
                continue;
            };
            let lines: Vec<&str> = content.lines().collect();

            if output_mode == "files_with_matches" {
                if lines.iter().any(|l| regex.is_match(l)) {
                    matching.push((display.clone(), mtime_secs(file)));
                }
                continue;
            }

            // content 模式：逐匹配行输出块。
            for (idx, line) in lines.iter().enumerate() {
                if !regex.is_match(line) {
                    continue;
                }
                seen_content += 1;
                if seen_content <= offset {
                    continue;
                }
                if let Some(lim) = limit {
                    if blocks.len() >= lim {
                        content_truncated = true;
                        break;
                    }
                }
                blocks.push(format_block(
                    &display,
                    &lines,
                    idx + 1,
                    context_before,
                    context_after,
                ));
            }
            if content_truncated {
                break;
            }
        }

        if output_mode == "files_with_matches" {
            if matching.is_empty() {
                return ToolResult::ok(format!(
                    "No matches found for pattern '{pattern}' in {path}"
                ));
            }
            // 按 (mtime 降序, 名称升序) 排序。
            matching.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let names: Vec<String> = matching.into_iter().map(|(name, _)| name).collect();
            let (paged, truncated) = paginate(&names, limit, offset);
            let mut result = paged
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if let Some(note) = pagination_note(limit, offset, truncated) {
                result.push_str(&format!("\n\n{note}"));
            }
            return ToolResult::ok(result);
        }

        if blocks.is_empty() {
            return ToolResult::ok(format!(
                "No matches found for pattern '{pattern}' in {path}"
            ));
        }
        let mut result = blocks.join("\n\n");
        if content_truncated {
            result.push_str(&format!(
                "\n\n(pagination: limit={}, offset={offset})",
                limit.unwrap_or(0)
            ));
        } else if offset > 0 {
            result.push_str(&format!("\n\n(pagination: offset={offset})"));
        }
        ToolResult::ok(result)
    }
}

/// content 模式单个匹配块：`display:matchline` 头 + 上下文（匹配行以 `>` 标记）。
fn format_block(
    display: &str,
    lines: &[&str],
    match_line: usize,
    before: usize,
    after: usize,
) -> String {
    let start = match_line.saturating_sub(before).max(1);
    let end = (match_line + after).min(lines.len());
    let mut block = vec![format!("{display}:{match_line}")];
    for line_no in start..=end {
        let marker = if line_no == match_line { '>' } else { ' ' };
        block.push(format!("{marker} {line_no}| {}", lines[line_no - 1]));
    }
    block.join("\n")
}

/// 递归收集 `root` 下所有文件（跳过噪声目录）；`root` 为文件时仅含自身。
fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    if root.is_file() {
        out.push(root.to_path_buf());
        return;
    }
    let Ok(items) = read_sorted_dir(root) else {
        return;
    };
    for (name, is_dir) in items {
        if is_ignored_dir(&name) {
            continue;
        }
        let child = root.join(&name);
        if is_dir {
            collect_files(&child, out);
        } else {
            out.push(child);
        }
    }
}

/// workspace 相对 posix 展示路径；越界回退为完整路径。
fn display_path(file: &Path, workspace: &Path) -> String {
    file.strip_prefix(workspace)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 文件 mtime（Unix 秒）；不可用为 0。
fn mtime_secs(file: &Path) -> u64 {
    fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// glob 匹配：含 `/` 或以 `**` 开头时对相对路径匹配，否则对文件名匹配。
fn match_glob(rel_path: &str, name: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if pattern.contains('/') || pattern.starts_with("**") {
        glob_to_regex(pattern, true)
            .map(|re| re.is_match(rel_path))
            .unwrap_or(false)
    } else {
        glob_to_regex(pattern, false)
            .map(|re| re.is_match(name))
            .unwrap_or(false)
    }
}

/// 把 glob 编译为锚定 regex：`**`→`.*`，`*`→`[^/]*`（path）或 `.*`（name），`?`→单字符。
fn glob_to_regex(pattern: &str, path_mode: bool) -> Option<regex::Regex> {
    let mut re = String::from("^");
    let star = if path_mode { "[^/]*" } else { ".*" };
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if path_mode && chars.peek() == Some(&'*') {
                    chars.next();
                    re.push_str(".*");
                } else {
                    re.push_str(star);
                }
            }
            '?' => re.push_str(if path_mode { "[^/]" } else { "." }),
            _ => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    regex::Regex::new(&re).ok()
}

/// 类型简写过滤：命中扩展名（默认 `*.{type}`）。
fn matches_type(name: &str, file_type: Option<&str>) -> bool {
    let Some(t) = file_type.map(str::trim).filter(|t| !t.is_empty()) else {
        return true;
    };
    let lowered = name.to_lowercase();
    lowered.ends_with(&format!(".{}", t.to_lowercase()))
}

/// 分页：`limit=None` 从 offset 起全取；否则取 `[offset, offset+limit)` 并标记是否截断。
fn paginate(items: &[String], limit: Option<usize>, offset: usize) -> (Vec<&String>, bool) {
    let rest: Vec<&String> = items.iter().skip(offset).collect();
    match limit {
        None => (rest, false),
        Some(lim) => {
            let truncated = items.len() > offset + lim;
            (rest.into_iter().take(lim).collect(), truncated)
        }
    }
}

/// 分页提示行（对齐上游 `_pagination_note`）。
fn pagination_note(limit: Option<usize>, offset: usize, truncated: bool) -> Option<String> {
    if truncated {
        return Some(match limit {
            Some(lim) => format!("(pagination: limit={lim}, offset={offset})"),
            None => format!("(pagination: offset={offset})"),
        });
    }
    if offset > 0 {
        return Some(format!("(pagination: offset={offset})"));
    }
    None
}
