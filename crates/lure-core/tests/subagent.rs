//! Subagent 子系统集成测试。
//!
//! 对照上游 `nanobot/agent/subagent.py` + `tests/agent/test_subagent_lifecycle.py`：
//! `SubagentStatus` 状态模型、label 派生、`_format_partial_progress` 纯格式化、以及
//! session→task 登记表（`get_running_count`/`get_running_count_by_session`/`cancel_by_session`）
//! 的簿记语义（含 /stop 级联终止的取消原语）。
//!
//! 暂缓：真实后台执行（`spawn`/`_run_subagent`/`_announce_result`）依赖 asyncio 后台任务 +
//! 完整 agent 运行时 + exec session 管理，lure 为同步模型，留待引入异步运行时后回补。

use lure_core::agent::subagent::{
    derive_label, format_partial_progress, SubagentPhase, SubagentRegistry, SubagentRunResult,
    SubagentStatus, ToolEvent,
};

// —— SubagentStatus / label —— //

#[test]
fn status_defaults_to_initializing() {
    let s = SubagentStatus::new("t1", "label", "desc", 0.0);
    assert_eq!(s.phase, SubagentPhase::Initializing);
    assert_eq!(s.iteration, 0);
    assert!(s.tool_events.is_empty());
    assert!(s.stop_reason.is_none());
    assert!(s.error.is_none());
}

#[test]
fn label_defaults_to_truncated_task() {
    // 短任务：原样。
    assert_eq!(derive_label("do a thing", None), "do a thing");
    // 长任务（>30 字符）：前 30 + "..."。
    let long = "abcdefghijklmnopqrstuvwxyz0123456789"; // 36 chars
    assert_eq!(derive_label(long, None), format!("{}...", &long[..30]));
}

#[test]
fn label_custom_overrides() {
    assert_eq!(
        derive_label("some long task text here", Some("My Label")),
        "My Label"
    );
}

// —— format_partial_progress —— //

fn ev(name: &str, status: &str, detail: &str) -> ToolEvent {
    ToolEvent {
        name: name.to_string(),
        status: status.to_string(),
        detail: detail.to_string(),
    }
}

#[test]
fn partial_progress_completed_only() {
    let r = SubagentRunResult {
        tool_events: vec![
            ev("read_file", "ok", "file content"),
            ev("exec", "ok", "output"),
        ],
        error: None,
    };
    let text = format_partial_progress(&r);
    assert!(text.contains("Completed steps:"));
    assert!(text.contains("read_file"));
    assert!(text.contains("exec"));
}

#[test]
fn partial_progress_failure_only() {
    let r = SubagentRunResult {
        tool_events: vec![ev("read_file", "error", "not found")],
        error: None,
    };
    let text = format_partial_progress(&r);
    assert!(text.contains("Failure:"));
    assert!(text.contains("not found"));
}

#[test]
fn partial_progress_completed_and_failure() {
    let r = SubagentRunResult {
        tool_events: vec![
            ev("read_file", "ok", "content"),
            ev("exec", "error", "timeout"),
        ],
        error: None,
    };
    let text = format_partial_progress(&r);
    assert!(text.contains("Completed steps:"));
    assert!(text.contains("Failure:"));
}

#[test]
fn partial_progress_limited_to_last_three() {
    let r = SubagentRunResult {
        tool_events: (0..5)
            .map(|i| ev(&format!("tool_{i}"), "ok", &format!("result_{i}")))
            .collect(),
        error: None,
    };
    let text = format_partial_progress(&r);
    assert!(text.contains("tool_2"));
    assert!(text.contains("tool_3"));
    assert!(text.contains("tool_4"));
    assert!(!text.contains("tool_0"));
    assert!(!text.contains("tool_1"));
}

#[test]
fn partial_progress_error_without_failure_event() {
    let r = SubagentRunResult {
        tool_events: vec![ev("read_file", "ok", "ok")],
        error: Some("Something went wrong".to_string()),
    };
    assert!(format_partial_progress(&r).contains("Something went wrong"));
}

#[test]
fn partial_progress_empty_events_with_error() {
    let r = SubagentRunResult {
        tool_events: vec![],
        error: Some("Total failure".to_string()),
    };
    assert!(format_partial_progress(&r).contains("Total failure"));
}

#[test]
fn partial_progress_empty_no_error_returns_fallback() {
    let r = SubagentRunResult {
        tool_events: vec![],
        error: None,
    };
    assert!(format_partial_progress(&r).contains("Error"));
}

// —— SubagentRegistry：running counts / cancel —— //

fn status(id: &str) -> SubagentStatus {
    SubagentStatus::new(id, id, id, 0.0)
}

#[test]
fn running_count_zero_initially() {
    let r = SubagentRegistry::new();
    assert_eq!(r.get_running_count(), 0);
}

#[test]
fn register_tracks_count_and_session() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.register("t2", Some("s1"), status("t2"));
    assert_eq!(r.get_running_count(), 2);
    assert_eq!(r.get_running_count_by_session("s1"), 2);
}

#[test]
fn finish_removes_task() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.finish("t1");
    assert_eq!(r.get_running_count(), 0);
    assert_eq!(r.get_running_count_by_session("s1"), 0);
}

#[test]
fn by_session_nonexistent_is_zero() {
    let r = SubagentRegistry::new();
    assert_eq!(r.get_running_count_by_session("nonexistent"), 0);
}

#[test]
fn no_session_key_not_registered_in_session_index() {
    let mut r = SubagentRegistry::new();
    r.register("t1", None, status("t1"));
    // 计入总数，但不计入任何 session。
    assert_eq!(r.get_running_count(), 1);
    assert_eq!(r.get_running_count_by_session("s1"), 0);
}

#[test]
fn mark_done_excludes_from_session_count() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.mark_done("t1");
    // 仍在 _tasks（未清理）故总数计入；但 done 不计入 session 运行数。
    assert_eq!(r.get_running_count(), 1);
    assert_eq!(r.get_running_count_by_session("s1"), 0);
}

#[test]
fn cancel_by_session_cancels_running() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.register("t2", Some("s1"), status("t2"));
    assert_eq!(r.cancel_by_session("s1"), 2);
    // 取消后移除，运行数归零。
    assert_eq!(r.get_running_count(), 0);
    assert_eq!(r.get_running_count_by_session("s1"), 0);
}

#[test]
fn cancel_by_session_no_tasks_returns_zero() {
    let mut r = SubagentRegistry::new();
    assert_eq!(r.cancel_by_session("nonexistent"), 0);
}

#[test]
fn cancel_by_session_already_finished_returns_zero() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.finish("t1"); // 模拟 drain 后 cleanup 回调
    assert_eq!(r.cancel_by_session("s1"), 0);
}

#[test]
fn cancel_by_session_skips_done_tasks() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.register("t2", Some("s1"), status("t2"));
    r.mark_done("t1");
    // 仅取消未完成的 t2。
    assert_eq!(r.cancel_by_session("s1"), 1);
}

#[test]
fn cancel_isolated_by_session() {
    let mut r = SubagentRegistry::new();
    r.register("t1", Some("s1"), status("t1"));
    r.register("t2", Some("s2"), status("t2"));
    assert_eq!(r.cancel_by_session("s1"), 1);
    // s2 不受影响。
    assert_eq!(r.get_running_count_by_session("s2"), 1);
}
