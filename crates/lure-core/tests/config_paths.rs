//! 映射上游 `tests/config/test_config_paths.py` 的 workspace 路径场景。
//!
//! 说明：上游同文件里的 `get_data_dir`/`get_cron_dir` 等运行时子目录依赖
//! 全局 config path 注入，尚未在 Phase 1 建模，留待引入运行时目录时补齐。

use lure_core::config::{
    default_config_path, default_workspace, home_dir, is_default_workspace, resolve_workspace,
};

#[test]
fn config_path_defaults_to_nanobot_home() {
    assert_eq!(
        default_config_path(),
        home_dir().join(".nanobot").join("config.json")
    );
}

#[test]
fn workspace_defaults_to_nanobot_home() {
    assert_eq!(
        resolve_workspace(None),
        home_dir().join(".nanobot").join("workspace")
    );
}

#[test]
fn custom_workspace_expands_tilde() {
    assert_eq!(
        resolve_workspace(Some("~/custom-workspace")),
        home_dir().join("custom-workspace")
    );
}

#[test]
fn is_default_workspace_distinguishes_default_and_custom() {
    assert!(is_default_workspace(None));

    let default = default_workspace();
    assert!(is_default_workspace(Some(default.to_str().unwrap())));

    assert!(!is_default_workspace(Some("~/custom-workspace")));
}
