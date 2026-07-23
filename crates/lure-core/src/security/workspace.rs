//! workspace 路径边界守卫。
//!
//! 对齐上游 `nanobot/security/workspace_policy.py`：这是应用层守卫，让路径决策在各
//! 工具间一致，但不替代 OS 沙箱。
//!
//! - `resolve_path`：相对路径按 workspace 解释，随后软解析（跟随已存在部分的符号链接）。
//! - `is_path_within`：目标是否解析到 root 或其子孙。
//! - `resolve_allowed_path`：解析并强制落在允许 root/文件内，否则 [`WorkspaceBoundaryError`]。

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// workspace 边界说明（提示这是硬策略边界，不是瞬时失败）。
pub const WORKSPACE_BOUNDARY_NOTE: &str =
    " (this is a hard policy boundary, not a transient failure)";

/// 请求路径逃逸出允许 workspace 边界时返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceBoundaryError {
    /// 人类可读的错误消息。
    pub message: String,
}

impl fmt::Display for WorkspaceBoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WorkspaceBoundaryError {}

/// 展开 `~` 前缀。
fn expand_user(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if text == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = text.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

/// 把路径按 workspace 解释为绝对（仍是词法层，不解析符号链接）。
fn lexical_join(path: &Path, workspace: Option<&Path>) -> PathBuf {
    let candidate = expand_user(path);
    if candidate.is_relative() {
        if let Some(ws) = workspace {
            return expand_user(ws).join(candidate);
        }
    }
    candidate
}

/// 纯词法归一：解析 `.`/`..`，相对路径按 cwd 展开，不触碰文件系统。
fn logical_normalize(path: PathBuf) -> PathBuf {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 软解析：规范化最长已存在祖先（跟随符号链接），再拼接不存在的尾部。
///
/// 对齐 Python `Path.resolve(strict=False)`：已存在部分跟随符号链接，尾部按词法拼接。
fn resolve_soft(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(canonical) = fs::canonicalize(&existing) {
            let mut result = canonical;
            for name in tail.iter().rev() {
                result.push(name);
            }
            return result;
        }
        match existing.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                if !existing.pop() {
                    break;
                }
            }
            None => break,
        }
    }
    logical_normalize(path.to_path_buf())
}

/// 解析路径（相对按 workspace 解释），跟随已存在部分的符号链接。
pub fn resolve_path(path: &Path, workspace: Option<&Path>) -> PathBuf {
    resolve_soft(&lexical_join(path, workspace))
}

/// 目标是否解析到 root 或其子孙（按路径分量比较，避免字符串前缀误判）。
pub fn is_path_within(path: &Path, root: &Path) -> bool {
    let resolved_path = resolve_soft(&expand_user(path));
    let resolved_root = resolve_soft(&expand_user(root));
    resolved_path.starts_with(&resolved_root)
}

/// 目标是否落在任一允许 root 内。
pub fn is_path_allowed<'a>(path: &Path, roots: impl IntoIterator<Item = &'a Path>) -> bool {
    roots.into_iter().any(|root| is_path_within(path, root))
}

/// 精确文件允许：请求软解析后与某个允许文件的软解析逐字相等。
///
/// 说明：上游还会用词法路径 vs 软解析路径的差异来拦截“经由额外允许文件的符号链接
/// 逃逸”；此处为跨平台稳健先用软解析对比（base 符号链接如 macOS `/var` 不影响），
/// 该 escape 边界留待后续（见 upstream-test-ledger）。
fn is_path_exactly_allowed(resolved: &Path, files: &[PathBuf]) -> bool {
    files
        .iter()
        .any(|file| resolve_soft(&expand_user(file)) == *resolved)
}

/// 解析路径并强制落在允许 root/文件内。
///
/// `allowed_root` 为 `None` 且无允许文件时不做边界检查，直接返回解析结果。
pub fn resolve_allowed_path(
    path: &Path,
    workspace: Option<&Path>,
    allowed_root: Option<&Path>,
    extra_allowed_roots: &[PathBuf],
    extra_allowed_files: &[PathBuf],
) -> Result<PathBuf, WorkspaceBoundaryError> {
    let resolved = resolve_path(path, workspace);

    if allowed_root.is_none() && extra_allowed_files.is_empty() {
        return Ok(resolved);
    }

    let mut roots: Vec<&Path> = Vec::new();
    if let Some(root) = allowed_root {
        roots.push(root);
    }
    roots.extend(extra_allowed_roots.iter().map(PathBuf::as_path));

    let exact_allowed =
        !extra_allowed_files.is_empty() && is_path_exactly_allowed(&resolved, extra_allowed_files);

    if !is_path_allowed(&resolved, roots.iter().copied()) && !exact_allowed {
        let boundary = allowed_root
            .map(|r| expand_user(r).display().to_string())
            .unwrap_or_else(|| "allowed files".to_string());
        return Err(WorkspaceBoundaryError {
            message: format!(
                "Path {} is outside allowed directory {boundary}{WORKSPACE_BOUNDARY_NOTE}",
                path.display()
            ),
        });
    }

    Ok(resolved)
}
