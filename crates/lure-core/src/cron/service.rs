//! Cron service：到期 job 的定时执行引擎。
//!
//! 对齐上游 `nanobot/cron/service.py` 的 timer 循环：每次 tick 加载 store，找出到期 job，
//! 逐个经 [`CronJobRunner`] 执行并 [`CronStore::record_run`] 记录状态（自动推进 next_run /
//! 删除一次性 job）。执行本体（跑 agent turn + 投递回原会话）由 runner 抽象，保持引擎
//! 与 agent/gateway 解耦、可测。后台调度线程见 [`CronScheduler`]。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::Utc;

use crate::cron::store::{CronError, CronStore};
use crate::cron::types::{CronJob, RunStatus};

/// 到期 job 的执行回调：跑 job 的 message 并返回运行状态。
///
/// 对齐上游 `on_job`：实现方负责把 job 的指令作为 agent turn 执行并投递回其 origin 会话。
pub trait CronJobRunner {
    /// 执行一个到期 job，返回运行状态（`Ok`/`Error`/`Skipped`）。
    fn run(&mut self, job: &CronJob) -> RunStatus;
}

/// cron 定时执行引擎（无状态，按 workspace 路径 load/save store）。
pub struct CronService {
    workspace: PathBuf,
}

impl CronService {
    /// 绑定 workspace（store 落在 `workspace/cron/jobs.json`）。
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }

    /// 执行一次 tick：跑所有到期 job 并记录，返回本次执行的 job 数。
    ///
    /// 顺序：加载 store → 收集到期 id → 逐个 `runner.run` → `record_run`（推进 next_run /
    /// 删一次性）。runner 与 store 变更分离，避免借用冲突。
    pub fn tick(&self, runner: &mut dyn CronJobRunner, now_ms: i64) -> Result<usize, CronError> {
        let mut store = CronStore::load(&self.workspace)?;
        let due_ids: Vec<String> = store
            .due_jobs(now_ms)
            .iter()
            .map(|j| j.id.clone())
            .collect();
        for id in &due_ids {
            let Some(job) = store.get(id).cloned() else {
                continue;
            };
            let status = runner.run(&job);
            store.record_run(id, status, now_ms)?;
        }
        Ok(due_ids.len())
    }

    /// 最早的下次唤醒时间（所有启用且有 next_run 的 job 里最小的 next_run_at_ms）。
    /// 无可调度 job 返回 `None`。
    pub fn next_wake_ms(&self) -> Result<Option<i64>, CronError> {
        let store = CronStore::load(&self.workspace)?;
        Ok(store
            .jobs()
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.state.next_run_at_ms)
            .min())
    }
}

/// 当前 epoch ms。
fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// 后台轮询调度器：固定间隔 tick，直到停止。
///
/// 采用轮询（而非精确睡到 next_wake）以求简单稳健；间隔内到期的 job 会在下个 tick 执行。
pub struct CronScheduler {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl CronScheduler {
    /// 启动后台线程，每 `poll` 间隔对 `service` 执行一次 tick（用 `runner` 跑到期 job）。
    ///
    /// runner 由 `make_runner` 在**后台线程内**构建：runner 及其持有的 agent/gateway 等
    /// 常常非 `Send`（含 `Rc`、trait 对象），线程内构建可避免跨线程移动约束——只要 factory
    /// 本身 `Send`（通常只捕获 config 路径等可 Send 值）。
    pub fn spawn<R, F>(service: CronService, make_runner: F, poll: Duration) -> Self
    where
        F: FnOnce() -> R + Send + 'static,
        R: CronJobRunner,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let handle = thread::spawn(move || {
            let mut runner = make_runner();
            while !stop_thread.load(Ordering::Relaxed) {
                let _ = service.tick(&mut runner, now_ms());
                // 分片睡眠：及时响应 stop，不必等满一个 poll 周期。
                let mut slept = Duration::ZERO;
                let slice = Duration::from_millis(50);
                while slept < poll && !stop_thread.load(Ordering::Relaxed) {
                    thread::sleep(slice.min(poll - slept));
                    slept += slice;
                }
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// 停止后台线程并等待其结束。
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CronScheduler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
