//! WebUI HTTP server 接线：bootstrap、/api/sessions、静态资源、鉴权。
//!
//! 测试拓扑对齐 `api_server.rs`：server 留主线程，ureq 客户端跑子线程，
//! 每个请求对应一次 `handle_next`。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use lure_core::session::SessionManager;
use lure_core::webui::http_server::{StaticAssets, WebuiServer, WebuiServerConfig};
use lure_core::webui::tokens::TokenIssuer;
use serde_json::Value;
use tempfile::TempDir;

/// 内存静态资源表。
pub struct MapAssets(pub HashMap<String, (Vec<u8>, &'static str)>);

impl StaticAssets for MapAssets {
    fn asset(&self, path: &str) -> Option<(Vec<u8>, &'static str)> {
        self.0.get(path).cloned()
    }
}

fn fake_assets() -> MapAssets {
    MapAssets(HashMap::from([
        (
            "index.html".to_string(),
            (b"<html>app</html>".to_vec(), "text/html; charset=utf-8"),
        ),
        (
            "assets/app.js".to_string(),
            (b"console.log(1)".to_vec(), "text/javascript; charset=utf-8"),
        ),
    ]))
}

fn seed_session(dir: &TempDir, key: &str, user: &str) {
    let mut manager = SessionManager::new(dir.path()).unwrap();
    {
        let session = manager.get_or_create(key).unwrap();
        session.add_message("user", user);
    }
    manager.save(key, false).unwrap();
}

fn bind_server(dir: &TempDir, assets: MapAssets) -> (WebuiServer<MapAssets>, SocketAddr) {
    let config = WebuiServerConfig {
        workspace: dir.path().to_path_buf(),
        model_name: Some("echo".to_string()),
        ws_path: "/ws".to_string(),
        ws_url: "ws://127.0.0.1:40099/ws".to_string(),
        token_ttl_secs: 3600,
    };
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(config.token_ttl_secs, 16)));
    let transcript =
        lure_core::webui::transcript::TranscripStore::new(dir.path().join("webui")).unwrap();
    let server = WebuiServer::bind("127.0.0.1:0", assets, config, issuer, transcript).unwrap();
    let addr = server.local_addr().unwrap();
    (server, addr)
}

/// 子线程发 GET/DELETE；调用方须在主线程 `handle_next` 后 `join` 取 (status, body)。
fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    auth: Option<&str>,
) -> JoinHandle<(u16, String)> {
    let url = format!("http://{addr}{path}");
    let method = method.to_string();
    let token = auth.map(str::to_string);
    thread::spawn(move || {
        let mut req = ureq::request(&method, &url);
        if let Some(token) = token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        match req.call() {
            Ok(resp) => (resp.status(), resp.into_string().unwrap()),
            Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().unwrap()),
            Err(e) => panic!("传输错误: {e}"),
        }
    })
}

/// 一轮请求-应答：子线程发请求，主线程处理，返回 (status, body)。
fn roundtrip(
    server: &mut WebuiServer<MapAssets>,
    addr: SocketAddr,
    method: &str,
    path: &str,
    auth: Option<&str>,
) -> (u16, String) {
    let pending = request(addr, method, path, auth);
    server.handle_next().unwrap();
    pending.join().unwrap()
}

/// 取 bootstrap 载荷（含 api_token）。
fn bootstrap(server: &mut WebuiServer<MapAssets>, addr: SocketAddr) -> Value {
    let (status, body) = roundtrip(server, addr, "GET", "/webui/bootstrap", None);
    assert_eq!(status, 200);
    serde_json::from_str(&body).unwrap()
}

#[test]
fn bootstrap_issues_tokens_and_reports_ws_url() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());

    let payload = bootstrap(&mut server, addr);
    for field in ["token", "api_token", "ws_path", "ws_url"] {
        assert!(payload[field].as_str().unwrap().len() > 1, "缺字段 {field}");
    }
    assert_eq!(payload["ws_path"], "/ws");
    assert_eq!(payload["ws_url"], "ws://127.0.0.1:40099/ws");
    assert_eq!(payload["expires_in"], 3600);
    assert_eq!(payload["model_name"], "echo");
}

