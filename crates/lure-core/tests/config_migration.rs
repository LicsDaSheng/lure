//! 映射上游 `tests/config/test_config_migration.py` 的 legacy 迁移变换。
//!
//! 在原始 JSON 层测试迁移；`tools` 尚未 typed 建模，故直接断言迁移后的 Value。

use lure_core::config::{load_config, migrate_config, DEFAULT_MODEL};
use serde_json::json;

#[tokio::test]
async fn drops_legacy_max_messages() {
    let mut data = json!({"agents": {"defaults": {"maxMessages": 25, "model": "x/y"}}});
    migrate_config(&mut data);
    let defaults = &data["agents"]["defaults"];
    assert!(defaults.get("maxMessages").is_none());
    assert_eq!(defaults["model"], "x/y");

    let mut snake = json!({"agents": {"defaults": {"max_messages": 25}}});
    migrate_config(&mut snake);
    assert!(snake["agents"]["defaults"].get("max_messages").is_none());
}

#[tokio::test]
async fn moves_exec_restrict_to_workspace() {
    let mut data = json!({"tools": {"exec": {"restrictToWorkspace": true, "timeout": 5}}});
    migrate_config(&mut data);
    assert_eq!(data["tools"]["restrictToWorkspace"], true);
    assert!(data["tools"]["exec"].get("restrictToWorkspace").is_none());
    // 其他 exec 字段保留。
    assert_eq!(data["tools"]["exec"]["timeout"], 5);
}

#[tokio::test]
async fn does_not_override_existing_restrict_to_workspace() {
    let mut data = json!({
        "tools": {"restrictToWorkspace": false, "exec": {"restrictToWorkspace": true}}
    });
    migrate_config(&mut data);
    // 顶层已存在，保留其值，不被 exec 覆盖。
    assert_eq!(data["tools"]["restrictToWorkspace"], false);
}

#[tokio::test]
async fn migrates_legacy_my_tool_keys() {
    let mut data = json!({"tools": {"myEnabled": false, "mySet": true}});
    migrate_config(&mut data);
    let tools = &data["tools"];
    assert!(tools.get("myEnabled").is_none());
    assert!(tools.get("mySet").is_none());
    assert_eq!(tools["my"], json!({"enable": false, "allowSet": true}));
}

#[tokio::test]
async fn new_my_tool_keys_take_precedence_over_legacy() {
    let mut data = json!({
        "tools": {"myEnabled": false, "mySet": false, "my": {"enable": true, "allowSet": true}}
    });
    migrate_config(&mut data);
    assert_eq!(
        data["tools"]["my"],
        json!({"enable": true, "allowSet": true})
    );
}

#[tokio::test]
async fn load_config_applies_migration_and_ignores_legacy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    // legacy maxMessages 应被迁移丢弃，typed config 正常加载。
    std::fs::write(&path, r#"{"agents":{"defaults":{"maxMessages":25}}}"#).unwrap();

    let config = load_config(&path).unwrap();
    assert_eq!(config.agents.defaults.model, DEFAULT_MODEL);
}
