//! API 子系统：OpenAI-compatible 表面（传输无关）。
//!
//! Phase 9 覆盖请求解析/校验、model 校验、鉴权、响应与 SSE 事件序列、固定 API session
//! key。真实 HTTP server、/v1/models、media 上传、并发 session lock 留待后续。

mod openai;
mod server;

pub use openai::{
    api_session_key, authorize, chat_completion_response, error_body, generate_completion_id,
    parse_chat_request, sse_chunks, validate_model, ApiError, ParsedChatRequest, API_SESSION_KEY,
};
pub use server::{ChatRunError, ChatRunner, ChatServer, ServerConfig};
