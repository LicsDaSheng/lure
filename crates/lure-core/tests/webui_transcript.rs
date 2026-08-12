//! WebUI transcript 存储：JSONL 格式持久化完整 user/assistant 消息对，
//! 为 webui-thread GET 提供消息视图。

use lure_core::webui::transcript::TranscripStore;
use serde_json::json;
use tempfile::TempDir;

fn store(dir: &TempDir) -> TranscripStore {
    TranscripStore::new(dir.path()).unwrap()
}

#[tokio::test]
async fn write_turn_and_read_thread_returns_ui_messages() {
    let dir = TempDir::new().unwrap();
    let mut store = store(&dir);

    store
        .append_turn("websocket:c1", "你好", "echo: 你好")
        .unwrap();
    let payload = store.read_thread("websocket:c1").unwrap().unwrap();
    assert_eq!(payload["schemaVersion"], 1);
    let msgs = payload["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0], json!({"role": "user", "content": "你好"}));
    assert_eq!(
        msgs[1],
        json!({"role": "assistant", "content": "echo: 你好"})
    );
    // 不泄露内部字段（timestamp）。
    assert!(msgs[0].get("timestamp").is_none());
}

#[tokio::test]
async fn read_thread_missing_returns_none() {
    let dir = TempDir::new().unwrap();
    let store = store(&dir);
    assert!(store.read_thread("websocket:none").unwrap().is_none());
}

#[tokio::test]
async fn multiple_turns_accumulate_in_order() {
    let dir = TempDir::new().unwrap();
    let mut store = store(&dir);

    store.append_turn("websocket:c1", "Q1", "A1").unwrap();
    store.append_turn("websocket:c1", "Q2", "A2").unwrap();
    let payload = store.read_thread("websocket:c1").unwrap().unwrap();
    let msgs = payload["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0]["content"], "Q1");
    assert_eq!(msgs[1]["content"], "A1");
    assert_eq!(msgs[2]["content"], "Q2");
    assert_eq!(msgs[3]["content"], "A2");
}

#[tokio::test]
async fn multiple_sessions_independent() {
    let dir = TempDir::new().unwrap();
    let mut store = store(&dir);

    store.append_turn("websocket:a", "pa", "ra").unwrap();
    store.append_turn("websocket:b", "pb", "rb").unwrap();

    let a = store.read_thread("websocket:a").unwrap().unwrap();
    let b = store.read_thread("websocket:b").unwrap().unwrap();
    assert_eq!(a["messages"].as_array().unwrap().len(), 2);
    assert_eq!(b["messages"].as_array().unwrap().len(), 2);
    assert_eq!(a["messages"][0]["content"], "pa");
    assert_eq!(b["messages"][0]["content"], "pb");
}

#[tokio::test]
async fn delete_clears_transcript_and_returns_false_afterwards() {
    let dir = TempDir::new().unwrap();
    let mut store = store(&dir);
    store.append_turn("websocket:d", "x", "y").unwrap();
    assert!(store.read_thread("websocket:d").unwrap().is_some());

    assert!(store.delete("websocket:d").unwrap());
    assert!(!store.delete("websocket:d").unwrap()); // 幂等 false
    assert!(store.read_thread("websocket:d").unwrap().is_none());
}
