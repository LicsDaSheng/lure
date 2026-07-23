//! WebUI WebSocket 协议：入站解析与出站事件（传输无关）。
//!
//! 对齐上游 `nanobot/channels/websocket/runtime.py` 的线协议：
//! - 出站事件形状 `{"event": <name>, ...}`（message/delta/status/error）。
//! - 入站帧 `{"type", "chat_id", "content"}` → [`InboundMessage`]；非法 chat_id 或
//!   缺 content 返回错误 detail（供 error 事件回送）。
//!
//! Phase 10 不做：真实 WebSocket 连接管理、SSL、attach/detach 生命周期、媒体重写。

use serde_json::{json, Value};

use crate::bus::InboundMessage;

/// 出站 `message` 事件：完整回复。
pub fn message_event(chat_id: &str, text: &str) -> Value {
    json!({"event": "message", "chat_id": chat_id, "text": text})
}

/// 出站 `delta` 事件：流式增量。
pub fn delta_event(chat_id: &str, text: &str) -> Value {
    json!({"event": "delta", "chat_id": chat_id, "text": text})
}

/// 出站 `status` 事件。
pub fn status_event(status: &str) -> Value {
    json!({"event": "status", "status": status})
}

/// 出站 `error` 事件。
pub fn error_event(detail: &str) -> Value {
    json!({"event": "error", "detail": detail})
}

/// 解析入站聊天帧为 [`InboundMessage`]；校验 chat_id 与 content。
///
/// 返回的 `Err(detail)` 适合直接放入 [`error_event`]。
pub fn parse_ws_inbound(frame: &Value, channel: &str) -> Result<InboundMessage, String> {
    let chat_id = frame.get("chat_id").and_then(Value::as_str).unwrap_or("");
    if chat_id.is_empty() {
        return Err("invalid chat_id".to_string());
    }
    let content = frame.get("content").and_then(Value::as_str).unwrap_or("");
    if content.is_empty() {
        return Err("missing content".to_string());
    }
    Ok(InboundMessage::new(channel, chat_id, content))
}
