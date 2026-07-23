//! 配置与 workspace 路径解析。
//!
//! 以上游 `nanobot/config/paths.py` 与 `loader.py` 为事实来源：
//! - 默认 config 路径为 `~/.nanobot/config.json`。
//! - 默认 workspace 为 `~/.nanobot/workspace`。
//! - 自定义 workspace 支持 `~` 展开。
//!
//! 本模块只做纯路径解析，不创建目录（上游 `ensure_dir` 的副作用留待调用点按需处理）。

use std::path::PathBuf;

/// 返回用户主目录。
///
/// 无法解析主目录属于启动期不可恢复错误，直接 panic，避免让上层携带无意义的
/// 半初始化状态继续运行。
pub fn home_dir() -> PathBuf {
    dirs::home_dir().expect("无法解析用户主目录（HOME 未设置）")
}

/// 默认 config 文件路径：`~/.nanobot/config.json`。
pub fn default_config_path() -> PathBuf {
    home_dir().join(".nanobot").join("config.json")
}

/// 默认 workspace 路径：`~/.nanobot/workspace`。
pub fn default_workspace() -> PathBuf {
    home_dir().join(".nanobot").join("workspace")
}

/// 展开路径中的 `~` 前缀为用户主目录。
pub fn expand_user(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// 解析 workspace 路径：`None` 回落默认，`Some` 走 `~` 展开。
pub fn resolve_workspace(workspace: Option<&str>) -> PathBuf {
    match workspace {
        Some(w) => expand_user(w),
        None => default_workspace(),
    }
}

/// 判断给定 workspace 是否解析到默认 workspace。
pub fn is_default_workspace(workspace: Option<&str>) -> bool {
    resolve_workspace(workspace) == default_workspace()
}
