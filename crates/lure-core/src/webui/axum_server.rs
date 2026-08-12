//! WebUI server（axum，Stage 3）。
//!
//! 统一 HTTP + WebSocket 到单个 tokio runtime / 端口，替换 tiny_http + 每连接一线程。
//! 端点面与契约（`/webui/bootstrap`、`/api/*`、WS 复用协议、静态资源 + SPA fallback）
//! 与旧 `http_server`/`ws_server` **字面一致**（E2E 守护）。每 WS 连接一个 tokio task。
//!
//! 状态经 trait object（`Box<dyn StaticAssets>` / `Box<dyn TurnRunner>`）去泛型化，
//! 使 axum handler 均为具体类型（避免泛型 handler 的 trait 解析问题）。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::config::{save_config, Config};
use crate::session::SessionManager;
use crate::webui::http_api::{bootstrap_payload, sessions_payload};
use crate::webui::hub::WsHub;
use crate::webui::list_webui_sessions;
use crate::webui::mux::{MuxSession, TurnRunner};
use crate::webui::settings_api::{settings_payload, usage_payload};
use crate::webui::settings_write::{
    apply_agent_update, apply_provider_update, create_model_configuration,
    update_model_configuration, Params,
};
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
    /// lure config 文件路径：`/api/settings/*/update` 写入落盘的目标。
    pub config_path: PathBuf,
    /// bootstrap 报告的模型名。
    pub model_name: Option<String>,
    /// WS 复用协议路径（前端拼 `ws_url` 用）。
    pub ws_path: String,
    /// WS 复用协议完整 URL（与 HTTP 同端口）。
    pub ws_url: String,
    /// token 寿命（秒）。
    pub token_ttl_secs: u64,
}

/// axum 路由共享状态（trait object 去泛型）。
struct AppState {
    assets: Box<dyn StaticAssets + Send + Sync>,
    config: WebuiServerConfig,
    /// lure 运行时配置：派生 `/api/settings` 载荷；写入端点持锁更新。
    lure_config: Mutex<Config>,
    issuer: Arc<Mutex<TokenIssuer>>,
    transcript: TranscripStore,
    /// 每连接一个 runner（同旧 WsServer factory 语义）。
    factory: Mutex<Box<dyn FnMut() -> Box<dyn TurnRunner + Send> + Send>>,
    hub: WsHub,
    next_conn_id: AtomicU64,
}

/// WebUI server：HTTP + WS 统一入口（非泛型，内部 trait object）。
pub struct WebuiServer {
    listener: tokio::net::TcpListener,
    addr: SocketAddr,
    state: Arc<AppState>,
}

impl WebuiServer {
    /// 绑定地址（`127.0.0.1:0` 表示随机端口）。
    ///
    /// `issuer` 签发 token 同时用于 `/api/*` 鉴权与 WS 握手校验。
    pub async fn bind<S, F, R>(
        addr: &str,
        assets: S,
        mut config: WebuiServerConfig,
        lure_config: Config,
        issuer: Arc<Mutex<TokenIssuer>>,
        transcript: TranscripStore,
        mut factory: F,
    ) -> std::io::Result<Self>
    where
        S: StaticAssets + Send + Sync + 'static,
        F: FnMut() -> R + Send + 'static,
        R: TurnRunner + Send + 'static,
    {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        // WS 与 HTTP 同端口：调用方传空占位时按实际地址回填 ws_url。
        if config.ws_url.is_empty() {
            config.ws_url = format!("ws://{addr}/ws");
        }
        let factory: Box<dyn FnMut() -> Box<dyn TurnRunner + Send> + Send> =
            Box::new(move || Box::new(factory()) as Box<dyn TurnRunner + Send>);
        let state = Arc::new(AppState {
            assets: Box::new(assets),
            config,
            lure_config: Mutex::new(lure_config),
            issuer,
            transcript,
            factory: Mutex::new(factory),
            hub: WsHub::new(),
            next_conn_id: AtomicU64::new(1),
        });
        Ok(Self {
            listener,
            addr,
            state,
        })
    }

