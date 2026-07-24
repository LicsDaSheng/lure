//! Phase 9 纵向切片：真实 HTTP server 接线 `/v1/chat/completions` 与 `/health`。
//!
//! 用 `EchoProvider` 驱动的 `AgentLoop` 作为 runner，真实启动 server 在随机端口，
//! 用 `ureq` 发真实 HTTP 请求验证端到端契约。全部不触外网。
//!
//! 测试拓扑：server 持有的 `AgentLoop` 不要求 `Send`，故 server 留在主线程，
//! HTTP 客户端跑在子线程；主线程按请求数调用 `handle_next` 逐条应答。

use std::net::SocketAddr;
use std::thread;

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::api::{ChatServer, ServerConfig};
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;
use serde_json::{json, Value};
use tempfile::TempDir;

/// 用 echo provider 构建一个最小 `AgentLoop`。
fn build_loop(dir: &TempDir) -> AgentLoop {
    let sessions = SessionManager::new(dir.path()).unwrap();
    AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    )
}

/// 绑定随机端口的 server，返回 `(server, addr)`。
fn bind_server(dir: &TempDir, api_key: Option<&str>) -> (ChatServer<AgentLoop>, SocketAddr) {
    let config = ServerConfig {
        model: "echo".to_string(),
        api_key: api_key.map(str::to_string),
    };
    let server = ChatServer::bind("127.0.0.1:0", build_loop(dir), config).unwrap();
    let addr = server.local_addr().unwrap();
    (server, addr)
}

/// 在子线程发一个 POST /v1/chat/completions，返回 `(status, body)`。
fn post_chat(addr: SocketAddr, body: Value, auth: Option<&str>) -> (u16, String) {
    let url = format!("http://{addr}/v1/chat/completions");
    let mut req = ureq::post(&url).set("Content-Type", "application/json");
    if let Some(token) = auth {
        req = req.set("Authorization", token);
    }
    match req.send_string(&body.to_string()) {
        Ok(resp) => (resp.status(), resp.into_string().unwrap()),
        Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap()),
        Err(e) => panic!("传输错误: {e}"),
    }
}

#[test]
fn non_streaming_chat_completion_shape_and_content() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, None);

    let client = thread::spawn(move || {
        post_chat(
            addr,
            json!({"messages": [{"role": "user", "content": "hello"}]}),
            None,
        )
    });
    server.handle_next().unwrap();
    let (status, body) = client.join().unwrap();

    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["object"], "chat.completion");
    assert_eq!(v["model"], "echo");
    assert_eq!(v["choices"][0]["message"]["role"], "assistant");
    assert_eq!(v["choices"][0]["message"]["content"], "echo: hello");
    assert_eq!(v["choices"][0]["finish_reason"], "stop");
    assert!(v["id"].as_str().unwrap().starts_with("chatcmpl-"));
}

#[test]
fn streaming_emits_sse_chunk_then_finish_then_done() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, None);

    let client = thread::spawn(move || {
        let url = format!("http://{addr}/v1/chat/completions");
        let payload = json!({
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true,
        });
        let resp = ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_string(&payload.to_string())
            .unwrap();
        let content_type = resp.header("Content-Type").unwrap_or_default().to_string();
        (content_type, resp.into_string().unwrap())
    });
    server.handle_next().unwrap();
    let (content_type, body) = client.join().unwrap();

    assert!(
        content_type.starts_with("text/event-stream"),
        "content-type 应为 SSE，实际: {content_type}"
    );

    let frames: Vec<&str> = body
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(frames.len(), 3, "应为 内容 chunk → finish chunk → [DONE]");

    // 帧 0：内容 chunk，delta.content 为完整回复，finish_reason 为空。
    let f0: Value = serde_json::from_str(frames[0].strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(f0["object"], "chat.completion.chunk");
    assert_eq!(f0["choices"][0]["delta"]["content"], "echo: hello");
    assert!(f0["choices"][0]["finish_reason"].is_null());

    // 帧 1：finish chunk，finish_reason = stop，delta 为空对象。
    let f1: Value = serde_json::from_str(frames[1].strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(f1["choices"][0]["finish_reason"], "stop");
    assert_eq!(f1["choices"][0]["delta"], json!({}));

    // 帧 2：终止哨兵。
    assert_eq!(frames[2], "data: [DONE]");
}

#[test]
fn missing_user_message_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, None);

    let client = thread::spawn(move || post_chat(addr, json!({"messages": []}), None));
    server.handle_next().unwrap();
    let (status, body) = client.join().unwrap();

    assert_eq!(status, 400);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], 400);
    assert_eq!(v["error"]["type"], "invalid_request_error");
}

