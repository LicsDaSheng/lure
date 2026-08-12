//! Cron service：到期 job 的定时执行引擎。
//!
//! 对齐上游 `nanobot/cron/service.py` 的 timer 循环：每次 tick 加载 store，找出到期 job，
//! 逐个经 [`CronJobRunner`] 执行并 [`CronStore::record_run`] 记录状态（自动推进 next_run /
//! 删除一次性 job）。执行本体（跑 agent turn + 投递回原会话）由 runner 抽象，保持引擎
//! 与 agent/gateway 解耦、可测。后台调度线程见 [`CronScheduler`]。

use std::path::PathBuf;
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

    /// 当前到期的 job（启用、有 next_run 且已到时），返回独立克隆。
    ///
    /// Stage 4：异步调度器用它把「找到期 job」与「submit 执行」解耦——submit 后
    /// 状态推进（`record_run`）由执行完成回调负责，不在本方法内发生。
    pub fn due_jobs(&self, now_ms: i64) -> Result<Vec<CronJob>, CronError> {
        let store = CronStore::load(&self.workspace)?;
        Ok(store
            .due_jobs(now_ms)
            .iter()
            .map(|j| (*j).clone())
            .collect())
    }
}

/// 当前 epoch ms。
fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// 异步 cron 调度器（Stage 4）：tokio runtime 内 interval task，替代后台轮询线程。
///
/// 每次 tick 用 [`CronService::due_jobs`] 取到期 job，逐个交给 `submit` 闭包投递
/// （submit 语义：投递即返回，执行与完成由下游异步处理）。job 的状态推进
/// （`record_run`）不在调度器内发生——由执行完成回调负责（desktop 经
/// `AgentLoopScheduler::on_completed` 投递回 transcript + hub + record_run）。
///
/// 停止经 oneshot 信号；`stop().await` 等待 task 退出，`Drop` 兜底 abort。
pub struct AsyncCronScheduler {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl AsyncCronScheduler {
    /// 启动 interval task：每 `poll` 周期对到期 job 执行 `submit`（在给定 runtime 内）。
    ///
    /// 显式接收 `handle`（而非 `tokio::spawn`）以支持在 runtime 上下文之外（如 desktop
    /// `main` 的装配段）启动。
    pub fn spawn<F>(
        handle: &tokio::runtime::Handle,
        service: CronService,
        mut submit: F,
        poll: Duration,
    ) -> Self
    where
        F: FnMut(&CronJob) + Send + 'static,
    {
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
        let handle = handle.spawn(async move {
            let mut interval = tokio::time::interval(poll);
            // 掉过 tick 不追赶：慢处理时按最新时刻对齐，避免连发补偿。
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    _ = interval.tick() => {
                        let Ok(due) = service.due_jobs(now_ms()) else {
                            continue;
                        };
                        for job in &due {
                            submit(job);
                        }
                    }
                }
            }
        });
        Self {
            stop: Some(stop_tx),
            handle: Some(handle),
        }
    }

    /// 发送停止信号并等待 interval task 退出。
    pub async fn stop(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for AsyncCronScheduler {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}