    /// 实际监听地址。
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.addr)
    }

    /// 在线连接注册表的克隆句柄：交给 cron runner 等服务端主动推送方。
    pub fn hub(&self) -> WsHub {
        self.state.hub.clone()
    }

    /// 持续服务直到出错或进程退出（axum serve）。
    pub async fn serve_forever(self) -> std::io::Result<()> {
        let fb_state = self.state.clone();
        let fallback_svc = tower::service_fn(move |request: axum::extract::Request| {
            let state = fb_state.clone();
            async move { Ok::<_, std::convert::Infallible>(fallback(request, state).await) }
        });
        let app = Router::new()
            .route("/ws", get(ws_upgrade))
            .fallback_service(fallback_svc)
            .with_state(self.state.clone());
        axum::serve(self.listener, app).await
    }
}

/// WS 握手 query 参数（`?token=...`）。
#[derive(serde::Deserialize)]
struct WsToken {
    token: Option<String>,
}

/// `/ws` upgrade：校验 token 后每连接一个 task。
async fn ws_upgrade(
    ws: WebSocketUpgrade,
    query: Query<WsToken>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let token = query.token.as_deref().unwrap_or("");
    if !state
        .issuer
        .lock()
        .expect("issuer 锁中毒")
        .check_ws_token(token)
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |socket| ws_connection(socket, state))
}

/// 单条 WS 连接的 async 生命周期（替换旧 serve_connection 的线程循环）。
async fn ws_connection(socket: axum::extract::ws::WebSocket, state: Arc<AppState>) {
    let conn_id = state.next_conn_id.fetch_add(1, Ordering::Relaxed);
    let hub = state.hub.clone();
    let runner = state.factory.lock().expect("factory 锁中毒")();

    use futures::{SinkExt, StreamExt};
    let (mut tx, mut rx) = socket.split();
    // hub 订阅通道保持 std mpsc（cron 侧同步 push 不阻塞）；连接侧 try_recv 轮询 drain。
    let (push_tx, mut push_rx) = std::sync::mpsc::channel::<serde_json::Value>();

    // turn 完成时经 transcript 落库（webui-thread GET 读取路径），对齐旧 ws_server 接线。
    let mut mux = MuxSession::new_with_transcript(
        runner,
        state.transcript.clone(),
        tokio::runtime::Handle::current(),
    );

    // 订阅默认 chat_id：用户停留在新会话时，其中创建的 cron 产出可直达本连接。
    let ready = mux.ready_frame();
    if let Some(chat_id) = frame_chat_id(&ready) {
        hub.subscribe(chat_id, conn_id, push_tx.clone());
    }
    if tx
        .send(axum::extract::ws::Message::Text(ready.to_string().into()))
        .await
        .is_err()
    {
        hub.remove_conn(conn_id);
        return;
    }

    connection_loop(
        &mut tx,
        &mut rx,
        &mut mux,
        &hub,
        conn_id,
        &push_tx,
        &mut push_rx,
    )
    .await;
    hub.remove_conn(conn_id);
}

/// 连接主循环：每轮先 drain 推送队列，再读一帧（至多阻塞 `READ_POLL`）。
///
/// 客户端 attach/message 到某会话 → 幂等订阅，使 cron 推送路由到本连接。
/// 语义对齐旧线程版 `connection_loop`（推送延迟上界 = READ_POLL）。
const READ_POLL: std::time::Duration = std::time::Duration::from_millis(250);

