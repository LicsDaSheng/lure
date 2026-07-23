//! 旧配置格式迁移。
//!
//! 对齐上游 `nanobot/config/loader.py::_migrate_config` 的原始 JSON 变换（在 typed
//! 解析之前应用）：
//! - 丢弃 `agents.defaults` 的 legacy `maxMessages`/`max_messages`。
//! - `tools.exec.restrictToWorkspace` → `tools.restrictToWorkspace`（目标不存在时）。
//! - `tools.myEnabled`/`mySet` → `tools.my.{enable, allowSet}`（子键不存在时；已有值优先）。
//!
//! 说明：`tools` 尚未在 typed config 中建模，迁移在原始 JSON 层完成并可独立测试；
//! typed 往返对 `tools` 的保留需等 ToolsConfig 落地（见 upstream-test-ledger）。

use serde_json::{json, Value};

/// 就地把旧格式配置迁移到当前格式。
pub fn migrate_config(data: &mut Value) {
    drop_legacy_max_messages(data);
    migrate_tools(data);
}

/// 丢弃 `agents.defaults.maxMessages` / `max_messages`（现已是内部安全上限）。
fn drop_legacy_max_messages(data: &mut Value) {
    if let Some(defaults) = data
        .pointer_mut("/agents/defaults")
        .and_then(Value::as_object_mut)
    {
        defaults.remove("maxMessages");
        defaults.remove("max_messages");
    }
}

fn migrate_tools(data: &mut Value) {
    let Some(tools) = data.get_mut("tools").and_then(Value::as_object_mut) else {
        return;
    };

    // tools.exec.restrictToWorkspace → tools.restrictToWorkspace（目标不存在时才移动）。
    if !tools.contains_key("restrictToWorkspace") {
        let moved = tools
            .get_mut("exec")
            .and_then(Value::as_object_mut)
            .and_then(|exec| exec.remove("restrictToWorkspace"));
        if let Some(value) = moved {
            tools.insert("restrictToWorkspace".to_string(), value);
        }
    }

    // tools.myEnabled/mySet → tools.my.{enable, allowSet}（已有子键优先）。
    let my_enabled = tools.remove("myEnabled");
    let my_set = tools.remove("mySet");
    if my_enabled.is_some() || my_set.is_some() {
        let my = tools.entry("my").or_insert_with(|| json!({}));
        if let Some(my_obj) = my.as_object_mut() {
            if let Some(enabled) = my_enabled {
                my_obj.entry("enable").or_insert(enabled);
            }
            if let Some(set) = my_set {
                my_obj.entry("allowSet").or_insert(set);
            }
        }
    }
}
