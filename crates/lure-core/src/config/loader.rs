//! 配置读写：load / save 与结构化错误。
//!
//! 行为对齐上游 `nanobot/config/loader.py`：
//! - `load_config`：文件不存在时返回默认配置；解析失败快速失败并给出路径上下文。
//! - `save_config`：camelCase、缩进 2、保留非 ASCII 字符；temp + rename 原子写，
//!   崩溃不会留下截断文件；unix 下保留既有文件权限位。
//!
//! 暂不复刻上游的 `_migrate_config`、env 变量插值和 SSRF whitelist 应用，
//! 这些依赖尚未建模的字段，留待对应 phase 补齐（见 upstream-test-ledger）。

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use crate::config::schema::Config;

/// 配置读写的结构化错误。
#[derive(Debug)]
pub enum ConfigError {
    /// 读取配置文件失败。
    Read { path: PathBuf, source: io::Error },
    /// 解析配置内容失败（JSON 语法或类型不匹配）。
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// 序列化配置失败。
    Serialize { source: serde_json::Error },
    /// 写入配置文件失败。
    Write { path: PathBuf, source: io::Error },
    /// 配置语义校验失败（如 preset 约束）。
    Validation { path: PathBuf, message: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read { path, source } => {
                write!(f, "读取配置失败 {}: {source}", path.display())
            }
            ConfigError::Parse { path, source } => {
                write!(f, "加载配置失败 {}: {source}", path.display())
            }
            ConfigError::Serialize { source } => write!(f, "序列化配置失败: {source}"),
            ConfigError::Write { path, source } => {
                write!(f, "写入配置失败 {}: {source}", path.display())
            }
            ConfigError::Validation { path, message } => {
                write!(f, "配置校验失败 {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Read { source, .. } | ConfigError::Write { source, .. } => Some(source),
            ConfigError::Parse { source, .. } | ConfigError::Serialize { source } => Some(source),
            ConfigError::Validation { .. } => None,
        }
    }
}

/// 从文件加载配置；文件不存在时返回默认配置。
pub fn load_config(path: &Path) -> Result<Config, ConfigError> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut raw: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    crate::config::migration::migrate_config(&mut raw);
    let config: Config = serde_json::from_value(raw).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    config
        .validate()
        .map_err(|message| ConfigError::Validation {
            path: path.to_path_buf(),
            message,
        })?;
    Ok(config)
}

/// 将配置以约定格式原子写入文件。
pub fn save_config(config: &Config, path: &Path) -> Result<(), ConfigError> {
    let json =
        serde_json::to_string_pretty(config).map_err(|source| ConfigError::Serialize { source })?;
    write_text_atomic(path, &json).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// temp + rename 原子写文本。
///
/// 先写临时文件并 fsync，再原子重命名覆盖目标；若目标已存在，unix 下继承其权限位。
/// 重命名前发生崩溃只会遗留临时文件，目标文件保持不变。
fn write_text_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
    }
    let dir = parent.unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.json");
    let tmp = dir.join(format!(".{file_name}.tmp.{}", process::id()));

    let write_result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        preserve_mode(path, &tmp)?;
        fs::rename(&tmp, path)
    })();

    if write_result.is_err() {
        // 清理半成品临时文件，避免污染数据目录。
        let _ = fs::remove_file(&tmp);
    }
    write_result
}

/// 在 unix 上把既有目标文件的权限位复制到临时文件。
#[cfg(unix)]
fn preserve_mode(existing: &Path, tmp: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::metadata(existing) {
        let mode = meta.permissions().mode();
        fs::set_permissions(tmp, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

/// 非 unix 平台不暴露 POSIX 权限位，跳过保留。
#[cfg(not(unix))]
fn preserve_mode(_existing: &Path, _tmp: &Path) -> io::Result<()> {
    Ok(())
}
