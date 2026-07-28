//! `cron` 工具：add/list/remove 行为与 origin 绑定。
//!
//! 对齐上游 `nanobot/agent/tools/cron.py`：单一 action 工具，add 绑定发起会话 origin。

use lure_core::cron::{CronStore, ScheduleKind};
use lure_core::tool::{CronTool, CronToolOrigin, Tool};
use serde_json::json;
use tempfile::TempDir;

fn tool(dir: &TempDir) -> CronTool {
    CronTool::new(
        dir.path().to_path_buf(),
        CronToolOrigin {
            session_key: "cli:local".to_string(),
            channel: "cli".to_string(),
            chat_id: "local".to_string(),
        },
        "UTC",
    )
}

#[test]
fn add_cron_job_persists_with_origin() {
    let dir = tempfile::tempdir().unwrap();
    let t = tool(&dir);
    let res = t.execute(&json!({
        "action": "add",
        "message": "每天 9 点提醒喝水",
        "cron_expr": "0 9 * * *"
    }));
    assert!(!res.is_error, "add 应成功: {}", res.content);
    assert!(res.content.contains("已创建任务"));

    let store = CronStore::load(dir.path()).unwrap();
    assert_eq!(store.jobs().len(), 1);
    let job = &store.jobs()[0];
    assert_eq!(job.schedule.kind, ScheduleKind::Cron);
    assert_eq!(job.schedule.expr.as_deref(), Some("0 9 * * *"));
    assert_eq!(job.payload.message, "每天 9 点提醒喝水");
    assert_eq!(job.payload.session_key.as_deref(), Some("cli:local"));
    assert_eq!(job.payload.origin_channel.as_deref(), Some("cli"));
    assert_eq!(job.payload.origin_chat_id.as_deref(), Some("local"));
    // cron 已能算出 next_run。
    assert!(job.state.next_run_at_ms.is_some());
}

#[test]
fn add_requires_message() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({"action": "add", "cron_expr": "0 9 * * *"}));
    assert!(res.is_error);
    assert!(res.content.contains("message"));
}

#[test]
fn add_every_seconds_builds_every_schedule() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({
        "action": "add", "message": "心跳", "every_seconds": 300
    }));
    assert!(!res.is_error);
    let store = CronStore::load(dir.path()).unwrap();
    let job = &store.jobs()[0];
    assert_eq!(job.schedule.kind, ScheduleKind::Every);
    assert_eq!(job.schedule.every_ms, Some(300_000));
}

#[test]
fn add_at_iso_builds_one_shot_and_deletes_after() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({
        "action": "add", "message": "一次性提醒", "at": "2030-01-01T00:00:00"
    }));
    assert!(!res.is_error, "{}", res.content);
    let store = CronStore::load(dir.path()).unwrap();
    let job = &store.jobs()[0];
    assert_eq!(job.schedule.kind, ScheduleKind::At);
    // 2030-01-01T00:00:00Z = 1893456000000 ms。
    assert_eq!(job.schedule.at_ms, Some(1_893_456_000_000));
    assert!(job.delete_after_run);
}

#[test]
fn add_invalid_at_errors() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({"action": "add", "message": "x", "at": "not-a-date"}));
    assert!(res.is_error);
    assert!(res.content.contains("ISO"));
}

#[test]
fn add_tz_without_cron_expr_errors() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({
        "action": "add", "message": "x", "every_seconds": 60, "tz": "UTC"
    }));
    assert!(res.is_error);
    assert!(res.content.contains("tz"));
}

#[test]
fn add_unsupported_tz_errors() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({
        "action": "add", "message": "x", "cron_expr": "0 9 * * *", "tz": "America/New_York"
    }));
    assert!(res.is_error);
    assert!(res.content.contains("时区"));
}

#[test]
fn add_without_schedule_errors() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({"action": "add", "message": "x"}));
    assert!(res.is_error);
}

#[test]
fn list_shows_jobs_and_remove_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let t = tool(&dir);
    t.execute(&json!({"action": "add", "message": "任务甲", "every_seconds": 60}));

    let listed = t.execute(&json!({"action": "list"}));
    assert!(!listed.is_error);
    assert!(listed.content.contains("任务甲"));
    assert!(listed.content.contains("every 1m"));

    // 从 list 拿 id。
    let store = CronStore::load(dir.path()).unwrap();
    let id = store.jobs()[0].id.clone();

    let removed = t.execute(&json!({"action": "remove", "job_id": id}));
    assert!(!removed.is_error, "{}", removed.content);
    assert_eq!(CronStore::load(dir.path()).unwrap().jobs().len(), 0);
}

#[test]
fn remove_requires_job_id_and_unknown_errors() {
    let dir = tempfile::tempdir().unwrap();
    let t = tool(&dir);
    assert!(t.execute(&json!({"action": "remove"})).is_error);
    let r = t.execute(&json!({"action": "remove", "job_id": "does-not-exist"}));
    assert!(r.is_error);
    assert!(r.content.contains("未找到"));
}

#[test]
fn list_empty_reports_none() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({"action": "list"}));
    assert!(!res.is_error);
    assert!(res.content.contains("暂无"));
}

#[test]
fn unknown_action_errors() {
    let dir = tempfile::tempdir().unwrap();
    let res = tool(&dir).execute(&json!({"action": "frobnicate"}));
    assert!(res.is_error);
}
