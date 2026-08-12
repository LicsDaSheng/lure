//! 映射上游 `tests/agent/test_memory_store.py` 的核心场景。

use lure_core::memory::{MemoryStore, HISTORY_ENTRY_HARD_CAP};
use serde_json::Value;
use tempfile::tempdir;

fn store() -> (tempfile::TempDir, MemoryStore) {
    let dir = tempdir().unwrap();
    let store = MemoryStore::new(dir.path()).unwrap();
    (dir, store)
}

#[tokio::test]
async fn read_memory_empty_when_missing_then_round_trips() {
    let (_dir, store) = store();
    assert_eq!(store.read_memory(), "");
    store.write_memory("hello");
    assert_eq!(store.read_memory(), "hello");
}

#[tokio::test]
async fn soul_and_user_round_trip() {
    let (_dir, store) = store();
    assert_eq!(store.read_soul(), "");
    store.write_soul("soul content");
    assert_eq!(store.read_soul(), "soul content");

    assert_eq!(store.read_user(), "");
    store.write_user("user content");
    assert_eq!(store.read_user(), "user content");
}

#[tokio::test]
async fn memory_context_formats_only_when_present() {
    let (_dir, store) = store();
    assert_eq!(store.get_memory_context(), "");
    store.write_memory("important fact");
    let ctx = store.get_memory_context();
    assert!(ctx.contains("Long-term Memory"));
    assert!(ctx.contains("important fact"));
}

#[tokio::test]
async fn append_history_returns_incrementing_cursor() {
    let (_dir, store) = store();
    assert_eq!(store.append_history("event 1", None).unwrap(), 1);
    assert_eq!(store.append_history("event 2", None).unwrap(), 2);
    assert_eq!(store.append_history("event 3", None).unwrap(), 3);
}

#[tokio::test]
async fn append_history_persists_cursor_and_session_key() {
    let (_dir, store) = store();
    store
        .append_history("event 1", Some("telegram:chat-1"))
        .unwrap();
    let content = MemoryStore::read_file(store.history_file());
    let data: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(data["cursor"], 1);
    assert_eq!(data["session_key"], "telegram:chat-1");
}

#[tokio::test]
async fn append_history_strips_thinking_content() {
    let (_dir, store) = store();
    store
        .append_history("<think>reasoning</think>final answer", None)
        .unwrap();
    let content = MemoryStore::read_file(store.history_file());
    let data: Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(data["content"], "final answer");
}

#[tokio::test]
async fn append_history_drops_pure_leak_content() {
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

#[tokio::test]
async fn read_unprocessed_history_filters_by_cursor() {
    let (_dir, store) = store();
    store.append_history("event 1", None).unwrap();
    store.append_history("event 2", None).unwrap();
    store.append_history("event 3", None).unwrap();

    let since_one = store.read_unprocessed_history(1);
    assert_eq!(since_one.len(), 2);
    assert_eq!(since_one[0].cursor, 2);

    assert_eq!(store.read_unprocessed_history(0).len(), 3);
}

#[tokio::test]
async fn compact_history_drops_oldest_beyond_cap() {
    // 对齐上游 `test_compact_history_drops_oldest`：max_history_entries=2，追加 5 条后
    // compact 仅保留最新 2 条。
    let dir = tempdir().unwrap();
    let store = MemoryStore::new(dir.path())
        .unwrap()
        .with_max_history_entries(2);
    for i in 1..=5 {
        store.append_history(&format!("event {i}"), None).unwrap();
    }
    store.compact_history().unwrap();

    let entries = store.read_unprocessed_history(0);
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[0].cursor, 4 | 5), "应保留最新条目");
    // 保留最新内容。
    assert_eq!(entries[1].content, "event 5");
}

#[tokio::test]
async fn compact_history_noop_without_cap() {
    let (_dir, store) = store();
    for i in 1..=5 {
        store.append_history(&format!("event {i}"), None).unwrap();
    }
    store.compact_history().unwrap();
    assert_eq!(store.read_unprocessed_history(0).len(), 5, "无上限时不压缩");
}

#[tokio::test]
async fn compact_history_preserves_session_key() {
    let dir = tempdir().unwrap();
    let store = MemoryStore::new(dir.path())
        .unwrap()
        .with_max_history_entries(1);
    store.append_history("older", Some("api:a")).unwrap();
    store.append_history("newest", Some("api:b")).unwrap();
    store.compact_history().unwrap();

    let entries = store.read_unprocessed_history(0);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "newest");
    assert_eq!(entries[0].session_key.as_deref(), Some("api:b"));
}

#[tokio::test]
async fn oversized_entry_is_truncated_with_marker() {
    // 对齐上游 `TestAppendHistoryHardCap`：超硬上限条目被截断并带 `... (truncated)` 标记。
    let (_dir, store) = store();
    let huge = "x".repeat(HISTORY_ENTRY_HARD_CAP + 10_000);
    store.append_history(&huge, None).unwrap();
    let entry = &store.read_unprocessed_history(0)[0];
    assert!(entry.content.chars().count() <= HISTORY_ENTRY_HARD_CAP + 50);
    assert!(entry.content.contains("truncated"), "应含截断标记");
}

