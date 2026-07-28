//! cron 表达式 next-run 计算。
//!
//! 对齐上游 `nanobot/cron/service.py::_compute_next_run` 的 cron 分支（croniter+zoneinfo）：
//! 从 `now` 起严格下一次匹配的墙钟分钟。测试统一用 UTC 保证确定性；tz=None（本地）
//! 与非 UTC IANA 名的行为见 upstream-test-ledger。

use lure_core::cron::{compute_next_run, CronSchedule};

/// 2026-01-01T00:00:00Z（周四）。
const NOW: i64 = 1_767_225_600_000;

fn cron_utc(expr: &str) -> CronSchedule {
    CronSchedule::cron(expr, Some("UTC".to_string()))
}

#[test]
fn every_15_minutes_returns_next_quarter() {
    // now 恰在 00:00:00 → 严格之后的下一个 */15 是 00:15。
    assert_eq!(
        compute_next_run(&cron_utc("*/15 * * * *"), NOW),
        Some(NOW + 15 * 60 * 1000)
    );
}

#[test]
fn daily_at_nine_returns_same_day_nine() {
    assert_eq!(
        compute_next_run(&cron_utc("0 9 * * *"), NOW),
        Some(NOW + 9 * 3600 * 1000)
    );
}

#[test]
fn daily_midnight_strictly_after_now_is_next_day() {
    assert_eq!(
        compute_next_run(&cron_utc("0 0 * * *"), NOW),
        Some(NOW + 86_400 * 1000)
    );
}

#[test]
fn weekday_monday_from_thursday() {
    // 2026-01-01 周四 → 下个周一是 2026-01-05（+4 天）。
    assert_eq!(
        compute_next_run(&cron_utc("0 0 * * 1"), NOW),
        Some(NOW + 4 * 86_400 * 1000)
    );
}

#[test]
fn dow_seven_is_sunday_same_as_zero() {
    // 周日 = 2026-01-04（+3 天）；7 与 0 等价。
    let expected = Some(NOW + 3 * 86_400 * 1000);
    assert_eq!(compute_next_run(&cron_utc("0 0 * * 7"), NOW), expected);
    assert_eq!(compute_next_run(&cron_utc("0 0 * * 0"), NOW), expected);
}

#[test]
fn dom_and_dow_both_restricted_is_or() {
    // "1 号 或 周一" 00:00：周一(01-05) 早于 下个 1 号(02-01) → 取 01-05。
    assert_eq!(
        compute_next_run(&cron_utc("0 0 1 * 1"), NOW),
        Some(NOW + 4 * 86_400 * 1000)
    );
}

#[test]
fn range_and_list_fields() {
    // 分钟 30，小时取 9,17 → 严格之后第一个是 09:30。
    assert_eq!(
        compute_next_run(&cron_utc("30 9,17 * * *"), NOW),
        Some(NOW + (9 * 3600 + 30 * 60) * 1000)
    );
}

#[test]
fn yearly_feb_29_within_four_years() {
    // "0 0 29 2 *"：2026 非闰年 → 下个 2-29 是 2028-02-29。函数应在 4 年上限内命中。
    let result = compute_next_run(&cron_utc("0 0 29 2 *"), NOW);
    assert!(result.is_some(), "4 年内应命中闰年 2-29");
}

#[test]
fn invalid_expr_returns_none() {
    assert_eq!(compute_next_run(&cron_utc("not a cron"), NOW), None);
    assert_eq!(compute_next_run(&cron_utc("* * * *"), NOW), None); // 字段数不足
    assert_eq!(compute_next_run(&cron_utc("60 * * * *"), NOW), None); // 分钟越界
    assert_eq!(compute_next_run(&cron_utc("0 24 * * *"), NOW), None); // 小时越界
}

#[test]
fn unsupported_named_timezone_returns_none() {
    // 非 UTC IANA 名暂不支持（无 chrono-tz）→ None（对齐上游异常回落）。
    let sched = CronSchedule::cron("0 9 * * *", Some("America/New_York".to_string()));
    assert_eq!(compute_next_run(&sched, NOW), None);
}

#[test]
fn none_timezone_local_returns_some_for_valid_expr() {
    // tz=None → 本地时区（值依赖机器，故只断言可算出）。
    let sched = CronSchedule::cron("*/5 * * * *", None);
    assert!(compute_next_run(&sched, NOW).is_some());
}
