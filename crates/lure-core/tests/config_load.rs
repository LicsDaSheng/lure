//! 映射上游 `tests/config/test_config_load_errors.py` 的加载路径场景。
//!
//! 暂未映射：
//! - invalid schema fails fast（上游用 `tools.exec.timeout=-1`）：tools 字段尚未建模。
//! - ApiConfig wildcard host 需 api_key：gateway/api 配置留待对应 phase。
//!
//! 详见 upstream-test-ledger。

use lure_core::config::{load_config, ConfigError, DEFAULT_MODEL};
use std::fs;
use tempfile::tempdir;

#[test]
fn missing_file_uses_defaults() {
    let dir = tempdir().unwrap();
    let config = load_config(&dir.path().join("missing.json")).unwrap();

    assert!(!config.agents.defaults.model.is_empty());
    assert_eq!(config.agents.defaults.model, DEFAULT_MODEL);
}

#[test]
fn invalid_json_fails_fast() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs::write(&path, "{broken json").unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }));
    assert!(err.to_string().contains("加载配置失败"));
}

#[test]
fn type_mismatch_fails_fast() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    // maxTokens 期望无符号整数，负数应作为解析错误快速失败。
    fs::write(&path, r#"{"agents":{"defaults":{"maxTokens":-1}}}"#).unwrap();

    let err = load_config(&path).unwrap_err();
    assert!(matches!(err, ConfigError::Parse { .. }));
}
