//! WebUI HTTP server 接线（tiny_http）。
//!
//! 端点面（desktop 第一切片）：
//! - `GET /webui/bootstrap`：签发 token 对并返回握手载荷（loopback 免 secret，对齐上游
//!   localhost-only 语义——server 只绑 127.0.0.1）。
//! - `GET /api/sessions`：会话列表（`Authorization: Bearer <api_token>` 鉴权）。
//! - `DELETE /api/sessions/{key}`：删除会话。
//! - `GET /api/sessions/{key}/webui-thread`：transcript 线程视图（无则 404，前端按 null 处理）。
//! - `GET /api/settings`：设置页主载荷，从 lure [`Config`] 派生（`agent`/`model_presets`/
//!   `providers`/`advanced.exec` 填真实值，其余段结构完整默认值；见 [`settings_api`]）。
//! - 前端加载期只读 stub（同鉴权，返回对齐上游顶层形状的空/默认载荷）：`GET /api/commands`、
//!   `/api/workspaces`、`/api/webui/skills`、`/api/webui/automations`、`/api/webui/sidebar-state`、
//!   `/api/settings/{usage,provider-models,cli-apps,nanobot-features,mcp-presets,pairing,`
//!   `api-service,version-check}`。—— 让页面平稳渲染；**写/更新端点**（`*/update`、`*/start`、
//!   `enable`/`disable`、channels/oauth 等）留待后续切片。
//! - 其余 `/api/*`：404 JSON。其余 GET：静态资源（[`StaticAssets`]），未命中走 SPA fallback。
//!
//! [`settings_api`]: crate::webui::settings_api
//!
//! WebSocket 复用协议在独立端口（`ws_url` 由 bootstrap 报告），不在本 server。

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tiny_http::{Header, Method, Request, Response, Server};

use crate::config::Config;
use crate::session::SessionManager;
use crate::webui::http_api::{bootstrap_payload, sessions_payload};
use crate::webui::list_webui_sessions;
use crate::webui::settings_api::{settings_payload, usage_payload};
use crate::webui::tokens::TokenIssuer;
use crate::webui::transcript::TranscripStore;

/// 静态资源提供者（desktop 侧用 rust-embed 实现；测试用内存表）。
pub trait StaticAssets {
    /// 按路径取资源内容与 MIME（如 `index.html`、`assets/app.js`）。
    fn asset(&self, path: &str) -> Option<(Vec<u8>, &'static str)>;
}

/// server 运行期配置。
#[derive(Debug, Clone)]
pub struct WebuiServerConfig {
    /// workspace 根（session 存储在其 `sessions/` 下）。
    pub workspace: PathBuf,
    /// bootstrap 报告的模型名。
    pub model_name: Option<String>,
    /// WS 复用协议路径（前端拼 `ws_url` 用）。
    pub ws_path: String,
    /// WS 复用协议完整 URL（独立端口）。
    pub ws_url: String,
    /// token 寿命（秒）。
    pub token_ttl_secs: u64,
}

/// WebUI HTTP server。
pub struct WebuiServer<S: StaticAssets> {
    server: Server,
    assets: S,
    config: WebuiServerConfig,
    /// lure 运行时配置：派生 `/api/settings` 载荷。
    lure_config: Config,
    issuer: Arc<Mutex<TokenIssuer>>,
    transcript: TranscripStore,
}

impl<S: StaticAssets> WebuiServer<S> {
    /// 绑定地址（`127.0.0.1:0` 表示随机端口）。
    ///
    /// `issuer` 与 WS server 共享：bootstrap 签发的 token 同时用于 `/api/*` 鉴权
    /// 与 WS 握手校验。
    pub fn bind(
        addr: &str,
        assets: S,
        config: WebuiServerConfig,
        lure_config: Config,
        issuer: Arc<Mutex<TokenIssuer>>,
        transcript: TranscripStore,
    ) -> io::Result<Self> {
        let server = Server::http(addr)
            .map_err(|e| io::Error::new(io::ErrorKind::AddrNotAvailable, e.to_string()))?;
        Ok(Self {
            server,
            assets,
            config,
            lure_config,
            issuer,
            transcript,
        })
    }

