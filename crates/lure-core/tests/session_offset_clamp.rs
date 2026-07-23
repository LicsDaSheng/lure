//! 映射上游 `tests/session/test_consolidated_offset_clamp.py`。
//!
//! 越界或类型损坏的 `last_consolidated` 应重置为 0，且不丢弃消息。

use lure_core::session::{Session, SessionManager};
use serde_json::{json, Value};
use tempfile::tempdir;

fn messages(count: usize) -> Vec<Value> {
    (0..count)
        .map(|i| json!({"role": "user", "content": format!("msg{i}")}))
        .collect()
}

fn session(count: usize, offset: Value) -> Session {
    Session::with_messages("chan:chat", messages(count), &offset)
}

#[test]
fn out_of_range_offset_is_reset() {
    assert_eq!(session(10, json!(999)).last_consolidated(), 0);
    assert_eq!(session(3, json!(-5)).last_consolidated(), 0);
}

#[test]
fn non_integer_offset_is_reset() {
    for offset in [json!("999"), json!(null), json!(0.5), json!(true)] {
        assert_eq!(session(3, offset).last_consolidated(), 0);
    }
}

#[test]
fn valid_offset_is_preserved() {
    let session = session(10, json!(4));
    assert_eq!(session.last_consolidated(), 4);
    assert_eq!(session.get_history(0).len(), 6);
}

#[test]
fn loaded_corrupt_offset_keeps_messages() {
    for offset in [json!("999"), json!(null), json!(0.5), json!(true)] {
        let dir = tempdir().unwrap();
        let mut manager = SessionManager::new(dir.path()).unwrap();
        let path = manager.session_path("chan:chat");
        let message = json!({"role": "user", "content": "survived"});
        let meta = json!({
            "_type": "metadata",
            "key": "chan:chat",
            "metadata": {},
            "last_consolidated": offset,
        });
        std::fs::write(&path, format!("{meta}\n{message}\n")).unwrap();

        let session = manager.get_or_create("chan:chat").unwrap();
        assert_eq!(session.messages, vec![message.clone()]);
        assert_eq!(session.last_consolidated(), 0);
        assert_eq!(session.get_history(10), vec![message]);
    }
}
