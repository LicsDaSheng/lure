//! 映射上游 `tests/agent/test_memory_store.py` 的核心场景。

use lure_core::memory::MemoryStore;
use serde_json::Value;
use tempfile::tempdir;

fn store() -> (tempfile::TempDir, MemoryStore) {
    let dir = tempdir().unwrap();
    let store = MemoryStore::new(dir.path()).unwrap();
    (dir, store)
}

#[test]
fn read_memory_empty_when_missing_then_round_trips() {
    let (_dir, store) = store();
    assert_eq!(store.read_memory(), "");
    store.write_memory("hello");
    assert_eq!(store.read_memory(), "hello");
}

#[test]
fn soul_and_user_round_trip() {
    let (_dir, store) = store();
    assert_eq!(store.read_soul(), "");
    store.write_soul("soul content");
    assert_eq!(store.read_soul(), "soul content");

    assert_eq!(store.read_user(), "");
    store.write_user("user content");
    assert_eq!(store.read_user(), "user content");
}

#[test]
fn memory_context_formats_only_when_present() {
    let (_dir, store) = store();
    assert_eq!(store.get_memory_context(), "");
    store.write_memory("important fact");
    let ctx = store.get_memory_context();
    assert!(ctx.contains("Long-term Memory"));
    assert!(ctx.contains("important fact"));
}

#[test]
fn append_history_returns_incrementing_cursor() {
    let (_dir, store) = store();
    assert_eq!(store.append_history("event 1", None).unwrap(), 1);
    assert_eq!(store.append_history("event 2", None).unwrap(), 2);
    assert_eq!(store.append_history("event 3", None).unwrap(), 3);
}

#[test]
fn append_history_persists_cursor_and_session_key() {
    let (_dir, store) = store();
    store
        .append_history("event 1", Some("telegram:chat-1"))
        .unwrap();
    let content = MemoryStore::read_file(store.history_file());
    let data: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(data["cursor"], 1);
    assert_eq!(data["session_key"], "telegram:chat-1");
}

#[test]
fn append_history_strips_thinking_content() {
    let (_dir, store) = store();
    store
        .append_history("<think>reasoning</think>final answer", None)
        .unwrap();
    let content = MemoryStore::read_file(store.history_file());
    let data: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(data["content"], "final answer");
}

#[test]
fn append_history_drops_pure_leak_content() {
    let (_dir, store) = store();
    store
        .append_history("<think>nothing user-facing</think>", None)
        .unwrap();
    let content = MemoryStore::read_file(store.history_file());
    let data: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(data["content"], "");

    store.append_history("<channel|>", None).unwrap();
    let content = MemoryStore::read_file(store.history_file());
    let last: Value = serde_json::from_str(content.lines().last().unwrap()).unwrap();
    assert_eq!(last["content"], "");
}

#[test]
fn read_unprocessed_history_filters_by_cursor() {
    let (_dir, store) = store();
    store.append_history("event 1", None).unwrap();
    store.append_history("event 2", None).unwrap();
    store.append_history("event 3", None).unwrap();

    let since_one = store.read_unprocessed_history(1);
    assert_eq!(since_one.len(), 2);
    assert_eq!(since_one[0].cursor, 2);

    assert_eq!(store.read_unprocessed_history(0).len(), 3);
}

#[test]
fn prompt_history_filters_to_current_session() {
    let (_dir, store) = store();
    store.append_history("legacy entry", None).unwrap();
    store
        .append_history("telegram entry", Some("telegram:chat-1"))
        .unwrap();
    store
        .append_history("slack entry", Some("slack:chat-2"))
        .unwrap();

    let entries = store.read_recent_history_for_prompt(0, Some("telegram:chat-1"));
    let contents: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
    assert_eq!(contents, vec!["telegram entry"]);

    // 未过滤时仍可见全部三条。
    assert_eq!(store.read_unprocessed_history(0).len(), 3);
}

#[test]
fn cursor_persists_across_store_reopen() {
    let dir = tempdir().unwrap();
    {
        let store = MemoryStore::new(dir.path()).unwrap();
        store.append_history("event 1", None).unwrap();
        store.append_history("event 2", None).unwrap();
    }
    let reopened = MemoryStore::new(dir.path()).unwrap();
    assert_eq!(reopened.append_history("event 3", None).unwrap(), 3);
}
