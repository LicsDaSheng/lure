//! WebUI 子系统：后端服务协议（传输无关）。
//!
//! Phase 10 覆盖 session 列表、thread 消息视图、WebSocket 事件形状与入站解析、
//! bootstrap/status 载荷。真实 HTTP/WebSocket 服务、前端构建资源、settings/transcript
//! 等大表面留待后续（见 upstream-test-ledger）。

mod session_index;
mod status;
mod thread;
mod ws;

/// WebUI HTTP 面的传输无关载荷构造。
pub mod http_api;
/// WebUI HTTP server 接线（tiny_http）。
pub mod http_server;
/// WebUI 复用协议会话 handler（传输无关）。
pub mod mux;
/// `/api/settings` 载荷派生（从 lure Config）。
pub mod settings_api;
/// `/api/settings/*/update` 写入语义（映射回 lure Config）。
pub mod settings_write;
/// WebUI token 签发与校验。
pub mod tokens;
/// WebUI transcript 存储。
pub mod transcript;
/// WebUI WS transport：真实 WebSocket 接线 mux。
pub mod ws_server;

pub use session_index::{list_webui_sessions, SessionRow};
pub use status::webui_status;
pub use thread::thread_messages;
pub use ws::{delta_event, error_event, message_event, parse_ws_inbound, status_event};
