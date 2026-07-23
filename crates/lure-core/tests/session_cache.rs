//! 映射上游 `tests/session/test_session_cache.py` 的有界与 LRU 语义，
//! 以及 `test_session_fsync.py::test_flush_all_data_survives_reload` 的持久化。
//!
//! 暂未映射：weak-overflow 身份保留（`keeps_identity_for_evicted_active_sessions`、
//! `flush_all_includes_live_sessions_outside_strong_cache`）——需 `Rc`/`Weak` 身份语义，
//! 留待后续（见 upstream-test-ledger）。

use lure_core::session::{SessionManager, SESSION_CACHE_MAX_SIZE};
use tempfile::tempdir;

#[test]
fn default_session_cache_is_bounded() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();

    for index in 0..=SESSION_CACHE_MAX_SIZE {
        manager.get_or_create(&format!("test:{index}")).unwrap();
    }

    assert_eq!(manager.cache_len(), SESSION_CACHE_MAX_SIZE);
}

#[test]
fn session_cache_refreshes_lru_order_on_access() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    manager.set_max_cached(2);

    manager.get_or_create("test:first").unwrap();
    manager.save("test:first", false).unwrap();
    manager.get_or_create("test:second").unwrap();
    manager.save("test:second", false).unwrap();

    manager.get_or_create("test:first").unwrap(); // refresh LRU
    manager.get_or_create("test:third").unwrap();
    manager.save("test:third", false).unwrap();

    assert_eq!(
        manager.cache_keys(),
        vec!["test:first".to_string(), "test:third".to_string()]
    );
}

#[test]
fn evicted_session_reloads_from_disk() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    manager.set_max_cached(1);

    manager
        .get_or_create("test:first")
        .unwrap()
        .add_message("user", "persist me");
    manager.save("test:first", false).unwrap();

    manager.get_or_create("test:second").unwrap();
    manager.save("test:second", false).unwrap();

    assert_eq!(manager.cache_len(), 1);
    let reloaded = manager.get_or_create("test:first").unwrap();
    assert_eq!(reloaded.messages[0]["content"], "persist me");
}

#[test]
fn flush_all_data_survives_reload() {
    let dir = tempdir().unwrap();
    {
        let mut manager = SessionManager::new(dir.path()).unwrap();
        {
            let session = manager.get_or_create("test:persist").unwrap();
            session.add_message("user", "remember this");
            session.add_message("assistant", "noted");
        }
        manager.save("test:persist", false).unwrap();
        assert_eq!(manager.flush_all(), 1);
    }

    let mut reloaded_manager = SessionManager::new(dir.path()).unwrap();
    let reloaded = reloaded_manager.get_or_create("test:persist").unwrap();
    let history = reloaded.get_history(100);

    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["content"], "remember this");
    assert_eq!(history[1]["content"], "noted");
}