#[test]
fn model_mismatch_returns_400() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, None);

    let client = thread::spawn(move || {
        post_chat(
            addr,
            json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "hi"}],
            }),
            None,
        )
    });
    server.handle_next().unwrap();
    let (status, body) = client.join().unwrap();

    assert_eq!(status, 400);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], 400);
    assert!(v["error"]["message"].as_str().unwrap().contains("gpt-4"));
}

#[test]
fn missing_or_wrong_bearer_key_returns_401_then_valid_passes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, Some("secret"));

    let client = thread::spawn(move || {
        // 无 Authorization → 401。
        let missing = post_chat(
            addr,
            json!({"messages": [{"role": "user", "content": "hi"}]}),
            None,
        );
        // 正确 Bearer key → 200。
        let ok = post_chat(
            addr,
            json!({"messages": [{"role": "user", "content": "hi"}]}),
            Some("Bearer secret"),
        );
        (missing, ok)
    });
    server.handle_next().unwrap();
    server.handle_next().unwrap();
    let ((missing_status, _), (ok_status, ok_body)) = client.join().unwrap();

    assert_eq!(missing_status, 401);
    assert_eq!(ok_status, 200);
    let v: Value = serde_json::from_str(&ok_body).unwrap();
    assert_eq!(v["choices"][0]["message"]["content"], "echo: hi");
}

/// 在子线程发一个 GET 请求，返回 `(status, body)`。
fn get(addr: SocketAddr, path: &str, auth: Option<&str>) -> (u16, String) {
    let url = format!("http://{addr}{path}");
    let mut req = ureq::get(&url);
    if let Some(token) = auth {
        req = req.set("Authorization", token);
    }
    match req.call() {
        Ok(resp) => (resp.status(), resp.into_string().unwrap()),
        Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap()),
        Err(e) => panic!("传输错误: {e}"),
    }
}

#[test]
fn models_returns_configured_model_shape() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, None);

    let client = thread::spawn(move || get(addr, "/v1/models", None));
    server.handle_next().unwrap();
    let (status, body) = client.join().unwrap();

    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["object"], "list");
    assert_eq!(v["data"][0]["id"], "echo");
    assert_eq!(v["data"][0]["object"], "model");
    assert_eq!(v["data"][0]["created"], 0);
    assert_eq!(v["data"][0]["owned_by"], "nanobot");
}

#[test]
fn models_requires_auth_when_api_key_configured() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, Some("secret"));

    let client = thread::spawn(move || {
        // 无 Authorization → 401。
        let missing = get(addr, "/v1/models", None);
        // 错误 Bearer key → 401。
        let wrong = get(addr, "/v1/models", Some("Bearer nope"));
        // 正确 Bearer key → 200。
        let ok = get(addr, "/v1/models", Some("Bearer secret"));
        (missing, wrong, ok)
    });
    server.handle_next().unwrap();
    server.handle_next().unwrap();
    server.handle_next().unwrap();
    let ((missing_status, missing_body), (wrong_status, wrong_body), (ok_status, _)) =
        client.join().unwrap();

    assert_eq!(missing_status, 401);
    let mv: Value = serde_json::from_str(&missing_body).unwrap();
    assert!(mv["error"]["message"]
        .as_str()
        .unwrap()
        .starts_with("Missing Authorization"));

    assert_eq!(wrong_status, 401);
    let wv: Value = serde_json::from_str(&wrong_body).unwrap();
    assert_eq!(wv["error"]["message"], "Invalid API key");

    assert_eq!(ok_status, 200);
}

#[test]
fn health_returns_ok_without_auth() {
    let dir = tempfile::tempdir().unwrap();
    // 即便配置了 api_key，/health 也应放行。
    let (mut server, addr) = bind_server(&dir, Some("secret"));

    let client = thread::spawn(move || {
        let resp = ureq::get(&format!("http://{addr}/health")).call().unwrap();
        (resp.status(), resp.into_string().unwrap())
    });
    server.handle_next().unwrap();
    let (status, body) = client.join().unwrap();

    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["status"], "ok");
}
