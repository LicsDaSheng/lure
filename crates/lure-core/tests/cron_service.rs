//! Cron service 定时执行：tick 处理到期 job、record_run 推进/删除、后台调度。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::channel::RecordingChannel;
use lure_core::cron::{
    CronJob, CronJobRunner, CronJobState, CronPayload, CronSchedule, CronScheduler, CronService,
    CronStore, RunStatus,
};
use lure_core::gateway::{Gateway, GatewayCronRunner};
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;

/// 记录被执行的 job id，返回固定状态。
#[derive(Clone)]
struct RecordingRunner {
    ran: Arc<Mutex<Vec<String>>>,
    status: RunStatus,
}

impl RecordingRunner {
    fn new(status: RunStatus) -> Self {
        Self {
            ran: Arc::new(Mutex::new(Vec::new())),
            status,
        }
    }
    fn ran_ids(&self) -> Vec<String> {
        self.ran.lock().unwrap().clone()
    }
}

impl CronJobRunner for RecordingRunner {
    fn run(&mut self, job: &CronJob) -> RunStatus {
        self.ran.lock().unwrap().push(job.id.clone());
        self.status
    }
}

fn job(id: &str, schedule: CronSchedule, delete_after_run: bool) -> CronJob {
    CronJob {
        id: id.to_string(),
        name: id.to_string(),
        enabled: true,
        schedule,
        payload: CronPayload {
            message: "do it".to_string(),
            origin_channel: Some("cli".to_string()),
            origin_chat_id: Some("local".to_string()),
            ..CronPayload::default()
        },
        state: CronJobState::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
        delete_after_run,
    }
}

#[test]
fn tick_runs_due_recurring_job_and_advances_next_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    // every 500ms，add 在 t=1000 → next_run=1500。
    store
        .add(job("a", CronSchedule::every(500), false), 1000)
        .unwrap();

    let service = CronService::new(dir.path());
    let mut runner = RecordingRunner::new(RunStatus::Ok);
    // tick 在 t=2000（>1500）→ 到期。
    let ran = service.tick(&mut runner, 2000).unwrap();
    assert_eq!(ran, 1);
    assert_eq!(runner.ran_ids(), vec!["a".to_string()]);

    let store = CronStore::load(dir.path()).unwrap();
    let j = &store.jobs()[0];
    assert_eq!(j.state.last_status, Some(RunStatus::Ok));
    // record_run 推进 next_run = 2000 + 500。
    assert_eq!(j.state.next_run_at_ms, Some(2500));
}

#[test]
fn tick_skips_not_due_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("a", CronSchedule::every(5000), false), 1000)
        .unwrap(); // next_run=6000

    let service = CronService::new(dir.path());
    let mut runner = RecordingRunner::new(RunStatus::Ok);
    let ran = service.tick(&mut runner, 2000).unwrap(); // 2000 < 6000
    assert_eq!(ran, 0);
    assert!(runner.ran_ids().is_empty());
}

#[test]
fn tick_deletes_one_shot_job_after_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    // at=1500（add 在 1000 → next_run=1500），一次性。
    store
        .add(job("once", CronSchedule::at(1500), true), 1000)
        .unwrap();

    let service = CronService::new(dir.path());
    let mut runner = RecordingRunner::new(RunStatus::Ok);
    let ran = service.tick(&mut runner, 2000).unwrap();
    assert_eq!(ran, 1);

    // 一次性 job 运行后被删除。
    assert_eq!(CronStore::load(dir.path()).unwrap().jobs().len(), 0);
}

#[test]
fn tick_records_error_status_but_keeps_recurring() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("a", CronSchedule::every(500), false), 1000)
        .unwrap();

    let service = CronService::new(dir.path());
    let mut runner = RecordingRunner::new(RunStatus::Error);
    service.tick(&mut runner, 2000).unwrap();

    let store = CronStore::load(dir.path()).unwrap();
    let j = &store.jobs()[0];
    assert_eq!(j.state.last_status, Some(RunStatus::Error));
    assert_eq!(j.state.next_run_at_ms, Some(2500)); // 仍推进
}

#[test]
fn tick_ignores_disabled_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    let mut j = job("a", CronSchedule::every(500), false);
    j.enabled = false;
    j.state.next_run_at_ms = Some(1500);
    store.add(j, 1000).unwrap();

    let service = CronService::new(dir.path());
    let mut runner = RecordingRunner::new(RunStatus::Ok);
    assert_eq!(service.tick(&mut runner, 2000).unwrap(), 0);
}

#[test]
fn next_wake_returns_earliest_enabled_next_run() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("a", CronSchedule::every(5000), false), 1000)
        .unwrap(); // 6000
    store
        .add(job("b", CronSchedule::every(500), false), 1000)
        .unwrap(); // 1500

    let service = CronService::new(dir.path());
    assert_eq!(service.next_wake_ms().unwrap(), Some(1500));
}

#[test]
fn scheduler_background_runs_due_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = CronStore::load(dir.path()).unwrap();
    // 远久前 next_run，确保任何 now 都到期。
    store
        .add(job("a", CronSchedule::every(1), false), 0)
        .unwrap();

    // runner 计数（跨线程共享）。
    struct CountingRunner(Arc<AtomicUsize>);
    impl CronJobRunner for CountingRunner {
        fn run(&mut self, _job: &CronJob) -> RunStatus {
            self.0.fetch_add(1, Ordering::Relaxed);
            RunStatus::Ok
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let scheduler = CronScheduler::spawn(
        CronService::new(dir.path()),
        CountingRunner(count.clone()),
        Duration::from_millis(20),
    );

    // 等待至少一次 tick。
    let start = std::time::Instant::now();
    while count.load(Ordering::Relaxed) == 0 && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    scheduler.stop();
    assert!(
        count.load(Ordering::Relaxed) >= 1,
        "后台调度应至少执行一次到期 job"
    );
}

#[test]
fn gateway_cron_runner_delivers_due_job_reply_to_channel() {
    // 端到端：到期 cron job → GatewayCronRunner → agent(echo) → 投递回 origin channel。
    let dir = tempfile::tempdir().unwrap();

    // gateway：echo agent + 记录型 channel（保留共享日志用于断言）。
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    let mut gateway = Gateway::new(agent);
    let channel = RecordingChannel::new("cli");
    let log = channel.delivery_log();
    gateway.register_channel(Box::new(channel)).unwrap();
    let mut runner = GatewayCronRunner::new(gateway);

    // 到期 cron job：origin cli:local，消息 "do it"。
    let mut store = CronStore::load(dir.path()).unwrap();
    store
        .add(job("a", CronSchedule::every(1), false), 0)
        .unwrap();

    let service = CronService::new(dir.path());
    assert_eq!(service.tick(&mut runner, 1000).unwrap(), 1);

    let delivered = log.borrow();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].channel, "cli");
    assert_eq!(delivered[0].chat_id, "local");
    assert_eq!(delivered[0].content, "echo: do it");
}
