//! WebUI thread 消息视图。
//!
//! 把 session 的历史投影为前端 thread 载荷（`{key, messages:[{role,content,...}]}`）。
//! 展示所需字段（如 `reasoning_content` 思维链）随消息透出；富媒体、tool 事件、
//! runtime context 留待后续。

use serde_json::{json, Map, Value};

use crate::session::Session;

/// 构建 thread 消息载荷。
pub fn thread_messages(session: &Session) -> Value {
    let messages: Vec<Value> = session.messages.iter().map(project_message).collect();

    json!({
        "key": session.key,
        "messages": messages,
    })
}

/// 投影为展示消息：保留 role/content，并透出 reasoning_content（存在时）。
fn project_message(message: &Value) -> Value {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user");
    let content = message.get("content").cloned().unwrap_or_else(|| json!(""));

    let mut out = Map::new();
    out.insert("role".to_string(), json!(role));
    out.insert("content".to_string(), content);
    if let Some(reasoning) = message.get("reasoning_content").and_then(Value::as_str) {
        out.insert("reasoning_content".to_string(), json!(reasoning));
    }
    Value::Object(out)
}
