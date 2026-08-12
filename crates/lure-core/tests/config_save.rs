//! 映射上游 `tests/config/test_config_atomic_save.py` 的保存语义。
//!
//! 暂未映射：`test_save_config_preserves_existing_file_when_write_fails`
//! 依赖对 `Path.replace` 打桩模拟崩溃；Rust 端由 temp + rename 的原子写在设计上
//! 保证目标文件不被截断，未单独 mock `fs::rename`。详见 upstream-test-ledger。

use lure_core::config::{load_config, save_config, Config};
use tempfile::tempdir;

#[tokio::test]
async fn save_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");

    save_config(&Config::default(), &path).unwrap();
    let loaded = load_config(&path).unwrap();

    assert_eq!(loaded, Config::default());
    assert!(!loaded.agents.defaults.model.is_empty());
}

#[tokio::test]
async fn save_emits_camel_case_indented_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");

    save_config(&Config::default(), &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();

    assert!(text.contains("\"maxTokens\""), "应输出 camelCase 键");
    assert!(!text.contains("max_tokens"), "不应输出 snake_case 键");
    assert!(text.contains("\n  \""), "应使用 2 空格缩进");
}

#[tokio::test]
async fn save_creates_missing_parent_dirs() {
    let dir = tempdir().unwrap();
    let path = dir
        .path()
        .join("nested")
        .join("instance")
        .join("config.json");

    save_config(&Config::default(), &path).unwrap();

    assert!(path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn save_preserves_existing_file_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, "{}").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    save_config(&Config::default(), &path).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}
