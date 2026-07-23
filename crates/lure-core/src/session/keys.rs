//! Session key 常量与助手。
//!
//! 对齐上游 `nanobot/session/keys.py`：
//! - 统一会话固定为 `unified:default`。
//! - 普通会话 key 为 `channel:chat_id`。

/// 统一会话 key。
pub const UNIFIED_SESSION_KEY: &str = "unified:default";

/// 返回 channel/chat 对应的 session key。
///
/// `unified_session` 为真时返回统一会话 key，否则拼接 `channel:chat_id`。
pub fn session_key_for_channel(channel: &str, chat_id: &str, unified_session: bool) -> String {
    if unified_session {
        UNIFIED_SESSION_KEY.to_string()
    } else {
        format!("{channel}:{chat_id}")
    }
}
