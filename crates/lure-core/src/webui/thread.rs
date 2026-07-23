//! WebUI thread 消息视图。
//!
//! 把 session 的历史投影为前端 thread 载荷（`{key, messages:[{role,content}]}`）。
//! Phase 10 只做 role/content 投影；富媒体、tool 事件、runtime context 留待后续。

use serde_json::{json, Value};

use crate::session::Session;

/// 构建 thread 消息载荷。
pub fn thread_messages(session: &Session) -> Value {
    let messages: Vec<Value> = session
        .messages
        .iter()
        .map(|message| {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("user");
            let content = message.get("content").cloned().unwrap_or_else(|| json!(""));
            json!({"role": role, "content": content})
        })
        .collect();

    json!({
        "key": session.key,
        "messages": messages,
    })
}
