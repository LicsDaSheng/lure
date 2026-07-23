//! 存储 key 可逆性与 JSONL 持久化（含 fsync 与非 ASCII 保留）。
//!
//! 对齐上游存储文件名编码与 `save` 的原子写/durable 语义。

use lure_core::session::{SessionError, SessionManager};
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn storage_key_is_reversible_and_filename_safe() {
    for key in [
        "telegram:12345",
        "unified:default",
        "cli:test",
        "a:b:c",
        "群聊:42",
    ] {
        let stem = SessionManager::storage_key(key);
        assert!(!stem.contains(':'), "stem 不应含 ':'");
        assert!(!stem.contains('/'), "stem 不应含 '/'");
        assert!(!stem.contains('='), "stem 不应含 padding");
        assert_eq!(
            SessionManager::decode_storage_key(&stem).as_deref(),
            Some(key)
        );
    }
}

#[test]
fn jsonl_round_trips_with_fsync_and_unicode() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    {
        let session = manager.get_or_create("cli:test").unwrap();
        session.add_message("user", "你好");
    }
    manager.save("cli:test", true).unwrap();

    let text = std::fs::read_to_string(manager.session_path("cli:test")).unwrap();
    let first_line = text.lines().next().unwrap();
    let meta: Value = serde_json::from_str(first_line).unwrap();
    assert_eq!(meta["_type"], "metadata");
    assert_eq!(meta["key"], "cli:test");
    assert!(text.contains("你好"), "非 ASCII 字符应原样保留");

    let mut reloaded_manager = SessionManager::new(dir.path()).unwrap();
    let reloaded = reloaded_manager.get_or_create("cli:test").unwrap();
    assert_eq!(reloaded.messages.len(), 1);
    assert_eq!(reloaded.messages[0]["content"], "你好");
}

#[test]
fn save_requires_cached_session() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();

    let err = manager.save("missing:key", false).unwrap_err();
    assert!(matches!(err, SessionError::NotCached { .. }));
}

#[test]
fn temp_file_is_not_left_behind_after_save() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    manager
        .get_or_create("cli:test")
        .unwrap()
        .add_message("user", "hi");
    manager.save("cli:test", true).unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(manager.sessions_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "tmp"))
        .collect();
    assert!(leftovers.is_empty(), "不应遗留 .tmp 文件");
}

#[test]
fn list_stored_keys_repairs_legacy_lossy_stem_with_corrupt_line() {
    let dir = tempdir().unwrap();
    let manager = SessionManager::new(dir.path()).unwrap();
    let legacy_path = manager.sessions_dir().join("telegram_12345.jsonl");
    let metadata = json!({
        "_type": "metadata",
        "key": "telegram:12345",
        "created_at": "2025-01-01T00:00:00+00:00",
        "updated_at": "2025-01-01T00:00:00+00:00"
    });
    std::fs::write(
        &legacy_path,
        format!(
            "{}\n{{INVALID JSON LINE\n{}\n",
            serde_json::to_string(&metadata).unwrap(),
            serde_json::to_string(&json!({"role": "user", "content": "recoverable message"}))
                .unwrap()
        ),
    )
    .unwrap();

    let keys = manager.list_stored_keys();

    assert_eq!(keys, vec!["telegram:12345"]);
    assert!(
        !legacy_path.exists(),
        "legacy stem 应迁移到 canonical 文件名"
    );
    assert!(manager.session_path("telegram:12345").exists());
}

#[test]
fn get_or_create_migrates_legacy_lossy_stem_when_key_matches_metadata() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    let legacy_path = manager.sessions_dir().join("cli_direct.jsonl");
    let metadata = json!({
        "_type": "metadata",
        "key": "cli:direct",
        "created_at": "2025-01-01T00:00:00+00:00",
        "updated_at": "2025-01-01T00:00:00+00:00"
    });
    std::fs::write(
        &legacy_path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&metadata).unwrap(),
            serde_json::to_string(&json!({"role": "assistant", "content": "legacy answer"}))
                .unwrap()
        ),
    )
    .unwrap();

    let session = manager.get_or_create("cli:direct").unwrap();

    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0]["content"], "legacy answer");
    assert!(!legacy_path.exists());
    assert!(manager.session_path("cli:direct").exists());
}
