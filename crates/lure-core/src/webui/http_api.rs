//! WebUI HTTP 面的传输无关载荷构造。
//!
//! 对齐上游 `ws_http.py::_handle_bootstrap` 与 `session_list_index.py::_public_row`
//! 的响应形状；HTTP 传输接线（tiny_http 路由）在 `webui::http_server`。

use serde_json::{json, Value};

use crate::webui::tokens::IssuedTokens;
use crate::webui::SessionRow;

/// 构造 `/webui/bootstrap` 响应。
///
/// `model_name` 为 `None` 时输出 `null`（前端 `modelName ?? null`）。
/// `limits`/`runtime_surface`/`runtime_capabilities` 等可选字段后续切片补充。
pub fn bootstrap_payload(
    issued: &IssuedTokens,
    ws_path: &str,
    ws_url: &str,
    expires_in: u64,
    model_name: Option<&str>,
) -> Value {
    json!({
        "token": issued.token,
        "api_token": issued.api_token,
        "ws_path": ws_path,
        "ws_url": ws_url,
        "expires_in": expires_in,
        "model_name": model_name,
    })
}

/// 构造 `GET /api/sessions` 响应：`{sessions: [...]}`，按 updated_at 倒序。
///
/// 行形状对齐上游 `_public_row`（key/created_at/updated_at/title/preview）；
/// title 暂固定空串（上游取自 session metadata，lure 暂无 title 概念）。
pub fn sessions_payload(rows: &[SessionRow]) -> Value {
    let mut sorted: Vec<&SessionRow> = rows.iter().collect();
    sorted.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let sessions: Vec<Value> = sorted
        .iter()
        .map(|row| {
            json!({
                "key": row.key,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
                "title": "",
                "preview": row.preview,
            })
        })
        .collect();
    json!({"sessions": sessions})
}