async fn connection_loop(
    tx: &mut futures::stream::SplitSink<axum::extract::ws::WebSocket, axum::extract::ws::Message>,
    rx: &mut futures::stream::SplitStream<axum::extract::ws::WebSocket>,
    mux: &mut MuxSession<Box<dyn TurnRunner + Send>>,
    hub: &WsHub,
    conn_id: u64,
    push_tx: &std::sync::mpsc::Sender<serde_json::Value>,
    push_rx: &mut std::sync::mpsc::Receiver<serde_json::Value>,
) -> bool {
    use futures::{SinkExt, StreamExt};
    loop {
        // 1) drain 服务端主动推送（cron 产出等）。
        while let Ok(pushed) = push_rx.try_recv() {
            if tx
                .send(axum::extract::ws::Message::Text(pushed.to_string().into()))
                .await
                .is_err()
            {
                return false;
            }
        }
        // 2) 读一帧客户端入站（超时则回到步骤 1 继续 drain）。
        let frame = match tokio::time::timeout(READ_POLL, rx.next()).await {
            Err(_) => continue,
            Ok(None) => return true,
            Ok(Some(Err(_))) => return false,
            Ok(Some(Ok(frame))) => frame,
        };
        let axum::extract::ws::Message::Text(text) = frame else {
            continue;
        };
        let Ok(frame_value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        // 客户端 attach/message 到某会话 → 订阅它，令 cron 推送可达。
        if let Some(chat_id) = frame_chat_id(&frame_value) {
            hub.subscribe(chat_id, conn_id, push_tx.clone());
        }
        let outbounds = {
            let mut collected = Vec::new();
            mux.handle_frame(&frame_value, &mut |outbound| {
                collected.push(outbound.clone());
            });
            collected
        };
        for outbound in outbounds {
            if tx
                .send(axum::extract::ws::Message::Text(
                    outbound.to_string().into(),
                ))
                .await
                .is_err()
            {
                return false;
            }
        }
    }
}

/// 从帧里取合法 `chat_id`（非空字符串）。
fn frame_chat_id(frame: &serde_json::Value) -> Option<&str> {
    frame
        .get("chat_id")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
}

/// HTTP fallback：按 (method, path) 分发，语义与旧 `handle()` 字面一致。
async fn fallback(request: axum::extract::Request, state: Arc<AppState>) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let uri = request.uri().clone();
    let headers = request.headers().clone();

    match (&method, path.as_str()) {
        (&Method::GET, "/webui/bootstrap") => handle_bootstrap(state).await,
        (&Method::GET, "/api/sessions") => handle_sessions(state, &headers).await,
        (&Method::GET, "/api/commands") => {
            handle_api_stub(state, &headers, serde_json::json!({"commands": []})).await
        }
        (&Method::GET, "/api/workspaces") => {
            let payload = workspaces_stub(&state.config.workspace);
            handle_api_stub(state, &headers, payload).await
        }
        (&Method::GET, "/api/webui/skills") => {
            handle_api_stub(state, &headers, serde_json::json!({"skills": []})).await
        }
        (&Method::GET, "/api/webui/automations") => {
            handle_api_stub(state, &headers, serde_json::json!({"jobs": []})).await
        }
        (&Method::GET, "/api/webui/sidebar-state") => {
            handle_api_stub(state, &headers, sidebar_state_stub()).await
        }
        (&Method::GET, "/api/settings") => {
            let payload = settings_payload(&state.lure_config.lock().expect("lure_config 锁中毒"));
            handle_api_stub(state, &headers, payload).await
        }
        (&Method::GET, "/api/settings/usage") => {
            handle_api_stub(state, &headers, usage_payload()).await
        }
        (&Method::GET, "/api/settings/provider-models") => {
            handle_api_stub(state, &headers, serde_json::json!({"models": []})).await
        }
        (&Method::GET, "/api/settings/cli-apps") => {
            handle_api_stub(state, &headers, serde_json::json!({"apps": []})).await
        }
        (&Method::GET, "/api/settings/nanobot-features") => {
            handle_api_stub(state, &headers, serde_json::json!({"features": []})).await
        }
        (&Method::GET, "/api/settings/mcp-presets") => {
            handle_api_stub(state, &headers, serde_json::json!({"presets": []})).await
        }
        (&Method::GET, "/api/settings/pairing") => {
            handle_api_stub(state, &headers, serde_json::json!({"requests": []})).await
        }
        (&Method::GET, "/api/settings/api-service") => {
            handle_api_stub(
                state,
                &headers,
                serde_json::json!({"running": false, "port": null}),
            )
            .await
        }
        (&Method::GET, "/api/settings/version-check") => {
            handle_api_stub(
                state,
                &headers,
                serde_json::json!({
                    "current": crate::version(),
                    "latest": null,
                    "update_available": false
                }),
            )
            .await
        }
        (_, "/api/settings/update") => {
            handle_settings_write(
                state,
                &headers,
                uri.to_string().as_str(),
                SettingsWrite::Agent,
            )
            .await
        }
        (_, "/api/settings/provider/update") => {
            handle_settings_write(
                state,
                &headers,
                uri.to_string().as_str(),
                SettingsWrite::Provider,
            )
            .await
        }
        (_, "/api/settings/model-configurations/create") => {
            handle_settings_write(
                state,
                &headers,
                uri.to_string().as_str(),
                SettingsWrite::PresetCreate,
            )
            .await
        }
        (_, "/api/settings/model-configurations/update") => {
            handle_settings_write(
                state,
                &headers,
                uri.to_string().as_str(),
                SettingsWrite::PresetUpdate,
            )
            .await
        }
        _ if path.starts_with("/api/sessions/") => {
            handle_session_sub(state, &headers, &uri, &path, &method).await
        }
        _ if path.starts_with("/api/") => json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({"error": "not found"}),
        ),
        (&Method::GET, _) => handle_static(state.assets.as_ref(), &path),
        _ => json_response(
            StatusCode::NOT_FOUND,
            serde_json::json!({"error": "not found"}),
        ),
    }
}

