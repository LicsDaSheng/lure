//! camelCase / snake_case 读取兼容与默认值回落。
//!
//! 对齐上游 `config_base.Base`（`alias_generator=to_camel`, `populate_by_name=True`）。

use lure_core::config::{load_config, Config};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn write_and_load(body: &str) -> (TempDir, Config) {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("config.json");
    fs::write(&path, body).unwrap();
    let config = load_config(&path).unwrap();
    (dir, config)
}

#[tokio::test]
async fn accepts_camel_case_keys() {
    let (_dir, config) = write_and_load(r#"{"agents":{"defaults":{"maxTokens":4096}}}"#);
    assert_eq!(config.agents.defaults.max_tokens, 4096);
}

#[tokio::test]
async fn accepts_snake_case_keys() {
    let (_dir, config) = write_and_load(r#"{"agents":{"defaults":{"max_tokens":4096}}}"#);
    assert_eq!(config.agents.defaults.max_tokens, 4096);
}

#[tokio::test]
async fn partial_config_fills_defaults() {
    let (_dir, config) = write_and_load(r#"{"agents":{"defaults":{"model":"x/y"}}}"#);
    assert_eq!(config.agents.defaults.model, "x/y");
    assert_eq!(config.agents.defaults.provider, "auto");
    assert_eq!(config.agents.defaults.max_tokens, 8192);
}

#[tokio::test]
async fn unknown_keys_are_ignored() {
    let (_dir, config) = write_and_load(r#"{"agents":{"defaults":{"model":"x/y"}},"unknown":1}"#);
    assert_eq!(config.agents.defaults.model, "x/y");
}
