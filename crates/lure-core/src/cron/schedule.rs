//! next-run 计算。
//!
//! 对齐上游 `nanobot/cron/service.py::_compute_next_run`：
//! - `at`：`at_ms` 在未来则返回，否则 `None`（已过期不再触发）。
//! - `every`：`now_ms + every_ms`（`every_ms` 需为正）。
//! - `cron`：解析 5 字段表达式，从 `now` 起严格下一次匹配的墙钟分钟（见 [`crate::cron::expr`]）。
//!   tz：`None`→本地时区、`UTC`/`Z`→UTC；其它 IANA 名暂不支持返回 `None`（无 chrono-tz，
//!   见 upstream-test-ledger）。任何解析错误亦回落 `None`（对齐上游 except→None）。

use chrono::{LocalResult, TimeZone};

use crate::cron::expr::CronExpr;
use crate::cron::types::{CronSchedule, ScheduleKind};

/// 每分钟一步；上限约 4 年，覆盖 `0 0 29 2 *`（闰年 2-29）这类稀疏表达式。
const MINUTE_MS: i64 = 60_000;
const MAX_STEPS: i64 = 366 * 4 * 24 * 60;

/// 计算下次运行时间（ms）。
pub fn compute_next_run(schedule: &CronSchedule, now_ms: i64) -> Option<i64> {
    match schedule.kind {
        ScheduleKind::At => schedule.at_ms.filter(|&at| at > now_ms),
        ScheduleKind::Every => match schedule.every_ms {
            Some(every) if every > 0 => Some(now_ms + every),
            _ => None,
        },
        ScheduleKind::Cron => compute_cron_next_run(schedule, now_ms),
    }
}

fn compute_cron_next_run(schedule: &CronSchedule, now_ms: i64) -> Option<i64> {
    let expr = CronExpr::parse(schedule.expr.as_deref()?)?;
    match schedule.tz.as_deref() {
        None => next_after(&expr, &chrono::Local, now_ms),
        Some(tz) if tz.eq_ignore_ascii_case("utc") || tz == "Z" => {
            next_after(&expr, &chrono::Utc, now_ms)
        }
        // 其它 IANA 名需 chrono-tz，暂不支持。
        Some(_) => None,
    }
}

/// 从 `now_ms` 起，在给定时区里逐分钟找严格之后第一个匹配墙钟的瞬间。
///
/// 按真实分钟（UTC 纪元）步进、每步换算到 tz 墙钟做匹配：DST 跳变时不存在的墙钟分钟
/// 自然跳过，重复分钟取较早者。所有时区偏移均为整分钟，故 UTC 分钟边界即墙钟分钟边界。
fn next_after<Tz: TimeZone>(expr: &CronExpr, tz: &Tz, now_ms: i64) -> Option<i64> {
    let start = now_ms - now_ms.rem_euclid(MINUTE_MS) + MINUTE_MS; // 严格 > now 的下一分钟
    for i in 0..MAX_STEPS {
        let t = start + i * MINUTE_MS;
        let dt = match tz.timestamp_millis_opt(t) {
            LocalResult::Single(d) => d,
            LocalResult::Ambiguous(d, _) => d,
            LocalResult::None => continue,
        };
        if expr.matches(&dt) {
            return Some(t);
        }
    }
    None
}
