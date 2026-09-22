use std::path::{Path, PathBuf};

/// Pi 默认的配置目录名。
pub const DEFAULT_CONFIG_DIR: &str = ".pi";
/// Pi 用于覆盖 agent 目录的环境变量名。
pub const AGENT_DIR_ENV: &str = "PI_CODING_AGENT_DIR";

/// 解析 Pi 的 agent 目录：优先环境变量，其次 `home/.pi/agent`。
#[must_use]
pub fn agent_directory(home: &Path) -> PathBuf {
    match std::env::var_os(AGENT_DIR_ENV) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => home.join(DEFAULT_CONFIG_DIR).join("agent"),
    }
}

/// 计算某个工作目录对应的 Pi 会话目录。
///
/// 编码规则与 Pi 保持一致：去掉开头的一个路径分隔符，再把 `/`、`\`、`:` 全部替换为 `-`，
/// 最后用 `--` 包裹。
#[must_use]
pub fn session_directory(cwd: &Path, agent_dir: &Path) -> PathBuf {
    agent_dir.join("sessions").join(encoded_cwd(cwd))
}

fn encoded_cwd(cwd: &Path) -> String {
    let raw = cwd.to_string_lossy();
    let trimmed = raw
        .strip_prefix('/')
        .or_else(|| raw.strip_prefix('\\'))
        .unwrap_or(&raw);
    let mut encoded = String::with_capacity(trimmed.len() + 4);
    encoded.push_str("--");
    for character in trimmed.chars() {
        if matches!(character, '/' | '\\' | ':') {
            encoded.push('-');
        } else {
            encoded.push(character);
        }
    }
    encoded.push_str("--");
    encoded
}

/// 规范化 cwd 以便比较来自不同来源的同一路径。
///
/// 只做与 Pi `resolvePath` 等价的语法级处理：去掉结尾分隔符，忽略空字符串。
#[must_use]
pub(crate) fn normalized_cwd(cwd: &str) -> Option<String> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_trailing = trimmed.trim_end_matches(['/', '\\']);
    if without_trailing.is_empty() {
        return Some("/".to_owned());
    }
    Some(without_trailing.to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{normalized_cwd, session_directory};

    #[test]
    fn encodes_the_working_directory_like_pi() {
        assert_eq!(
            session_directory(
                Path::new("/Users/me/lure"),
                Path::new("/Users/me/.pi/agent")
            ),
            PathBuf::from("/Users/me/.pi/agent/sessions/--Users-me-lure--")
        );
    }

    #[test]
    fn encodes_colons_and_backslashes_and_the_root_directory() {
        assert_eq!(
            session_directory(Path::new("/tmp/a:b/c"), Path::new("/agent")),
            PathBuf::from("/agent/sessions/--tmp-a-b-c--")
        );
        assert_eq!(
            session_directory(Path::new("/"), Path::new("/agent")),
            PathBuf::from("/agent/sessions/----")
        );
    }

    #[test]
    fn compares_working_directories_without_trailing_separators() {
        assert_eq!(normalized_cwd("/tmp/lure/"), Some("/tmp/lure".to_owned()));
        assert_eq!(normalized_cwd("   "), None);
    }
}
