/// 把 ISO 8601 时间戳解析为 Unix 毫秒。
///
/// 支持 Pi 会话头部使用的 `YYYY-MM-DDTHH:MM:SS[.sss]Z`，以及带 `±HH:MM` 偏移的形式。
/// 无法解析时返回 `None`，调用方负责回退到文件修改时间。
pub(crate) fn iso8601_to_epoch_ms(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = parse_digits(value, 0, 4)?;
    let month: i64 = parse_digits(value, 5, 2)?;
    let day: i64 = parse_digits(value, 8, 2)?;
    let hour: i64 = parse_digits(value, 11, 2)?;
    let minute: i64 = parse_digits(value, 14, 2)?;
    let second: i64 = parse_digits(value, 17, 2)?;
    if !matches!(bytes[4], b'-') || !matches!(bytes[7], b'-') || !matches!(bytes[10], b'T') {
        return None;
    }
    if !matches!(bytes[13], b':') || !matches!(bytes[16], b':') {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let mut index = 19;
    let mut milliseconds: i64 = 0;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == start {
            return None;
        }
        let fraction = &value[start..index];
        let millis_text: String = fraction.chars().take(3).collect();
        let padded = format!("{millis_text:0<3}");
        milliseconds = padded.parse().ok()?;
    }

    let offset_minutes = match bytes.get(index) {
        None | Some(b'Z' | b'z') => 0,
        Some(sign @ (b'+' | b'-')) => {
            let sign_value = if *sign == b'+' { 1 } else { -1 };
            let hours: i64 = parse_digits(value, index + 1, 2)?;
            if bytes.get(index + 3) != Some(&b':') {
                return None;
            }
            let minutes: i64 = parse_digits(value, index + 4, 2)?;
            sign_value * (hours * 60 + minutes)
        }
        _ => return None,
    };

    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_minutes * 60;
    Some(seconds * 1_000 + milliseconds)
}

fn parse_digits(value: &str, start: usize, length: usize) -> Option<i64> {
    let slice = value.get(start..start + length)?;
    if !slice.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    slice.parse().ok()
}

/// Howard Hinnant 的 `days_from_civil`，返回相对 1970-01-01 的天数。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::iso8601_to_epoch_ms;

    #[test]
    fn parses_pi_session_timestamps() {
        assert_eq!(iso8601_to_epoch_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            iso8601_to_epoch_ms("2026-09-20T12:29:50.230Z"),
            Some(1_789_907_390_230)
        );
        assert_eq!(
            iso8601_to_epoch_ms("2026-09-20T12:29:50Z"),
            Some(1_789_907_390_000)
        );
    }

    #[test]
    fn applies_timezone_offsets() {
        assert_eq!(
            iso8601_to_epoch_ms("2026-09-20T12:29:50.000+02:00"),
            Some(1_789_900_190_000)
        );
    }

    #[test]
    fn rejects_malformed_timestamps() {
        assert_eq!(iso8601_to_epoch_ms("not-a-time"), None);
        assert_eq!(iso8601_to_epoch_ms("2026-13-20T12:29:50Z"), None);
        assert_eq!(iso8601_to_epoch_ms("2026-09-20 12:29:50Z"), None);
        assert_eq!(iso8601_to_epoch_ms("2026-09-20T12:29:50.Z"), None);
    }
}
