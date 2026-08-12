//! API 子系统：OpenAI-compatible 表面（传输无关）。
//!
//! Phase 9 覆盖请求解析/校验、model 校验、鉴权、响应与 SSE 事件序列、固定 API session
//! key。/v1 HTTP server 已在 Stage 7 删除（未接线生产入口；如需 OpenAI-compat 出站端点
//! 可基于 axum 重建）。/v1/models、media 上传、并发 session lock 留待后续。

mod openai;

pub use openai::{
    api_session_key, authorize, chat_completion_response, error_body, generate_completion_id,
    models_response, parse_chat_request, sse_chunks, sse_content_chunk, sse_finish_chunk,
    validate_model, ApiError, ParsedChatRequest, API_SESSION_KEY, SSE_DONE,
};
