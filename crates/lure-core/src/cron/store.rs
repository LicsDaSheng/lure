//! Cron job 持久化 store 与运行状态推进。
//!
//! 对齐上游 `nanobot/cron/service.py` 的 store 语义：`workspace/cron/jobs.json`
//! 持久化 job；add 计算 next_run；record_run 推进 next_run 或删除一次性 job；
//! `heartbeat` 为受保护 job，不能删除（区别于普通 reminder）。
//!
//! Phase 8 不做：cron 表达式调度、并发调度线程、run history 完整记录。

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cron::schedule::compute_next_run;
use crate::cron::types::{CronJob, CronJobState, CronStoreData, RunStatus};

/// 受保护的 heartbeat job 名。
pub const HEARTBEAT_JOB_NAME: &str = "heartbeat";

/// 判断一个 job 是否为受保护的 heartbeat（区别于普通 reminder）。
pub fn is_heartbeat(job: &CronJob) -> bool {
    job.name == HEARTBEAT_JOB_NAME
}

/// cron store 错误。
#[derive(Debug)]
pub enum CronError {
    /// 读写 jobs.json 失败。
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// 解析 jobs.json 失败。
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// 尝试删除受保护的 heartbeat job。
    Protected(String),
}

impl fmt::Display for CronError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CronError::Io { path, source } => {
                write!(f, "cron 存储 IO 失败 {}: {source}", path.display())
            }
            CronError::Parse { path, source } => {
                write!(f, "cron 存储解析失败 {}: {source}", path.display())
            }
            CronError::Protected(name) => {
                write!(f, "受保护的 job '{name}' 不能删除")
            }
        }
    }
}

impl std::error::Error for CronError {}

/// cron job 持久化 store。
pub struct CronStore {
    path: PathBuf,
    data: CronStoreData,
}

impl CronStore {
    /// 从 `workspace/cron/jobs.json` 加载（缺失则空）。
    pub fn load(workspace: impl AsRef<Path>) -> Result<Self, CronError> {
        let cron_dir = workspace.as_ref().join("cron");
        let path = cron_dir.join("jobs.json");
        let data = if path.exists() {
            let text = fs::read_to_string(&path).map_err(|source| CronError::Io {
                path: path.clone(),
                source,
            })?;
            serde_json::from_str(&text).map_err(|source| CronError::Parse {
                path: path.clone(),
                source,
            })?
        } else {
            CronStoreData::default()
        };
        Ok(Self { path, data })
    }

    /// 持久化到 jobs.json（camelCase，缩进 2）。
    pub fn save(&self) -> Result<(), CronError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| CronError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(&self.data).expect("cron store 可序列化");
        fs::write(&self.path, json).map_err(|source| CronError::Io {
            path: self.path.clone(),
            source,
        })
    }

    /// 全部 job。
    pub fn jobs(&self) -> &[CronJob] {
        &self.data.jobs
    }

    /// 按 id 获取 job。
    pub fn get(&self, id: &str) -> Option<&CronJob> {
        self.data.jobs.iter().find(|j| j.id == id)
    }

    /// 添加 job：计算 next_run 并持久化。
    pub fn add(&mut self, mut job: CronJob, now_ms: i64) -> Result<(), CronError> {
        job.created_at_ms = now_ms;
        job.updated_at_ms = now_ms;
        job.state = CronJobState {
            next_run_at_ms: compute_next_run(&job.schedule, now_ms),
            ..CronJobState::default()
        };
        self.data.jobs.push(job);
        self.save()
    }

    /// 删除 job；受保护的 heartbeat 返回 [`CronError::Protected`]。返回是否删除。
    pub fn remove(&mut self, id: &str) -> Result<bool, CronError> {
        if let Some(job) = self.get(id) {
            if is_heartbeat(job) {
                return Err(CronError::Protected(job.name.clone()));
            }
        }
        let before = self.data.jobs.len();
        self.data.jobs.retain(|j| j.id != id);
        let removed = self.data.jobs.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// 到期的 job（启用、有 next_run 且已到时）。
    pub fn due_jobs(&self, now_ms: i64) -> Vec<&CronJob> {
        self.data
            .jobs
            .iter()
            .filter(|j| j.enabled && j.state.next_run_at_ms.is_some_and(|next| now_ms >= next))
            .collect()
    }

    /// 记录一次运行：更新状态并推进 next_run；一次性 job 运行后删除。
    pub fn record_run(
        &mut self,
        id: &str,
        status: RunStatus,
        now_ms: i64,
    ) -> Result<(), CronError> {
        let Some(idx) = self.data.jobs.iter().position(|j| j.id == id) else {
            return Ok(());
        };
        let delete_after = self.data.jobs[idx].delete_after_run;
        {
            let job = &mut self.data.jobs[idx];
            job.state.last_run_at_ms = Some(now_ms);
            job.state.last_status = Some(status);
            job.updated_at_ms = now_ms;
            if !delete_after {
                job.state.next_run_at_ms = compute_next_run(&job.schedule, now_ms);
            }
        }
        if delete_after {
            self.data.jobs.remove(idx);
        }
        self.save()
    }
}