async fn handle_bootstrap(state: Arc<AppState>) -> Response {
    let Some(issued) = state.issuer.lock().expect("issuer 锁中毒").try_issue() else {
        return json_response(
            StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({"error": "too many outstanding tokens"}),
        );
    };
    let payload = bootstrap_payload(
        &issued,
        &state.config.ws_path,
        &state.config.ws_url,
        state.config.token_ttl_secs,
        state.config.model_name.as_deref(),
    );
    json_response(StatusCode::OK, payload)
}

async fn handle_sessions(state: Arc<AppState>, headers: &HeaderMap) -> Response {
    if !authorized(&state, headers) {
        return unauthorized();
    }
    let Ok(mut manager) = SessionManager::new(&state.config.workspace) else {
        return json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": "session 存储不可用"}),
        );
    };
    let rows = list_webui_sessions(&mut manager);
    json_response(StatusCode::OK, sessions_payload(&rows))
}

async fn handle_api_stub(
    state: Arc<AppState>,
    headers: &HeaderMap,
    payload: serde_json::Value,
) -> Response {
    if !authorized(&state, headers) {
        return unauthorized();
    }
    json_response(StatusCode::OK, payload)
}

/// 设置页写入：鉴权 → 解析 query → 映射回 Config → 原子落盘 → 回派生载荷。
async fn handle_settings_write(
    state: Arc<AppState>,
    headers: &HeaderMap,
    uri: &str,
    kind: SettingsWrite,
) -> Response {
    if !authorized(&state, headers) {
        return unauthorized();
    }
    let params = parse_query(uri);
    let result = {
        let config = state.lure_config.lock().expect("lure_config 锁中毒");
        match kind {
            SettingsWrite::Agent => apply_agent_update(&config, &params),
            SettingsWrite::Provider => apply_provider_update(&config, &params),
            SettingsWrite::PresetCreate => create_model_configuration(&config, &params),
            SettingsWrite::PresetUpdate => update_model_configuration(&config, &params),
        }
    };
    let next = match result {
        Ok(next) => next,
        Err(message) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                serde_json::json!({"error": message}),
            )
        }
    };
    if let Err(e) = save_config(&next, &state.config.config_path) {
        return json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({"error": format!("保存配置失败: {e}")}),
        );
    }
    let mut config = state.lure_config.lock().expect("lure_config 锁中毒");
    *config = next;
    let payload = settings_payload(&config);
    json_response(StatusCode::OK, payload)
}

async fn handle_session_sub(
    state: Arc<AppState>,
    headers: &HeaderMap,
    uri: &axum::http::Uri,
    path: &str,
    method: &Method,
) -> Response {
    if !authorized(&state, headers) {
        return unauthorized();
    }
    let method_is_delete = *method == Method::DELETE;
    let key = path.trim_start_matches("/api/sessions/");
    if key.ends_with("/webui-thread") {
        let session_key = percent_decode(key.trim_end_matches("/webui-thread"));
        return match state.transcript.read_thread(&session_key) {
            Ok(Some(payload)) => json_response(StatusCode::OK, payload),
            Ok(None) => json_response(
                StatusCode::NOT_FOUND,
                serde_json::json!({"error": "not found"}),
            ),
            Err(_) => json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "读取 transcript 失败"}),
            ),
        };
    }
    if key.ends_with("/automations") {
        return json_response(StatusCode::OK, serde_json::json!({"jobs": []}));
    }
    if key.ends_with("/file-preview") {
        let is_probe = uri
            .query()
            .map(|q| q.split('&').any(|kv| kv == "probe=1"))
            .unwrap_or(false);
        return if is_probe {
            json_response(StatusCode::OK, serde_json::json!({"available": false}))
        } else {
            json_response(
                StatusCode::NOT_FOUND,
                serde_json::json!({"error": "file preview 未实现"}),
            )
        };
    }
    let delete_target = if key.ends_with("/delete") {
        Some(key.trim_end_matches("/delete"))
    } else if method_is_delete {
        Some(key)
    } else {
        None
    };
    if let Some(session_key) = delete_target {
        let session_key = percent_decode(session_key);
        let Ok(mut manager) = SessionManager::new(&state.config.workspace) else {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "session 存储不可用"}),
            );
        };
        return match manager.delete_stored(&session_key) {
            Ok(true) => json_response(StatusCode::OK, serde_json::json!({"deleted": true})),
            Ok(false) => json_response(
                StatusCode::NOT_FOUND,
                serde_json::json!({"error": "not found"}),
            ),
            Err(e) => json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": format!("删除失败: {e}")}),
            ),
        };
    }
    json_response(
        StatusCode::NOT_FOUND,
        serde_json::json!({"error": "not found"}),
    )
}

