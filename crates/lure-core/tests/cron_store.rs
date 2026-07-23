//! 映射上游 `tests/cron/test_cron_persistence.py` 与 next-run/heartbeat 场景。

use lure_core::cron::{
    compute_next_run, is_heartbeat, CronError, CronJob, CronPayload, CronSchedule, CronStore,
    RunStatus, ScheduleKind,
};
use tempfile::tempdir;

fn job(id: &str, name: &str, schedule: CronSchedule) -> CronJob {
    CronJob {
        id: id.to_string(),
        name: name.to_string(),
        enabled: true,
        schedule,
        payload: CronPayload::default(),
        state: Default::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
        delete_after_run: false,
    }
}

#[test]
fn compute_next_run_for_at_and_every() {
    // at：未来才触发，过期返回 None。
    assert_eq!(compute_next_run(&CronSchedule::at(2000), 1000), Some(2000));
    assert_eq!(compute_next_run(&CronSchedule::at(500), 1000), None);
    // every：now + 间隔。
    assert_eq!(
        compute_next_run(&CronSchedule::every(300), 1000),
        Some(1300)
    );
    assert_eq!(compute_next_run(&CronSchedule::every(0), 1000), None);
    // cron：暂返回 None。
    let cron = CronSchedule {
        kind: ScheduleKind::Cron,
        at_ms: None,
        every_ms: None,
        expr: Some("0 9 * * *".to_string()),
        tz: None,
    };
    assert_eq!(compute_next_run(&cron, 1000), None);
}

#[test]
fn add_computes_next_run_and_persists() {
    let dir = tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("j1", "reminder", CronSchedule::every(1000)), 5000)
        .unwrap();

    let saved = store.get("j1").unwrap();
    assert_eq!(saved.state.next_run_at_ms, Some(6000));
    assert_eq!(saved.created_at_ms, 5000);

    // 冷启动重载，job 仍在且状态保留。
    let reloaded = CronStore::load(dir.path()).unwrap();
    assert_eq!(reloaded.jobs().len(), 1);
    assert_eq!(reloaded.get("j1").unwrap().state.next_run_at_ms, Some(6000));
}

#[test]
fn store_serializes_camel_case() {
    let dir = tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("j1", "reminder", CronSchedule::every(1000)), 5000)
        .unwrap();

    let text = std::fs::read_to_string(dir.path().join("cron").join("jobs.json")).unwrap();
    assert!(text.contains("\"everyMs\""));
    assert!(text.contains("\"nextRunAtMs\""));
    assert!(!text.contains("every_ms"));
}

#[test]
fn due_jobs_respects_next_run_and_enabled() {
    let dir = tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("j1", "reminder", CronSchedule::every(1000)), 5000)
        .unwrap(); // next_run 6000

    assert!(store.due_jobs(5999).is_empty());
    assert_eq!(store.due_jobs(6000).len(), 1);
    assert_eq!(store.due_jobs(9999).len(), 1);
}

#[test]
fn record_run_advances_next_run() {
    let dir = tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("j1", "reminder", CronSchedule::every(1000)), 5000)
        .unwrap();

    store.record_run("j1", RunStatus::Ok, 6000).unwrap();
    let updated = store.get("j1").unwrap();
    assert_eq!(updated.state.last_run_at_ms, Some(6000));
    assert_eq!(updated.state.last_status, Some(RunStatus::Ok));
    assert_eq!(updated.state.next_run_at_ms, Some(7000));
}

#[test]
fn delete_after_run_removes_one_shot_job() {
    let dir = tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    let mut one_shot = job("once", "reminder", CronSchedule::at(9000));
    one_shot.delete_after_run = true;
    store.add(one_shot, 5000).unwrap();

    store.record_run("once", RunStatus::Ok, 9000).unwrap();
    assert!(store.get("once").is_none());
}

#[test]
fn heartbeat_is_protected_from_deletion() {
    let dir = tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("hb", "heartbeat", CronSchedule::every(1000)), 5000)
        .unwrap();
    store
        .add(job("rm", "reminder", CronSchedule::every(1000)), 5000)
        .unwrap();

    // heartbeat 与普通 reminder 区分：heartbeat 受保护。
    assert!(is_heartbeat(store.get("hb").unwrap()));
    assert!(!is_heartbeat(store.get("rm").unwrap()));

    let err = store.remove("hb").unwrap_err();
    assert!(matches!(err, CronError::Protected(_)));
    assert!(store.get("hb").is_some());

    // 普通 reminder 可删除。
    assert!(store.remove("rm").unwrap());
    assert!(store.get("rm").is_none());
}
