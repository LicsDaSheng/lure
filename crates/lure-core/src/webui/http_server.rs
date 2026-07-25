//! WebUI HTTP server 接线（tiny_http）。
//!
//! 端点面（desktop 第一切片）：
//! - `GET /webui/bootstrap`：签发 token 对并返回握手载荷（loopback 免 secret，对齐上游
//!   localhost-only 语义——server 只绑 127.0.0.1）。
//! - `GET /api/sessions`：会话列表（`Authorization: Bearer <api_token>` 鉴权）。
//! - `DELETE /api/sessions/{key}`：删除会话。
//! - `GET /api/sessions/{key}/webui-thread`：404（transcript 表面留待后续切片，
//!   前端按 null 处理）。
//! - 其余 GET：静态资源（[`StaticAssets`]），未命中走 SPA fallback 到 `index.html`。
//!
//! WebSocket 复用协议在独立端口（`ws_url` 由 bootstrap 报告），不在本 server。

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tiny_http::{Header, Method, Request, Response, Server};

use crate::session::SessionManager;
use crate::webui::http_api::{bootstrap_payload, sessions_payload};
use crate::webui::list_webui_sessions;
use crate::webui::tokens::TokenIssuer;

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
    issuer: Arc<Mutex<TokenIssuer>>,
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
        issuer: Arc<Mutex<TokenIssuer>>,
    ) -> io::Result<Self> {
        let server = Server::http(addr)
            .map_err(|e| io::Error::new(io::ErrorKind::AddrNotAvailable, e.to_string()))?;
        Ok(Self {
            server,
            assets,
            config,
            issuer,
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

    fn handle_session_sub(&mut self, request: Request, path: &str) -> io::Result<()> {
        if !self.authorized(&request) {
            return respond_json(request, 401, serde_json::json!({"error": "unauthorized"}));
        }
        let key = path.trim_start_matches("/api/sessions/");
        if key.ends_with("/webui-thread") {
            // transcript 表面未落地：404，前端按 null 处理。
            return respond_json(request, 404, serde_json::json!({"error": "not found"}));
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
