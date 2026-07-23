//! 显式持续目标（goal）的 session metadata 派生视图。
//!
//! 对齐上游 `nanobot/session/goal_state.py` 的纯函数部分：读取 `goal_state`
//! （兼容 legacy `thread_goal` key）、解析、判断激活、生成运行时行与 WebSocket blob。
//!
//! `runner_wall_llm_timeout_s` 依赖 `SessionManager` 且属 runner 运行时关注点，
//! 留待 Phase 3 落地（见 upstream-test-ledger）。

use serde_json::{Map, Value};

/// metadata 中存储 goal 的主 key。
pub const GOAL_STATE_KEY: &str = "goal_state";
/// 触发显式 goal 的命令。
pub const GOAL_COMMAND: &str = "/goal";
/// runtime 行中 objective 的最大字符数。
pub const MAX_GOAL_OBJECTIVE_CHARS: usize = 4000;
/// 旧版本存储 goal 的 legacy key。
const LEGACY_GOAL_STATE_SESSION_KEY: &str = "thread_goal";
/// WebSocket blob 中 objective 的最大字符数。
const MAX_OBJECTIVE_WS: usize = 600;

/// 返回 `goal_state`（或 legacy `thread_goal`）原始 blob。
pub fn goal_state_raw(metadata: Option<&Map<String, Value>>) -> Option<Value> {
    let meta = metadata?;
    if meta.is_empty() {
        return None;
    }
    if let Some(value) = meta.get(GOAL_STATE_KEY) {
        return Some(value.clone());
    }
    meta.get(LEGACY_GOAL_STATE_SESSION_KEY).cloned()
}

/// 迁移写入到主 key 后，移除 legacy metadata key。
pub fn discard_legacy_goal_state_key(metadata: &mut Map<String, Value>) {
    metadata.remove(LEGACY_GOAL_STATE_SESSION_KEY);
}

/// 把 goal blob 解析为对象；接受对象或 JSON 字符串，其余返回 `None`。
pub fn parse_goal_state(blob: Option<&Value>) -> Option<Map<String, Value>> {
    match blob {
        Some(Value::Object(map)) => Some(map.clone()),
        Some(Value::String(text)) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => Some(map),
            _ => None,
        },
        _ => None,
    }
}

/// 该 session 是否存在激活的持续目标。
pub fn sustained_goal_active(metadata: Option<&Map<String, Value>>) -> bool {
    let raw = goal_state_raw(metadata);
    matches!(
        parse_goal_state(raw.as_ref()),
        Some(goal) if status_is_active(&goal)
    )
}

/// 本轮是否由 `/goal` 命令显式发起。
pub fn explicit_goal_requested(message_metadata: Option<&Map<String, Value>>) -> bool {
    let Some(meta) = message_metadata else {
        return false;
    };
    if meta.get("goal_requested") == Some(&Value::Bool(true)) {
        return true;
    }
    meta.get("original_command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        == GOAL_COMMAND
}

/// 本轮是否应使用持续目标的运行时限制。
pub fn sustained_goal_turn(
    metadata: Option<&Map<String, Value>>,
    message_metadata: Option<&Map<String, Value>>,
) -> bool {
    sustained_goal_active(metadata) || explicit_goal_requested(message_metadata)
}

/// goal 激活时追加到 Runtime Context 块的文本行。
pub fn goal_state_runtime_lines(metadata: Option<&Map<String, Value>>) -> Vec<String> {
    let raw = goal_state_raw(metadata);
    let Some(goal) = parse_goal_state(raw.as_ref()) else {
        return Vec::new();
    };
    if !status_is_active(&goal) {
        return Vec::new();
    }

    let objective = str_field(&goal, "objective");
    if objective.is_empty() {
        return vec!["Goal: active (no objective text stored).".to_string()];
    }
    let objective = truncate_chars(&objective, MAX_GOAL_OBJECTIVE_CHARS, "\n… (truncated)");

    let mut out = vec!["Goal (active):".to_string(), objective];
    let hint = str_field(&goal, "ui_summary");
    if !hint.is_empty() {
        out.push(format!("Summary: {hint}"));
    }
    out
}

/// WebSocket `goal_state` 事件的 JSON-safe 快照。
pub fn goal_state_ws_blob(metadata: Option<&Map<String, Value>>) -> Value {
    let raw = goal_state_raw(metadata);
    let goal = parse_goal_state(raw.as_ref());
    if let Some(goal) = goal {
        if status_is_active(&goal) {
            let mut objective = str_field(&goal, "objective");
            if objective.chars().count() > MAX_OBJECTIVE_WS {
                objective = truncate_chars(&objective, MAX_OBJECTIVE_WS, "…");
            }
            let summary: String = str_field(&goal, "ui_summary").chars().take(120).collect();

            let mut blob = Map::new();
            blob.insert("active".to_string(), Value::Bool(true));
            if !summary.is_empty() {
                blob.insert("ui_summary".to_string(), Value::String(summary));
            }
            if !objective.is_empty() {
                blob.insert("objective".to_string(), Value::String(objective));
            }
            return Value::Object(blob);
        }
    }
    let mut blob = Map::new();
    blob.insert("active".to_string(), Value::Bool(false));
    Value::Object(blob)
}

fn status_is_active(goal: &Map<String, Value>) -> bool {
    goal.get("status").and_then(Value::as_str) == Some("active")
}

/// 读取字符串字段并 trim；缺失或非字符串返回空串。
fn str_field(goal: &Map<String, Value>, key: &str) -> String {
    goal.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

/// 若超过 `max` 字符，截断到 `max` 字符、去尾部空白后追加 `suffix`。
fn truncate_chars(text: &str, max: usize, suffix: &str) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{}{}", head.trim_end(), suffix)
}