    /// 实际监听地址。
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.server
            .server_addr()
            .to_ip()
            .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "非 IP 监听地址"))
    }

    /// 阻塞接收并处理下一个请求。
    pub fn handle_next(&mut self) -> io::Result<bool> {
        match self.server.recv() {
            Ok(request) => {
                self.handle(request)?;
                Ok(true)
            }
            Err(e) => Err(e),
        }
    }

    /// 持续处理请求直到出错。
    pub fn serve_forever(&mut self) -> io::Result<()> {
        while self.handle_next()? {}
        Ok(())
    }

    fn handle(&mut self, request: Request) -> io::Result<()> {
        let method = request.method().clone();
        let path = request.url().split('?').next().unwrap_or("").to_string();
        match (&method, path.as_str()) {
            (Method::Get, "/webui/bootstrap") => self.handle_bootstrap(request),
            (Method::Get, "/api/sessions") => self.handle_sessions(request),
            // 前端加载期只读表面：能力未实现时返回对齐上游顶层形状的空载荷，
            // 让页面正常渲染而非 404 报错（写/更新与 settings 大表面留待后续切片）。
            (Method::Get, "/api/commands") => {
                self.handle_api_stub(request, serde_json::json!({"commands": []}))
            }
            (Method::Get, "/api/workspaces") => {
                let payload = workspaces_stub(&self.config.workspace);
                self.handle_api_stub(request, payload)
            }
            (Method::Get, "/api/webui/skills") => {
                self.handle_api_stub(request, serde_json::json!({"skills": []}))
            }
            (Method::Get, "/api/webui/automations") => {
                self.handle_api_stub(request, serde_json::json!({"jobs": []}))
            }
            (Method::Get, "/api/webui/sidebar-state") => {
                self.handle_api_stub(request, sidebar_state_stub())
            }
            // 设置页主载荷：从 lure Config 派生（agent/presets/providers/exec 真实值，
            // 其余段结构完整默认值）。
            (Method::Get, "/api/settings") => {
                let payload = settings_payload(&self.lure_config);
                self.handle_api_stub(request, payload)
            }
            (Method::Get, "/api/settings/usage") => self.handle_api_stub(request, usage_payload()),
            // 设置页外围只读切片：能力未实现时给对齐上游顶层形状的空载荷。
            (Method::Get, "/api/settings/provider-models") => {
                self.handle_api_stub(request, serde_json::json!({"models": []}))
            }
            (Method::Get, "/api/settings/cli-apps") => {
                self.handle_api_stub(request, serde_json::json!({"apps": []}))
            }
            (Method::Get, "/api/settings/nanobot-features") => {
                self.handle_api_stub(request, serde_json::json!({"features": []}))
            }
            (Method::Get, "/api/settings/mcp-presets") => {
                self.handle_api_stub(request, serde_json::json!({"presets": []}))
            }
            (Method::Get, "/api/settings/pairing") => {
                self.handle_api_stub(request, serde_json::json!({"requests": []}))
            }
            (Method::Get, "/api/settings/api-service") => {
                self.handle_api_stub(request, serde_json::json!({"running": false, "port": null}))
            }
            (Method::Get, "/api/settings/version-check") => self
                .handle_api_stub(request, serde_json::json!({"current": crate::version(), "latest": null, "update_available": false})),
            _ if path.starts_with("/api/sessions/") => self.handle_session_sub(request, &path),
            _ if path.starts_with("/api/") => {
                respond_json(request, 404, serde_json::json!({"error": "not found"}))
            }
            (Method::Get, _) => self.handle_static(request, &path),
            _ => respond_json(request, 404, serde_json::json!({"error": "not found"})),
        }
    }

    fn handle_bootstrap(&mut self, request: Request) -> io::Result<()> {
        let Some(issued) = self.issuer.lock().expect("issuer 锁中毒").try_issue() else {
            return respond_json(
                request,
                429,
                serde_json::json!({"error": "too many outstanding tokens"}),
            );
        };
        let payload = bootstrap_payload(
            &issued,
            &self.config.ws_path,
            &self.config.ws_url,
            self.config.token_ttl_secs,
            self.config.model_name.as_deref(),
        );
        respond_json(request, 200, payload)
    }

    fn handle_sessions(&mut self, request: Request) -> io::Result<()> {
        if !self.authorized(&request) {
            return respond_json(request, 401, serde_json::json!({"error": "unauthorized"}));
        }
        let mut manager = SessionManager::new(&self.config.workspace)?;
        let rows = list_webui_sessions(&mut manager);
        respond_json(request, 200, sessions_payload(&rows))
    }

    /// 只读 /api stub：鉴权通过后返回固定空载荷（顶层形状对齐上游），供尚未实现的
    /// 前端能力面平稳降级。
    fn handle_api_stub(&mut self, request: Request, payload: serde_json::Value) -> io::Result<()> {
        if !self.authorized(&request) {
            return respond_json(request, 401, serde_json::json!({"error": "unauthorized"}));
        }
        respond_json(request, 200, payload)
    }

    fn handle_session_sub(&mut self, request: Request, path: &str) -> io::Result<()> {
        if !self.authorized(&request) {
            return respond_json(request, 401, serde_json::json!({"error": "unauthorized"}));
        }
        let key = path.trim_start_matches("/api/sessions/");
        if key.ends_with("/webui-thread") {
            let session_key = key.trim_end_matches("/webui-thread");
            return match self.transcript.read_thread(session_key) {
                Ok(Some(payload)) => respond_json(request, 200, payload),
                Ok(None) => respond_json(request, 404, serde_json::json!({"error": "not found"})),
                Err(_) => respond_json(
                    request,
                    500,
                    serde_json::json!({"error": "读取 transcript 失败"}),
                ),
            };
        }
        if request.method() == &Method::Delete {
            let mut manager = SessionManager::new(&self.config.workspace)?;
            return match manager.delete_stored(key) {
                Ok(true) => respond_json(request, 200, serde_json::json!({"ok": true})),
                Ok(false) => respond_json(request, 404, serde_json::json!({"error": "not found"})),
                Err(e) => respond_json(
                    request,
                    500,
                    serde_json::json!({"error": format!("删除失败: {e}")}),
                ),
            };
        }
        respond_json(request, 404, serde_json::json!({"error": "not found"}))
    }

    fn handle_static(&mut self, request: Request, path: &str) -> io::Result<()> {
        let trimmed = path.trim_start_matches('/');
        let key = if trimmed.is_empty() {
            "index.html"
        } else {
            trimmed
        };
        if let Some((body, mime)) = self.assets.asset(key) {
            return respond_bytes(request, 200, body, mime);
        }
        // SPA fallback：未知前端路由回退 index.html。
        if let Some((body, mime)) = self.assets.asset("index.html") {
            return respond_bytes(request, 200, body, mime);
        }
        respond_json(request, 404, serde_json::json!({"error": "not found"}))
    }

    fn authorized(&mut self, request: &Request) -> bool {
        let header = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"));
        let Some(header) = header else {
            return false;
        };
        let token = header.value.as_str().trim_start_matches("Bearer ").trim();
        self.issuer
            .lock()
            .expect("issuer 锁中毒")
            .check_api_token(token)
    }
}

