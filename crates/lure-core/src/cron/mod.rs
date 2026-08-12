//! Cron 子系统：调度类型、next-run 计算、持久化 store 与 session-bound 投递。
//!
//! Phase 8 覆盖 at/every/cron 调度、jobs.json 持久化、heartbeat 受保护 job、session-bound
//! delivery 与**定时执行**（[`CronService::tick`] 跑到期 job + `record_run` 推进/删一次性、
//! [`CronScheduler`] 后台轮询、[`CronJobRunner`] 执行 seam）。cron 表达式支持标准 5 字段
//! （`* , - /`、dow 0-7）与 UTC/本地时区；IANA 具名时区、cron 宏/扩展、run history 留待后续。

mod delivery;
mod expr;
mod schedule;
mod service;
mod store;
mod types;

pub use delivery::{cron_submit_message, origin_delivery_context, MissingOriginError};
pub use schedule::compute_next_run;
pub use service::{AsyncCronScheduler, CronJobRunner, CronService};
pub use store::{is_heartbeat, CronError, CronStore, HEARTBEAT_JOB_NAME};
pub use types::{
    CronJob, CronJobState, CronPayload, CronSchedule, CronStoreData, RunStatus, ScheduleKind,
};
