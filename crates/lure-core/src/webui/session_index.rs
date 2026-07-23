//! WebUI session 列表。
//!
//! 对齐上游 `nanobot/webui/session_list_index.py` 的行数据（key + preview + 计数 +
//! 更新时间）。Phase 10 每次扫描构建；上游的索引缓存/增量重扫优化留待后续。

use serde::Serialize;
use serde_json::Value;

use crate::session::SessionManager;

/// 一行 WebUI session 摘要。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionRow {
    /// session key。
    pub key: String,
    /// 首条 user 消息预览。
    pub preview: String,
    /// 消息条数。
    pub message_count: usize,
    /// 更新时间（RFC3339）。
    pub updated_at: String,
}

/// 枚举 workspace 中的 session，构建 WebUI 列表行。
pub fn list_webui_sessions(manager: &mut SessionManager) -> Vec<SessionRow> {
    let keys = manager.list_stored_keys();
    let mut rows = Vec::with_capacity(keys.len());
    for key in keys {
        let Ok(session) = manager.get_or_create(&key) else {
            continue;
        };
        rows.push(SessionRow {
            key: session.key.clone(),
            preview: first_user_preview(&session.messages),
            message_count: session.messages.len(),
            updated_at: session.updated_at.to_rfc3339(),
        });
    }
    rows
}

/// 首条 role 为 user 的消息内容，缺失返回空串。
fn first_user_preview(messages: &[Value]) -> String {
    messages
        .iter()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