#[tokio::test]
async fn normal_sized_entry_unaffected_by_cap() {
    let (_dir, store) = store();
    store.append_history("normal short entry", None).unwrap();
    let entry = &store.read_unprocessed_history(0)[0];
    assert_eq!(entry.content, "normal short entry");
}

#[tokio::test]
async fn prompt_history_filters_to_current_session() {
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

#[tokio::test]
async fn cursor_persists_across_store_reopen() {
    let dir = tempdir().unwrap();
    {
        let store = MemoryStore::new(dir.path()).unwrap();
        store.append_history("event 1", None).unwrap();
        store.append_history("event 2", None).unwrap();
    }
    let reopened = MemoryStore::new(dir.path()).unwrap();
    assert_eq!(reopened.append_history("event 3", None).unwrap(), 3);
}

#[tokio::test]
async fn migrates_legacy_history_md_preserving_partial_entries() {
    let dir = tempdir().unwrap();
    let memory_dir = dir.path().join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    let legacy_file = memory_dir.join("HISTORY.md");
    let legacy_content = concat!(
        "[2026-04-01 10:00] User prefers dark mode.\n\n",
        "[2026-04-01 10:05] [RAW] 2 messages\n",
        "[2026-04-01 10:04] USER: hello\n",
        "[2026-04-01 10:04] ASSISTANT: hi\n\n",
        "Legacy chunk without timestamp.\n",
        "Keep whatever content we can recover.\n",
    );
    std::fs::write(&legacy_file, legacy_content).unwrap();

    let store = MemoryStore::new(dir.path()).unwrap();
    let entries = store.read_unprocessed_history(0);

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].cursor, 1);
    assert_eq!(entries[0].timestamp, "2026-04-01 10:00");
    assert_eq!(entries[0].content, "User prefers dark mode.");
    assert_eq!(entries[1].timestamp, "2026-04-01 10:05");
    assert!(entries[1].content.starts_with("[RAW] 2 messages"));
    assert!(entries[1].content.contains("USER: hello"));
    assert!(entries[2]
        .content
        .starts_with("Legacy chunk without timestamp."));
    assert_eq!(
        MemoryStore::read_file(&memory_dir.join(".cursor")).trim(),
        "3"
    );
    assert_eq!(
        MemoryStore::read_file(&memory_dir.join(".dream_cursor")).trim(),
        "3"
    );
    assert!(!legacy_file.exists());
    assert_eq!(
        MemoryStore::read_file(&memory_dir.join("HISTORY.md.bak")),
        legacy_content
    );
}

#[tokio::test]
async fn migrates_consecutive_legacy_entries_without_blank_lines() {
    let dir = tempdir().unwrap();
    let memory_dir = dir.path().join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("HISTORY.md"),
        concat!(
            "[2026-04-01 10:00] First event.\n",
            "[2026-04-01 10:01] Second event.\n",
            "[2026-04-01 10:02] Third event.\n",
        ),
    )
    .unwrap();

    let store = MemoryStore::new(dir.path()).unwrap();
    let contents: Vec<String> = store
        .read_unprocessed_history(0)
        .into_iter()
        .map(|entry| entry.content)
        .collect();

    assert_eq!(
        contents,
        vec!["First event.", "Second event.", "Third event."]
    );
}

#[tokio::test]
async fn existing_nonempty_history_jsonl_skips_legacy_migration() {
    let dir = tempdir().unwrap();
    let memory_dir = dir.path().join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("history.jsonl"),
        r#"{"cursor":7,"timestamp":"2026-04-01 12:00","content":"existing"}"#,
    )
    .unwrap();
    std::fs::write(memory_dir.join("HISTORY.md"), "[2026-04-01 10:00] legacy\n").unwrap();

    let store = MemoryStore::new(dir.path()).unwrap();
    let entries = store.read_unprocessed_history(0);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].cursor, 7);
    assert_eq!(entries[0].content, "existing");
    assert!(memory_dir.join("HISTORY.md").exists());
    assert!(!memory_dir.join("HISTORY.md.bak").exists());
}

#[tokio::test]
async fn empty_history_jsonl_still_allows_legacy_migration() {
    let dir = tempdir().unwrap();
    let memory_dir = dir.path().join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("history.jsonl"), "").unwrap();
    std::fs::write(memory_dir.join("HISTORY.md"), "[2026-04-01 10:00] legacy\n").unwrap();

    let store = MemoryStore::new(dir.path()).unwrap();
    let entries = store.read_unprocessed_history(0);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].cursor, 1);
    assert_eq!(entries[0].timestamp, "2026-04-01 10:00");
    assert_eq!(entries[0].content, "legacy");
    assert!(!memory_dir.join("HISTORY.md").exists());
    assert!(memory_dir.join("HISTORY.md.bak").exists());
}

#[tokio::test]
async fn migrates_legacy_history_with_invalid_utf8_bytes() {
    let dir = tempdir().unwrap();
    let memory_dir = dir.path().join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    std::fs::write(
        memory_dir.join("HISTORY.md"),
        b"[2026-04-01 10:00] Broken \xff data still needs migration.\n\n",
    )
    .unwrap();

    let store = MemoryStore::new(dir.path()).unwrap();
    let entries = store.read_unprocessed_history(0);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].timestamp, "2026-04-01 10:00");
    assert!(entries[0].content.contains("Broken"));
    assert!(entries[0].content.contains("migration."));
}
