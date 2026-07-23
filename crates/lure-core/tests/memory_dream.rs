//! Dream consolidation：可替换 runner（不接真实 LLM）。
//!
//! 覆盖：dream 触发、结果写回 MEMORY.md、dream cursor 推进、无新历史时幂等。

use lure_core::memory::{DreamRunner, HistoryEntry, MemoryStore};
use tempfile::tempdir;

/// 把当前记忆与新历史内容拼接为新记忆的 fake runner。
struct AppendRunner;

impl DreamRunner for AppendRunner {
    fn consolidate(&self, current_memory: &str, entries: &[HistoryEntry]) -> String {
        let mut lines: Vec<String> = if current_memory.is_empty() {
            Vec::new()
        } else {
            vec![current_memory.to_string()]
        };
        lines.extend(entries.iter().map(|e| format!("- {}", e.content)));
        lines.join("\n")
    }
}

fn store() -> (tempfile::TempDir, MemoryStore) {
    let dir = tempdir().unwrap();
    let store = MemoryStore::new(dir.path()).unwrap();
    (dir, store)
}

#[test]
fn consolidate_writes_memory_and_advances_cursor() {
    let (_dir, store) = store();
    store.append_history("learned fact A", None).unwrap();
    store.append_history("learned fact B", None).unwrap();

    let outcome = store.consolidate(&AppendRunner).unwrap();
    assert_eq!(outcome.processed, 2);
    assert_eq!(outcome.new_cursor, 2);
    assert_eq!(store.get_last_dream_cursor(), 2);

    let memory = store.read_memory();
    assert!(memory.contains("learned fact A"));
    assert!(memory.contains("learned fact B"));
}

#[test]
fn consolidate_is_noop_without_new_history() {
    let (_dir, store) = store();
    store.append_history("fact", None).unwrap();
    store.consolidate(&AppendRunner).unwrap();
    let memory_after_first = store.read_memory();

    // 无新历史 → None，且记忆不变。
    assert!(store.consolidate(&AppendRunner).is_none());
    assert_eq!(store.read_memory(), memory_after_first);
}

#[test]
fn consolidate_only_processes_entries_after_dream_cursor() {
    let (_dir, store) = store();
    store.append_history("first", None).unwrap();
    let first = store.consolidate(&AppendRunner).unwrap();
    assert_eq!(first.new_cursor, 1);

    store.append_history("second", None).unwrap();
    store.append_history("third", None).unwrap();

    let second = store.consolidate(&AppendRunner).unwrap();
    assert_eq!(second.processed, 2, "只处理 dream cursor 之后的两条");
    assert_eq!(second.new_cursor, 3);

    let memory = store.read_memory();
    assert!(memory.contains("second"));
    assert!(memory.contains("third"));
    // 第一条只在首次整合出现一次，不被重复处理。
    assert_eq!(memory.matches("first").count(), 1);
}

#[test]
fn consolidate_empty_history_returns_none() {
    let (_dir, store) = store();
    assert!(store.consolidate(&AppendRunner).is_none());
}
