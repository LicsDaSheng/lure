//! next-run 计算。
//!
//! 对齐上游 `nanobot/cron/service.py::_compute_next_run`：
//! - `at`：`at_ms` 在未来则返回，否则 `None`（已过期不再触发）。
//! - `every`：`now_ms + every_ms`（`every_ms` 需为正）。
//! - `cron`：需 cron 表达式解析（croniter），Phase 8 暂返回 `None`（见 upstream-test-ledger）。

use crate::cron::types::{CronSchedule, ScheduleKind};

/// 计算下次运行时间（ms）。
pub fn compute_next_run(schedule: &CronSchedule, now_ms: i64) -> Option<i64> {
    match schedule.kind {
        ScheduleKind::At => schedule.at_ms.filter(|&at| at > now_ms),
        ScheduleKind::Every => match schedule.every_ms {
            Some(every) if every > 0 => Some(now_ms + every),
            _ => None,
        },
        ScheduleKind::Cron => None,
    }
}