#[test]
fn sessions_requires_api_token_and_lists_rows() {
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:c1", "hello preview");
    let (mut server, addr) = bind_server(&dir, fake_assets());

    let (status, _) = roundtrip(&mut server, addr, "GET", "/api/sessions", None);
    assert_eq!(status, 401);

    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/sessions", Some(&token));
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    let rows = body["sessions"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["key"], "websocket:c1");
    assert_eq!(rows[0]["preview"], "hello preview");
    assert_eq!(rows[0]["title"], "");
}

#[test]
fn unknown_api_route_returns_404_json() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/nope", Some(&token));
    assert_eq!(status, 404);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["error"], "not found");
}

#[test]
fn static_assets_serve_and_spa_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());

    // 根路径 → index.html。
    let (status, body) = roundtrip(&mut server, addr, "GET", "/", None);
    assert_eq!(status, 200);
    assert_eq!(body, "<html>app</html>");

    // 真实资源。
    let (status, body) = roundtrip(&mut server, addr, "GET", "/assets/app.js", None);
    assert_eq!(status, 200);
    assert_eq!(body, "console.log(1)");

    // 未知非 API 路径 → SPA fallback 到 index.html。
    let (status, body) = roundtrip(&mut server, addr, "GET", "/chat/abc-def", None);
    assert_eq!(status, 200);
    assert_eq!(body, "<html>app</html>");
}

#[test]
fn delete_session_removes_row() {
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:gone", "bye");
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, _) = roundtrip(
        &mut server,
        addr,
        "DELETE",
        "/api/sessions/websocket:gone",
        Some(&token),
    );
    assert_eq!(status, 200);

    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/sessions", Some(&token));
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["sessions"].as_array().unwrap().len(), 0);

    // 再删 → 404。
    let (status, _) = roundtrip(
        &mut server,
        addr,
        "DELETE",
        "/api/sessions/websocket:gone",
        Some(&token),
    );
    assert_eq!(status, 404);
}

#[test]
fn webui_thread_returns_404_until_transcript_lands() {
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:t1", "hi");
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    // 前端 fetchWebuiThread 对 404 按 null 处理（新会话流程不受影响）。
    let (status, _) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:t1/webui-thread",
        Some(&token),
    );
    assert_eq!(status, 404);
}

#[test]
fn webui_readonly_stubs_return_shaped_payloads_with_auth() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    // 加载期只读 stub：鉴权后返回对齐上游顶层形状的空载荷（前端可平稳渲染）。
    let cases = [
        ("/api/commands", "commands"),
        ("/api/webui/skills", "skills"),
        ("/api/webui/automations", "jobs"),
    ];
    for (path, key) in cases {
        let (status, body) = roundtrip(&mut server, addr, "GET", path, Some(&token));
        assert_eq!(status, 200, "{path} 应 200");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert!(body[key].is_array(), "{path} 顶层 {key} 应为数组: {body}");
        assert_eq!(body[key].as_array().unwrap().len(), 0, "{path} 应为空");
    }

    // sidebar-state：默认态对象含 view 子结构。
    let (status, body) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/webui/sidebar-state",
        Some(&token),
    );
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["view"]["density"], "comfortable");
    assert!(body["pinned_keys"].is_array());

    // workspaces：默认壳含 controls。
    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/workspaces", Some(&token));
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["default_access_mode"], "default");
    assert_eq!(body["controls"]["can_change_project"], false);
}

#[test]
fn webui_readonly_stubs_require_api_token() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());

    // 无 token → 401（与 /api/sessions 一致）。
    for path in [
        "/api/commands",
        "/api/webui/skills",
        "/api/webui/sidebar-state",
    ] {
        let (status, _) = roundtrip(&mut server, addr, "GET", path, None);
        assert_eq!(status, 401, "{path} 无 token 应 401");
    }
}

#[test]
fn session_manager_delete_stored_removes_file_and_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = SessionManager::new(dir.path()).unwrap();
    {
        let session = manager.get_or_create("websocket:d").unwrap();
        session.add_message("user", "x");
    }
    manager.save("websocket:d", false).unwrap();
    assert!(manager.session_path("websocket:d").exists());

    assert!(manager.delete_stored("websocket:d").unwrap());
    assert!(!manager.session_path("websocket:d").exists());
    // 幂等：不存在 → Ok(false)。
    assert!(!manager.delete_stored("websocket:d").unwrap());
}