/// `/api/webui/sidebar-state` 默认态（对齐上游 `default_webui_sidebar_state`）。
fn sidebar_state_stub() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "pinned_keys": [],
        "archived_keys": [],
        "title_overrides": {},
        "project_name_overrides": {},
        "tags_by_key": {},
        "collapsed_groups": {},
        "view": {
            "density": "comfortable",
            "show_previews": false,
            "show_timestamps": false,
            "show_archived": false,
            "sort": "updated_desc"
        },
        "updated_at": serde_json::Value::Null
    })
}

/// `/api/workspaces` 默认壳（对齐上游 `workspaces_payload`）。
///
/// `default_scope.project_path` 为**必填**（前端 `projectNameFromPath` 在加载渲染时对其
/// 调 `.replace()`，缺失即崩），故用真实 workspace 路径填充。
fn workspaces_stub(workspace: &std::path::Path) -> serde_json::Value {
    let project_path = workspace.to_string_lossy().into_owned();
    let project_name = workspace
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_path.clone());
    serde_json::json!({
        "schema_version": 1,
        "default_access_mode": "default",
        "default_scope": {
            "project_path": project_path,
            "project_name": project_name,
            "access_mode": "full",
            "restrict_to_workspace": false,
        },
        "controls": {"can_change_project": false, "can_use_full_access": false}
    })
}

fn respond_json(request: Request, status: u16, body: serde_json::Value) -> io::Result<()> {
    respond_bytes(
        request,
        status,
        body.to_string().into_bytes(),
        "application/json; charset=utf-8",
    )
}

fn respond_bytes(
    request: Request,
    status: u16,
    body: Vec<u8>,
    mime: &'static str,
) -> io::Result<()> {
    let header = Header::from_bytes("Content-Type", mime).expect("合法 header");
    let response = Response::from_data(body)
        .with_status_code(status as i32)
        .with_header(header);
    request.respond(response)
}
