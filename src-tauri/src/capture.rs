//! 临时采集：决定 Pi RPC stdout 原始行的落盘位置。
//!
//! 采集文件按会话工作目录保存，命名为
//! `<工作目录>/.lure-capture/stdout-<UTC 时间戳>.jsonl`，同秒重复连接时追加序号。
//! 该模块与 `lure-rpc` 的 `capture` 模块都是一次性采集工具，真机数据录完、
//! mock 数据固定下来后应整体删除。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 采集文件所在目录，位于会话工作目录之下。
pub(crate) const CAPTURE_DIRECTORY: &str = ".lure-capture";

const MAX_CAPTURE_ATTEMPTS: u32 = 100;
const SECONDS_PER_DAY: i64 = 86_400;

/// 为一次连接挑选未被占用的采集文件路径。
pub(crate) async fn next_capture_path(working_directory: &Path, at: SystemTime) -> PathBuf {
    let directory = working_directory.join(CAPTURE_DIRECTORY);
    let stamp = format_utc_timestamp(at);
    for attempt in 1..MAX_CAPTURE_ATTEMPTS {
        let candidate = directory.join(capture_file_name(&stamp, attempt));
        if !tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
            return candidate;
        }
    }
    directory.join(capture_file_name(&stamp, MAX_CAPTURE_ATTEMPTS))
}

/// 采集文件名；同一秒内第二次连接使用 `-2`、`-3` 后缀。
pub(crate) fn capture_file_name(stamp: &str, attempt: u32) -> String {
    if attempt <= 1 {
        format!("stdout-{stamp}.jsonl")
    } else {
        format!("stdout-{stamp}-{attempt}.jsonl")
    }
}

/// 把时间格式化为文件名安全的 UTC 时间戳：`2026-09-20T12-30-05Z`。
pub(crate) fn format_utc_timestamp(at: SystemTime) -> String {
    let seconds = at.duration_since(UNIX_EPOCH).map_or(0, |elapsed| {
        i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
    });
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let time_of_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3_600;
    let minute = time_of_day % 3_600 / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{minute:02}-{second:02}Z")
}

/// 由 Unix 纪元天数换算公历日期（Howard Hinnant 的 `civil_from_days` 算法）。
fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{capture_file_name, format_utc_timestamp, next_capture_path};

    #[test]
    fn formats_utc_timestamps_in_a_file_name_safe_form() {
        assert_eq!(format_utc_timestamp(UNIX_EPOCH), "1970-01-01T00-00-00Z");
        assert_eq!(
            format_utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_000_000_000)),
            "2001-09-09T01-46-40Z"
        );
        assert_eq!(
            format_utc_timestamp(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "2000-02-29T00-00-00Z"
        );
        assert_eq!(
            format_utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_788_531_005)),
            "2026-09-04T14-10-05Z"
        );
    }

    #[test]
    fn names_capture_files_after_the_timestamp() {
        assert_eq!(
            capture_file_name("2026-09-20T12-30-05Z", 1),
            "stdout-2026-09-20T12-30-05Z.jsonl"
        );
        assert_eq!(
            capture_file_name("2026-09-20T12-30-05Z", 2),
            "stdout-2026-09-20T12-30-05Z-2.jsonl"
        );
    }

    #[tokio::test]
    async fn places_the_capture_file_below_the_working_directory() {
        let root = std::env::temp_dir().join(format!("lure-capture-dir-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&root).await;
        tokio::fs::create_dir_all(&root).await.unwrap();

        let at = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let path = next_capture_path(&root, at).await;

        assert_eq!(
            path,
            root.join(".lure-capture")
                .join("stdout-2001-09-09T01-46-40Z.jsonl")
        );
        tokio::fs::remove_dir_all(&root).await.unwrap();
    }

    #[tokio::test]
    async fn avoids_overwriting_an_existing_capture_file() {
        let root = std::env::temp_dir().join(format!("lure-capture-dup-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&root).await;
        tokio::fs::create_dir_all(root.join(".lure-capture"))
            .await
            .unwrap();

        let at = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let first = next_capture_path(&root, at).await;
        tokio::fs::write(&first, b"{}\n").await.unwrap();
        let second = next_capture_path(&root, at).await;

        assert_eq!(
            second,
            first.with_file_name("stdout-2001-09-09T01-46-40Z-2.jsonl")
        );
        tokio::fs::remove_dir_all(&root).await.unwrap();
    }
}
