//! OSC 8 超链接的打开策略。
//!
//! 终端内核只负责把超链接元数据带到单元格上；是否允许打开、打开什么目标，
//! 由展示层收敛。任何来自终端输出的 URI 都等价于把外部协议处理器暴露给
//! 该输出，所以这里采用显式白名单，而不是把未知 scheme 原样交给系统。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::links::LinkKind;

/// 允许交给系统默认方式打开的最长 URI。
pub const MAX_URI_BYTES: usize = 2047;

/// 一次待打开的链接目标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    /// 交给系统默认浏览器或邮件客户端打开的网络地址。
    Url(String),
    /// 交给系统默认应用打开的本地绝对路径。
    File(PathBuf),
}

/// 无法打开时保留原因，用于非阻塞提示；不影响终端内容与 Pi 进程。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// 空链接或超长链接。
    Empty,
    /// 不在白名单内的 scheme。
    UnsupportedScheme(String),
    /// `file:` 目标不是绝对路径或无法解析。
    InvalidPath(String),
}

impl OpenError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "链接为空或过长，已忽略".to_string(),
            Self::UnsupportedScheme(scheme) => {
                format!("不打开 {scheme} 链接，仅支持 http、https、mailto 与文件路径")
            }
            Self::InvalidPath(path) => format!("文件链接不是有效的绝对路径：{path}"),
        }
    }
}

/// 按链接来源解析为可打开的目标。
///
/// - [`LinkKind::Uri`] 与 [`LinkKind::Url`]：交给 [`resolve_target`] 按 scheme 校验。
/// - [`LinkKind::Path`]：文本识别得到的路径，可能相对；相对路径以 `base`
///   （当前项目目录）为基准解析，绝对路径与 `~/` 直接展开。
///
/// # Errors
///
/// 与 [`resolve_target`] 相同；相对路径在 `base` 为相对目录本身时也会失败。
pub fn resolve_link(kind: LinkKind, target: &str, base: &Path) -> Result<OpenTarget, OpenError> {
    if !matches!(kind, LinkKind::Path) {
        return resolve_target(target);
    }
    let target = target.trim();
    if target.is_empty() || target.len() > MAX_URI_BYTES {
        return Err(OpenError::Empty);
    }
    let expanded = expand_home(target);
    let path = PathBuf::from(&expanded);
    let absolute = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    if !absolute.is_absolute() {
        return Err(OpenError::InvalidPath(expanded));
    }
    Ok(OpenTarget::File(clean_path(&absolute)))
}

/// 把 `~/x` 展开为用户主目录；其他形式原样返回。
fn expand_home(target: &str) -> String {
    let Some(rest) = target.strip_prefix("~/") else {
        return target.to_string();
    };
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            Path::new(&home).join(rest).to_string_lossy().into_owned()
        }
        _ => target.to_string(),
    }
}

/// 解析 OSC 8 URI 为可打开的目标。
///
/// 支持 `http`、`https`、`mailto` 与 `file`。`file` 目标会去掉 `#L42` 这类
/// 编辑器行号片段后按绝对路径处理；相对路径没有明确的基准目录可比对，直接拒绝。
///
/// # Errors
///
/// - [`OpenError::Empty`]：链接为空或超过 [`MAX_URI_BYTES`]。
/// - [`OpenError::UnsupportedScheme`]：scheme 不在白名单内，或根本没有 scheme。
/// - [`OpenError::InvalidPath`]：`file:` 目标无法解码或不是绝对路径。
pub fn resolve_target(uri: &str) -> Result<OpenTarget, OpenError> {
    let uri = uri.trim();
    if uri.is_empty() || uri.len() > MAX_URI_BYTES {
        return Err(OpenError::Empty);
    }
    let Some((scheme, rest)) = uri.split_once(':') else {
        return Err(OpenError::UnsupportedScheme(uri.to_string()));
    };
    let scheme = scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" | "mailto" => Ok(OpenTarget::Url(uri.to_string())),
        "file" => parse_file_uri(rest).map(OpenTarget::File),
        _ => Err(OpenError::UnsupportedScheme(scheme)),
    }
}

/// 解析 `file:` 之后的部分，兼容 `file:///path`、`file://host/path` 与省略斜杠的写法。
fn parse_file_uri(rest: &str) -> Result<PathBuf, OpenError> {
    let trimmed = rest.trim_start_matches("//");
    // 保留 UNC 风格主机名之外的绝对路径；空主机（file:///path）直接得到 /path。
    let trimmed = match trimmed.strip_prefix('/') {
        Some(rest) => format!("/{}", strip_fragment(rest)),
        None => strip_fragment(trimmed).to_string(),
    };
    let decoded =
        percent_decode(&trimmed).ok_or_else(|| OpenError::InvalidPath(trimmed.clone()))?;
    let decoded = expand_home(&decoded);
    let path = PathBuf::from(&decoded);
    if !path.is_absolute() {
        return Err(OpenError::InvalidPath(decoded));
    }
    Ok(clean_path(&path))
}

/// 去掉 `#L42`、`?query` 等不属于文件系统的后缀。
fn strip_fragment(path: &str) -> &str {
    let end = path.find(['#', '?']).unwrap_or(path.len());
    &path[..end]
}

