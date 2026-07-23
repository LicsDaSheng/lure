//! WebUI 子系统：后端服务协议（传输无关）。
//!
//! Phase 10 覆盖 session 列表、thread 消息视图、WebSocket 事件形状与入站解析、
//! bootstrap/status 载荷。真实 HTTP/WebSocket 服务、前端构建资源、settings/transcript
//! 等大表面留待后续（见 upstream-test-ledger）。

mod session_index;
mod status;
mod thread;
mod ws;

pub use session_index::{list_webui_sessions, SessionRow};
pub use status::webui_status;
pub use thread::thread_messages;
pub use ws::{delta_event, error_event, message_event, parse_ws_inbound, status_event};
