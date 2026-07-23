//! WebUI bootstrap / status 载荷。
//!
//! 对齐上游 WebUI status 的最小形状：运行状态、版本、会话数。真实 build/资源指纹、
//! feature flags、token usage 等留待后续。

use serde_json::{json, Value};

use crate::session::SessionManager;

/// 构建 bootstrap/status 载荷。
pub fn webui_status(manager: &SessionManager) -> Value {
    json!({
        "status": "ok",
        "version": crate::version(),
        "sessions": manager.list_stored_keys().len(),
    })
}
