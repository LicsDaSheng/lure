//! 映射上游 `tests/session/test_goal_state.py` 的纯函数场景。
//!
//! 暂未映射：`runner_wall_llm_timeout_s` 依赖 `SessionManager` 且属 runner 关注点，
//! 留待 Phase 3（见 upstream-test-ledger）。

use lure_core::session::goal_state::{
    discard_legacy_goal_state_key, explicit_goal_requested, goal_state_runtime_lines,
    goal_state_ws_blob, parse_goal_state, sustained_goal_active, GOAL_STATE_KEY,
    MAX_GOAL_OBJECTIVE_CHARS,
};
use serde_json::{json, Map, Value};

fn obj(value: &Value) -> &Map<String, Value> {
    value.as_object().unwrap()
}

#[tokio::test]
async fn runtime_lines_empty_when_no_metadata() {
    assert_eq!(goal_state_runtime_lines(None), Vec::<String>::new());
    let empty = json!({});
    assert_eq!(
        goal_state_runtime_lines(Some(obj(&empty))),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn runtime_lines_empty_when_completed() {
    let meta = json!({ "goal_state": {"status": "completed", "objective": "was doing X"} });
    assert_eq!(
        goal_state_runtime_lines(Some(obj(&meta))),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn runtime_lines_include_objective_when_active() {
    let meta = json!({
        "goal_state": {"status": "active", "objective": "Ship the fix.", "ui_summary": "fix"}
    });
    let lines = goal_state_runtime_lines(Some(obj(&meta)));
    assert!(lines.iter().any(|l| l == "Goal (active):"));
    assert!(lines.iter().any(|l| l == "Ship the fix."));
    assert!(lines.iter().any(|l| l == "Summary: fix"));
}

#[tokio::test]
async fn runtime_lines_preserve_maximum_accepted_objective() {
    let objective = "x".repeat(MAX_GOAL_OBJECTIVE_CHARS);
    let meta = json!({ "goal_state": {"status": "active", "objective": objective} });
    let lines = goal_state_runtime_lines(Some(obj(&meta)));
    assert_eq!(lines, vec!["Goal (active):".to_string(), objective]);
}

#[tokio::test]
async fn runtime_lines_read_legacy_thread_goal_key() {
    let meta = json!({
        "thread_goal": {"status": "active", "objective": "Legacy key.", "ui_summary": "L"}
    });
    let lines = goal_state_runtime_lines(Some(obj(&meta)));
    assert!(lines.iter().any(|l| l == "Legacy key."));
}

#[tokio::test]
async fn goal_state_key_takes_precedence_over_legacy() {
    let meta = json!({
        "goal_state": {"status": "active", "objective": "New key wins.", "ui_summary": "n"},
        "thread_goal": {"status": "active", "objective": "Ignored.", "ui_summary": "o"}
    });
    let lines = goal_state_runtime_lines(Some(obj(&meta)));
    assert!(lines.iter().any(|l| l == "New key wins."));
    assert!(!lines.join("").contains("Ignored."));
}

#[tokio::test]
async fn discard_legacy_key_removes_only_legacy() {
    let source = json!({ "thread_goal": {"x": 1}, "goal_state": {"status": "active"} });
    let mut meta = obj(&source).clone();
    discard_legacy_goal_state_key(&mut meta);
    assert!(!meta.contains_key("thread_goal"));
    assert!(meta.contains_key(GOAL_STATE_KEY));
}

#[tokio::test]
async fn parse_goal_state_accepts_json_string() {
    let blob = json!("{\"status\":\"active\",\"objective\":\"x\"}");
    let parsed = parse_goal_state(Some(&blob)).unwrap();
    assert_eq!(parsed.get("status").unwrap(), "active");
    assert_eq!(parsed.get("objective").unwrap(), "x");
}

#[tokio::test]
async fn ws_blob_inactive_when_missing_or_completed() {
    assert_eq!(goal_state_ws_blob(None), json!({"active": false}));
    let empty = json!({});
    assert_eq!(
        goal_state_ws_blob(Some(obj(&empty))),
        json!({"active": false})
    );
    let completed = json!({ "goal_state": {"status": "completed", "objective": "x"} });
    assert_eq!(
        goal_state_ws_blob(Some(obj(&completed))),
        json!({"active": false})
    );
}

#[tokio::test]
async fn ws_blob_active_shape() {
    let meta = json!({
        "goal_state": {"status": "active", "objective": "Build feature.", "ui_summary": "feat"}
    });
    assert_eq!(
        goal_state_ws_blob(Some(obj(&meta))),
        json!({"active": true, "ui_summary": "feat", "objective": "Build feature."})
    );
}

#[tokio::test]
async fn sustained_goal_active_reflects_status_and_legacy_key() {
    assert!(!sustained_goal_active(None));
    let empty = json!({});
    assert!(!sustained_goal_active(Some(obj(&empty))));
    let completed = json!({ "goal_state": {"status": "completed", "objective": "x"} });
    assert!(!sustained_goal_active(Some(obj(&completed))));
    let active = json!({ "goal_state": {"status": "active", "objective": "Run long task."} });
    assert!(sustained_goal_active(Some(obj(&active))));
    let legacy = json!({ "thread_goal": {"status": "active", "objective": "Legacy."} });
    assert!(sustained_goal_active(Some(obj(&legacy))));
}

#[tokio::test]
async fn explicit_goal_requested_reads_command_metadata() {
    let empty = json!({});
    assert!(!explicit_goal_requested(Some(obj(&empty))));
    let requested = json!({ "original_command": "/goal", "goal_requested": true });
    assert!(explicit_goal_requested(Some(obj(&requested))));
}