/// 折叠 `.` 与 `..`；不访问文件系统，缺失的路径仍然允许交给系统打开。
fn clean_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                cleaned.pop();
            }
            std::path::Component::CurDir => {}
            component => cleaned.push(component.as_os_str()),
        }
    }
    cleaned
}

/// 百分号解码；非法转义（如 `%zz`、`%4`）返回 `None`。
fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = hex_value(*bytes.get(index + 1)?)?;
        let low = hex_value(*bytes.get(index + 2)?)?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl OpenTarget {
    /// 交给 `App::open_url` 的字符串形式。
    #[must_use]
    pub fn url(&self) -> String {
        match self {
            Self::Url(url) => url.clone(),
            Self::File(path) => format!("file://{}", encoded_path(path)),
        }
    }
}

/// 按 RFC 8089 对本地路径做最小必要的百分号编码，保留路径分隔符。
fn encoded_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(char::from(byte));
            }
            _ => {
                // 写入 String 不会失败；忽略结果以满足 clippy 的 format_push_string。
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_and_mail_links_are_passed_through() {
        for uri in [
            "https://example.dev/a?b=1#c",
            "http://x.dev",
            "mailto:a@b.dev",
        ] {
            assert_eq!(
                resolve_target(uri),
                Ok(OpenTarget::Url(uri.to_string())),
                "{uri}"
            );
        }
    }

    #[test]
    fn unknown_schemes_are_rejected_instead_of_delegating_to_the_system() {
        assert_eq!(
            resolve_target("javascript:alert(1)"),
            Err(OpenError::UnsupportedScheme("javascript".to_string()))
        );
        assert_eq!(
            resolve_target("vscode://file/etc/hosts"),
            Err(OpenError::UnsupportedScheme("vscode".to_string()))
        );
        assert_eq!(
            resolve_target("relative/path"),
            Err(OpenError::UnsupportedScheme("relative/path".to_string()))
        );
    }

    #[test]
    fn file_links_decode_percent_escapes_and_drop_editor_fragments() {
        assert_eq!(
            resolve_target("file:///tmp/a%20b.txt#L42"),
            Ok(OpenTarget::File(PathBuf::from("/tmp/a b.txt")))
        );
        assert_eq!(
            resolve_target("file:///Users/x/%E6%97%A5%E5%BF%97.md"),
            Ok(OpenTarget::File(PathBuf::from("/Users/x/日志.md")))
        );
        assert_eq!(
            resolve_target("file:///tmp/../etc/hosts"),
            Ok(OpenTarget::File(PathBuf::from("/etc/hosts")))
        );
    }

    #[test]
    fn malformed_or_relative_file_links_are_rejected() {
        assert_eq!(
            resolve_target("file:///tmp/%zz"),
            Err(OpenError::InvalidPath("/tmp/%zz".to_string()))
        );
        assert_eq!(
            resolve_target("file:relative.md"),
            Err(OpenError::InvalidPath("relative.md".to_string()))
        );
    }

    #[test]
    fn empty_and_oversized_links_are_rejected() {
        assert_eq!(resolve_target("   "), Err(OpenError::Empty));
        let long = format!("https://x.dev/{}", "a".repeat(MAX_URI_BYTES));
        assert_eq!(resolve_target(&long), Err(OpenError::Empty));
    }

    #[test]
    fn relative_paths_resolve_against_the_project_directory() {
        let base = Path::new("/Users/me/project");
        assert_eq!(
            resolve_link(LinkKind::Path, "crates/a.rs", base),
            Ok(OpenTarget::File(PathBuf::from(
                "/Users/me/project/crates/a.rs"
            )))
        );
        assert_eq!(
            resolve_link(LinkKind::Path, "/tmp/a.rs", base),
            Ok(OpenTarget::File(PathBuf::from("/tmp/a.rs")))
        );
        assert_eq!(
            resolve_link(LinkKind::Path, "../other/a.rs", base),
            Ok(OpenTarget::File(PathBuf::from("/Users/me/other/a.rs")))
        );
    }

    #[test]
    fn url_and_uri_link_kinds_keep_the_scheme_whitelist() {
        let base = Path::new("/tmp");
        assert_eq!(
            resolve_link(LinkKind::Url, "https://example.dev", base),
            Ok(OpenTarget::Url("https://example.dev".to_string()))
        );
        assert_eq!(
            resolve_link(LinkKind::Uri, "file:///tmp/a.rs", base),
            Ok(OpenTarget::File(PathBuf::from("/tmp/a.rs")))
        );
        assert!(matches!(
            resolve_link(LinkKind::Uri, "javascript:alert(1)", base),
            Err(OpenError::UnsupportedScheme(_))
        ));
    }

    #[test]
    fn open_target_round_trips_through_url_encoding() {
        let target = OpenTarget::File(PathBuf::from("/tmp/a b/c.md"));
        assert_eq!(target.url(), "file:///tmp/a%20b/c.md");
        assert_eq!(
            resolve_target(&target.url()),
            Ok(OpenTarget::File(PathBuf::from("/tmp/a b/c.md")))
        );
    }
}
