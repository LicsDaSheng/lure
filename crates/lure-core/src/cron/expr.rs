//! 标准 5 字段 cron 表达式解析与匹配。
//!
//! 对齐上游依赖的 `croniter` 常用语义：字段 `分 时 日 月 周`，支持 `* , - /`；
//! 周（dow）0-7，0 与 7 均为周日；日(dom)与周(dow)同时受限时按 **OR**（Vixie cron 语义）。
//! 未覆盖：名称（JAN/MON）、`L`/`#`/`W` 扩展、`@daily` 宏、秒字段——见 upstream-test-ledger。

use chrono::{Datelike, Timelike};

/// 解析后的 cron 表达式：每字段一个位掩码 + 是否受限（非 `*`）。
pub struct CronExpr {
    minutes: u64, // bit 0..=59
    hours: u32,   // bit 0..=23
    doms: u32,    // bit 1..=31
    months: u16,  // bit 1..=12
    dows: u8,     // bit 0..=6（周日=0）
    dom_restricted: bool,
    dow_restricted: bool,
}

impl CronExpr {
    /// 解析 5 字段表达式；任何字段非法（越界/语法错）返回 `None`。
    pub fn parse(expr: &str) -> Option<Self> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return None;
        }
        let (minutes, _) = parse_field::<u64>(fields[0], 0, 59)?;
        let (hours, _) = parse_field::<u32>(fields[1], 0, 23)?;
        let (doms, dom_restricted) = parse_field::<u32>(fields[2], 1, 31)?;
        let (months, _) = parse_field::<u16>(fields[3], 1, 12)?;
        let (mut dows, dow_restricted) = parse_field::<u8>(fields[4], 0, 7)?;
        // dow：7 归一为 0（周日）；bit 7 不用于匹配。
        if dows & (1 << 7) != 0 {
            dows |= 1 << 0;
            dows &= !(1 << 7);
        }
        Some(Self {
            minutes,
            hours,
            doms,
            months,
            dows,
            dom_restricted,
            dow_restricted,
        })
    }

    /// 给定墙钟时间是否匹配。
    pub fn matches<Tz: chrono::TimeZone>(&self, dt: &chrono::DateTime<Tz>) -> bool {
        let minute = dt.minute();
        let hour = dt.hour();
        let dom = dt.day();
        let month = dt.month();
        let dow = dt.weekday().num_days_from_sunday(); // 周日=0

        let time_hit = bit(self.minutes, minute)
            && bit(self.hours as u64, hour)
            && bit(self.months as u64, month);
        if !time_hit {
            return false;
        }
        let dom_hit = bit(self.doms as u64, dom);
        let dow_hit = bit(self.dows as u64, dow);
        // 日与周同时受限 → OR；否则 AND（未受限侧位全置，恒为 true）。
        if self.dom_restricted && self.dow_restricted {
            dom_hit || dow_hit
        } else {
            dom_hit && dow_hit
        }
    }
}

fn bit(mask: u64, n: u32) -> bool {
    n < 64 && mask & (1u64 << n) != 0
}

/// 位掩码字段：可容纳 `max`（最多 63）位的整型。
trait BitMask: Copy {
    fn zero() -> Self;
    fn set(self, n: u32) -> Self;
}

macro_rules! impl_bitmask {
    ($($t:ty),*) => {$(
        impl BitMask for $t {
            fn zero() -> Self { 0 }
            fn set(self, n: u32) -> Self { self | (1 as $t) << n }
        }
    )*};
}
impl_bitmask!(u64, u32, u16, u8);

/// 解析单个字段为位掩码，返回 `(mask, restricted)`；`restricted` 表示非 `*`。
/// 支持逗号列表，每项为 `*` / `*/step` / `a` / `a-b` / `a-b/step` / `a/step`。
fn parse_field<T: BitMask>(spec: &str, min: u32, max: u32) -> Option<(T, bool)> {
    if spec.is_empty() {
        return None;
    }
    let mut mask = T::zero();
    let restricted = spec != "*";
    for part in spec.split(',') {
        // 拆出 range 与 step。
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, Some(s.parse::<u32>().ok().filter(|&s| s > 0)?)),
            None => (part, None),
        };
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let a = a.parse::<u32>().ok()?;
            let b = b.parse::<u32>().ok()?;
            (a, b)
        } else {
            let v = range.parse::<u32>().ok()?;
            // "a/step" → 从 a 到 max 步进；单值 "a" → 仅 a。
            if step.is_some() {
                (v, max)
            } else {
                (v, v)
            }
        };
        if lo < min || hi > max || lo > hi {
            return None;
        }
        let step = step.unwrap_or(1);
        let mut v = lo;
        while v <= hi {
            mask = mask.set(v);
            v += step;
        }
    }
    Some((mask, restricted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_field_count() {
        assert!(CronExpr::parse("* * * *").is_none());
        assert!(CronExpr::parse("* * * * * *").is_none());
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(CronExpr::parse("60 * * * *").is_none());
        assert!(CronExpr::parse("* 24 * * *").is_none());
        assert!(CronExpr::parse("* * 0 * *").is_none()); // dom 最小 1
        assert!(CronExpr::parse("* * * 13 *").is_none());
        assert!(CronExpr::parse("* * * * 8").is_none()); // dow 最大 7
    }

    #[test]
    fn parses_star_as_unrestricted() {
        let e = CronExpr::parse("* * * * *").unwrap();
        assert!(!e.dom_restricted && !e.dow_restricted);
    }

    #[test]
    fn step_and_list_and_range() {
        // 分钟 */20 → {0,20,40}
        let e = CronExpr::parse("*/20 * * * *").unwrap();
        assert!(bit(e.minutes, 0) && bit(e.minutes, 20) && bit(e.minutes, 40));
        assert!(!bit(e.minutes, 10));
        // 列表 + 范围
        let e = CronExpr::parse("0 9-11,15 * * *").unwrap();
        for h in [9, 10, 11, 15] {
            assert!(bit(e.hours as u64, h));
        }
        assert!(!bit(e.hours as u64, 12));
    }
}