fn handle_static(assets: &dyn StaticAssets, path: &str) -> Response {
    let trimmed = path.trim_start_matches('/');
    let key = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    if let Some((body, mime)) = assets.asset(key) {
        return bytes_response(StatusCode::OK, body, mime);
    }
    if let Some((body, mime)) = assets.asset("index.html") {
        return bytes_response(StatusCode::OK, body, mime);
    }
    json_response(
        StatusCode::NOT_FOUND,
        serde_json::json!({"error": "not found"}),
    )
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(header) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let token = header.trim_start_matches("Bearer ").trim();
    state
        .issuer
        .lock()
        .expect("issuer 锁中毒")
        .check_api_token(token)
}

fn unauthorized() -> Response {
    json_response(
        StatusCode::UNAUTHORIZED,
        serde_json::json!({"error": "unauthorized"}),
    )
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

fn bytes_response(status: StatusCode, body: Vec<u8>, mime: &'static str) -> Response {
    (status, [(header::CONTENT_TYPE, mime)], body).into_response()
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

/// 设置页写入端点的类别（路由分发用）。
enum SettingsWrite {
    Agent,
    Provider,
    PresetCreate,
    PresetUpdate,
}

/// 从完整 URL 解析 query 为已解码的参数表（语义同旧 `parse_query`）。
fn parse_query(url: &str) -> Params {
    let mut params = Params::new();
    let Some(query) = url.split('?').nth(1) else {
        return params;
    };
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(decode_form_component(key), decode_form_component(value));
    }
    params
}

/// form-urlencoded 分量解码：`+` → 空格后走百分号解码。
fn decode_form_component(raw: &str) -> String {
    let spaced = raw.replace('+', " ");
    percent_decode(&spaced).into_owned()
}

/// 最小百分号解码：还原 `%XX`。不完整或非 UTF-8 的转义序列原样返回。
fn percent_decode(input: &str) -> std::borrow::Cow<'_, str> {
    if !input.contains('%') {
        return std::borrow::Cow::Borrowed(input);
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    match String::from_utf8(out) {
        Ok(s) => std::borrow::Cow::Owned(s),
        Err(_) => std::borrow::Cow::Borrowed(input),
    }
}

/// 把 [`crate::agent::AgentLoop`] 适配为 mux 的 [`TurnRunner`]（channel 固定 `websocket`）。
///
/// 含可选的 dream runner：每轮 turn 完成后按阈值触发 memory consolidation。
pub struct AgentTurnRunner {
    agent: crate::agent::AgentLoop,
    dream_runner: Option<Box<dyn crate::memory::DreamRunner>>,
    dream_threshold: usize,
}

impl AgentTurnRunner {
    /// 不挂 dream runner 的最小实例。
    pub fn new(agent: crate::agent::AgentLoop) -> Self {
        Self {
            agent,
            dream_runner: None,
            dream_threshold: 10,
        }
    }

    /// 挂载 dream runner 并在每轮 turn 后自动检查阈值触发。
    pub fn with_dream(
        mut self,
        runner: Box<dyn crate::memory::DreamRunner>,
        threshold: usize,
    ) -> Self {
        self.dream_runner = Some(runner);
        self.dream_threshold = threshold;
        self
    }
}

#[async_trait::async_trait]
impl TurnRunner for AgentTurnRunner {
    async fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut (dyn FnMut(crate::agent::ProgressEvent) + Send),
    ) -> Result<String, String> {
        let input = crate::bus::InboundMessage::new("websocket", chat_id, content);
        let outcome = self
            .agent
            .process_streaming(&input, on_progress)
            .await
            .map_err(|e| e.to_string())?;

        // 阈值触发 dream consolidation。
        if let Some(ref runner) = self.dream_runner {
            let _ = self
                .agent
                .maybe_consolidate(runner.as_ref(), self.dream_threshold)
                .await;
        }

        Ok(outcome.final_content)
    }
}
