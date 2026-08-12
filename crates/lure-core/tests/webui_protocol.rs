//! 映射上游 `tests/webui/test_session_list_index.py` 与 WebSocket 协议事件形状。

use lure_core::session::SessionManager;
use lure_core::webui::{
    delta_event, error_event, list_webui_sessions, message_event, parse_ws_inbound, status_event,
    thread_messages, webui_status,
};
use serde_json::json;
use tempfile::tempdir;

fn seed_session(manager: &mut SessionManager, key: &str, user: &str, assistant: &str) {
    {
        let session = manager.get_or_create(key).unwrap();
        session.add_message("user", user);
        session.add_message("assistant", assistant);
    }
    manager.save(key, false).unwrap();
}

#[tokio::test]
async fn session_list_reports_preview_and_counts() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    seed_session(
        &mut manager,
        "websocket:first",
        "first preview",
        "reply one",
    );
    seed_session(
        &mut manager,
        "websocket:second",
        "second preview",
        "reply two",
    );

    // 冷启动新 manager，从磁盘枚举。
    let mut reader = SessionManager::new(dir.path()).unwrap();
    let rows = list_webui_sessions(&mut reader);
    assert_eq!(rows.len(), 2);

    let previews: std::collections::BTreeSet<&str> =
        rows.iter().map(|r| r.preview.as_str()).collect();
    assert!(previews.contains("first preview"));
    assert!(previews.contains("second preview"));

    let first = rows.iter().find(|r| r.key == "websocket:first").unwrap();
    assert_eq!(first.message_count, 2);
    assert!(!first.updated_at.is_empty());
}

#[tokio::test]
async fn session_list_empty_workspace() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    assert!(list_webui_sessions(&mut manager).is_empty());
}

#[tokio::test]
async fn thread_messages_projects_role_and_content() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    seed_session(&mut manager, "websocket:t", "hello", "hi there");

    let session = manager.get_or_create("websocket:t").unwrap();
    let payload = thread_messages(session);
    assert_eq!(payload["key"], "websocket:t");
    assert_eq!(payload["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        payload["messages"][0],
        json!({"role": "user", "content": "hello"})
    );
    assert_eq!(
        payload["messages"][1],
        json!({"role": "assistant", "content": "hi there"})
    );
    // 内部字段（timestamp）被丢弃。
    assert!(payload["messages"][0].get("timestamp").is_none());
}

#[tokio::test]
async fn thread_messages_surface_reasoning_content() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    {
        let session = manager.get_or_create("websocket:r").unwrap();
        session.add_message("user", "2+2");
        let mut extra = serde_json::Map::new();
        extra.insert("reasoning_content".to_string(), json!("2 加 2 等于 4"));
        session.add_message_with("assistant", "答案是 4", extra);
    }
    manager.save("websocket:r", false).unwrap();

    let session = manager.get_or_create("websocket:r").unwrap();
    let payload = thread_messages(session);
    let assistant = &payload["messages"][1];
    assert_eq!(assistant["role"], "assistant");
    assert_eq!(assistant["content"], "答案是 4");
    assert_eq!(assistant["reasoning_content"], "2 加 2 等于 4");
    // user 消息无 reasoning_content。
    assert!(payload["messages"][0].get("reasoning_content").is_none());
}

#[tokio::test]
async fn status_reports_session_count_and_version() {
    let dir = tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    seed_session(&mut manager, "websocket:s", "hi", "yo");

    let reader = SessionManager::new(dir.path()).unwrap();
    let status = webui_status(&reader);
    assert_eq!(status["status"], "ok");
    assert_eq!(status["sessions"], 1);
    assert!(!status["version"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn ws_outbound_event_shapes() {
    assert_eq!(
        message_event("chat-1", "full reply"),
        json!({"event": "message", "chat_id": "chat-1", "text": "full reply"})
    );
    assert_eq!(
        delta_event("chat-1", "chunk"),
        json!({"event": "delta", "chat_id": "chat-1", "text": "chunk"})
    );
    assert_eq!(
        status_event("busy"),
        json!({"event": "status", "status": "busy"})
    );
    assert_eq!(
        error_event("invalid chat_id"),
        json!({"event": "error", "detail": "invalid chat_id"})
    );
}

#[tokio::test]
async fn ws_inbound_parses_and_validates() {
    let frame = json!({"type": "chat", "chat_id": "c1", "content": "hello"});
    let inbound = parse_ws_inbound(&frame, "websocket").unwrap();
    assert_eq!(inbound.channel, "websocket");
    assert_eq!(inbound.chat_id, "c1");
    assert_eq!(inbound.content, "hello");
    assert_eq!(inbound.session_key(), "websocket:c1");

    // 缺 chat_id。
    let err = parse_ws_inbound(&json!({"type": "chat", "content": "x"}), "websocket").unwrap_err();
    assert_eq!(err, "invalid chat_id");

    // 缺 content。
    let err = parse_ws_inbound(&json!({"type": "chat", "chat_id": "c1"}), "websocket").unwrap_err();
    assert_eq!(err, "missing content");
}
