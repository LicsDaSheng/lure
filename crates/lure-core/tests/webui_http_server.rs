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
    bind_server_with_config(dir, assets, lure_core::config::Config::default())
}

fn bind_server_with_config(
    dir: &TempDir,
    assets: MapAssets,
    lure_config: lure_core::config::Config,
) -> (WebuiServer<MapAssets>, SocketAddr) {
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
    let server = WebuiServer::bind(
        "127.0.0.1:0",
        assets,
        config,
        lure_config,
        issuer,
        transcript,
    )
    .unwrap();
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
fn delete_session_via_get_delete_path_returns_deleted_shape() {
    // 前端 deleteSession 走 GET /api/sessions/{key}/delete，并读取 result.deleted。
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:gone", "bye");
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, body) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:gone/delete",
        Some(&token),
    );
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["deleted"], true);

    // 行已消失。
    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/sessions", Some(&token));
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["sessions"].as_array().unwrap().len(), 0);

    // 再删 → 404。
    let (status, _) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:gone/delete",
        Some(&token),
    );
    assert_eq!(status, 404);

    // delete_automations query 参数被接受（会话级 automations 尚未实现，删除照常）。
    seed_session(&dir, "websocket:opt", "opt");
    let (status, body) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:opt/delete?delete_automations=true",
        Some(&token),
    );
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["deleted"], true);
}

#[test]
fn delete_session_via_get_delete_path_requires_api_token() {
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:gone", "bye");
    let (mut server, addr) = bind_server(&dir, fake_assets());

    let (status, _) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:gone/delete",
        None,
    );
    assert_eq!(status, 401);
}

#[test]
fn session_automations_stub_returns_empty_jobs() {
    // 会话级 automations 尚未实现：返回对齐上游形状的空 jobs，面板平稳显示"无"。
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:a1", "hi");
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, body) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:a1/automations",
        Some(&token),
    );
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["jobs"].as_array().unwrap().len(), 0);

    // 无 token → 401。
    let (status, _) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:a1/automations",
        None,
    );
    assert_eq!(status, 401);
}

#[test]
fn file_preview_probe_reports_unavailable_and_fetch_404s() {
    // 文件预览能力未实现：probe 返回 available:false 让前端隐藏入口；实际取内容 404。
    let dir = tempfile::tempdir().unwrap();
    seed_session(&dir, "websocket:f1", "hi");
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, body) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:f1/file-preview?path=out.txt&probe=1",
        Some(&token),
    );
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["available"], false);

    let (status, _) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/sessions/websocket:f1/file-preview?path=out.txt",
        Some(&token),
    );
    assert_eq!(status, 404);
}

#[test]
fn skill_detail_returns_404_when_no_skills() {
    // 技能列表为空 → 详情端点对任何名称返回 404（前端 request 抛 ApiError，面板显错）。
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, _) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/webui/skills/nonexistent",
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

    // workspaces：默认壳含 controls，且 default_scope.project_path 必须是非空 string
    // （前端 projectNameFromPath 在加载渲染时对其调 .replace()，缺失即崩）。
    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/workspaces", Some(&token));
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["default_access_mode"], "default");
    assert_eq!(body["controls"]["can_change_project"], false);
    let project_path = body["default_scope"]["project_path"].as_str();
    assert!(
        project_path.is_some_and(|p| !p.is_empty()),
        "default_scope.project_path 必须为非空字符串: {body}"
    );
    assert!(body["default_scope"]["access_mode"].is_string());
}

#[test]
fn settings_payload_reflects_lure_config() {
    use lure_core::config::{Config, ProviderConfig};

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.agents.defaults.model = "deepseek-chat".to_string();
    cfg.agents.defaults.provider = "deepseek".to_string();
    cfg.agents.defaults.max_tokens = 4096;
    cfg.providers.insert(
        "deepseek".to_string(),
        ProviderConfig {
            api_key: Some("sk-secret".to_string()),
            api_base: None,
            enabled: true,
        },
    );

    let (mut server, addr) = bind_server_with_config(&dir, fake_assets(), cfg);
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let (status, body) = roundtrip(&mut server, addr, "GET", "/api/settings", Some(&token));
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();

    // agent 段填真实 config 值。
    assert_eq!(body["agent"]["model"], "deepseek-chat");
    assert_eq!(body["agent"]["provider"], "deepseek");
    assert_eq!(body["agent"]["max_tokens"], 4096);
    assert_eq!(body["agent"]["has_api_key"], true);

    // providers 段来自 config.providers；secret 只回显已配置、不泄明文。
    let providers = body["providers"].as_array().unwrap();
    let ds = providers.iter().find(|p| p["name"] == "deepseek").unwrap();
    assert_eq!(ds["configured"], true);
    assert_eq!(ds["default_api_base"], "https://api.deepseek.com");
    assert_ne!(ds["api_key_hint"], "sk-secret", "不应泄露明文 key");

    // 12 段顶层键都在（结构完整，前端不因缺字段崩溃）。
    for key in [
        "agent",
        "model_presets",
        "providers",
        "web_search",
        "web",
        "api",
        "observability",
        "image_generation",
        "transcription",
        "runtime",
        "usage",
        "advanced",
    ] {
        assert!(!body[key].is_null(), "settings 缺顶层段 {key}");
    }
}

#[test]
fn settings_read_stubs_return_shaped_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let (mut server, addr) = bind_server(&dir, fake_assets());
    let boot = bootstrap(&mut server, addr);
    let token = boot["api_token"].as_str().unwrap().to_string();

    let cases = [
        ("/api/settings/usage", "days"),
        ("/api/settings/provider-models", "models"),
        ("/api/settings/cli-apps", "apps"),
        ("/api/settings/nanobot-features", "features"),
        ("/api/settings/mcp-presets", "presets"),
        ("/api/settings/pairing", "requests"),
    ];
    for (path, key) in cases {
        let (status, body) = roundtrip(&mut server, addr, "GET", path, Some(&token));
        assert_eq!(status, 200, "{path} 应 200");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert!(body[key].is_array(), "{path} 顶层 {key} 应为数组: {body}");
    }

    // version-check 是对象。
    let (status, body) = roundtrip(
        &mut server,
        addr,
        "GET",
        "/api/settings/version-check",
        Some(&token),
    );
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["update_available"], false);
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
