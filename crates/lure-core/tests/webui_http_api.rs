//! WebUI HTTP 面（传输无关部分）：token 签发、bootstrap 与 sessions 载荷。
//!
//! 映射上游 `nanobot/webui/gateway_tokens.py` 与 `ws_http.py::_handle_bootstrap` /
//! `session_list_index.py::_public_row`。

use lure_core::session::SessionManager;
use lure_core::webui::http_api::{bootstrap_payload, sessions_payload};
use lure_core::webui::list_webui_sessions;
use lure_core::webui::tokens::TokenIssuer;
use serde_json::json;

#[test]
fn token_issuer_issues_and_checks_ws_and_api_tokens() {
    let mut issuer = TokenIssuer::new(3600, 16);
    let issued = issuer.issue();
    assert!(!issued.token.is_empty());
    assert!(!issued.api_token.is_empty());
    assert_ne!(issued.token, issued.api_token);

    assert!(issuer.check_ws_token(&issued.token));
    assert!(issuer.check_api_token(&issued.api_token));
    // 交叉无效：ws token 不能当 api token 用。
    assert!(!issuer.check_api_token(&issued.token));
    assert!(!issuer.check_ws_token(&issued.api_token));
    // 未知 token 拒绝。
    assert!(!issuer.check_ws_token("nope"));
    assert!(!issuer.check_api_token("nope"));
}

#[test]
fn token_issuer_enforces_capacity() {
    let mut issuer = TokenIssuer::new(3600, 2);
    issuer.issue();
    issuer.issue();
    assert!(issuer.try_issue().is_none(), "超出容量上限应拒绝签发");
}

#[test]
fn token_issuer_expires_tokens() {
    let mut issuer = TokenIssuer::new(0, 16);
    let issued = issuer.issue();
    // ttl=0：立即过期。
    assert!(!issuer.check_ws_token(&issued.token));
    assert!(!issuer.check_api_token(&issued.api_token));
}

#[test]
fn bootstrap_payload_matches_upstream_shape() {
    let mut issuer = TokenIssuer::new(3600, 16);
    let issued = issuer.issue();
    let payload = bootstrap_payload(
        &issued,
        "/ws",
        "ws://127.0.0.1:9000/ws",
        3600,
        Some("deepseek-v4-pro"),
    );
    assert_eq!(payload["token"], issued.token);
    assert_eq!(payload["api_token"], issued.api_token);
    assert_eq!(payload["ws_path"], "/ws");
    assert_eq!(payload["ws_url"], "ws://127.0.0.1:9000/ws");
    assert_eq!(payload["expires_in"], 3600);
    assert_eq!(payload["model_name"], "deepseek-v4-pro");
    // 无 model 时为 null（前端 modelName ?? null）。
    let payload = bootstrap_payload(&issued, "/ws", "ws://127.0.0.1:9000/ws", 3600, None);
    assert_eq!(payload["model_name"], serde_json::Value::Null);
}

#[test]
fn sessions_payload_matches_frontend_row_shape_and_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    {
        let session = manager.get_or_create("websocket:old").unwrap();
        session.add_message("user", "older question");
        session.add_message("assistant", "older answer");
    }
    manager.save("websocket:old", false).unwrap();
    // 保证 updated_at 有可见差异。
    std::thread::sleep(std::time::Duration::from_millis(1100));
    {
        let session = manager.get_or_create("websocket:new").unwrap();
        session.add_message("user", "latest question");
    }
    manager.save("websocket:new", false).unwrap();

    let mut reader = SessionManager::new(dir.path()).unwrap();
    let rows = list_webui_sessions(&mut reader);
    let payload = sessions_payload(&rows);

    let sessions = payload["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    // 按 updated_at 倒序（最新在前），对齐上游 list_webui_sessions。
    assert_eq!(sessions[0]["key"], "websocket:new");
    assert_eq!(sessions[1]["key"], "websocket:old");

    let row = &sessions[1];
    assert!(row["created_at"].as_str().unwrap().contains('T'));
    assert!(row["updated_at"].as_str().unwrap().contains('T'));
    assert_eq!(row["title"], "");
    assert!(!row["preview"].as_str().unwrap().is_empty());
    // 前端不消费的字段不外泄。
    assert!(row.get("message_count").is_none());

    // 空 workspace → 空数组。
    let empty = tempfile::tempdir().unwrap();
    let mut reader = SessionManager::new(empty.path()).unwrap();
    let rows = list_webui_sessions(&mut reader);
    assert_eq!(sessions_payload(&rows), json!({"sessions": []}));
}
