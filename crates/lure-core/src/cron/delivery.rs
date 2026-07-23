//! Session-bound cron 投递上下文。
//!
//! 对齐上游 `nanobot/cron/session_delivery.py`：把 session-bound cron turn 路由回其
//! 来源 session（channel/chat_id/metadata）。缺少 origin 字段时返回错误。

use serde_json::{Map, Value};

use crate::cron::types::CronJob;

/// origin delivery 缺失字段错误。
#[derive(Debug, Clone, PartialEq)]
pub struct MissingOriginError {
    /// job id。
    pub job_id: String,
}

impl std::fmt::Display for MissingOriginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cron job {} is missing origin delivery context",
            self.job_id
        )
    }
}

impl std::error::Error for MissingOriginError {}

/// 返回 session-bound cron job 的 `(channel, chat_id, metadata)`。
pub fn origin_delivery_context(
    job: &CronJob,
) -> Result<(String, String, Map<String, Value>), MissingOriginError> {
    let payload = &job.payload;
    match (&payload.origin_channel, &payload.origin_chat_id) {
        (Some(channel), Some(chat_id)) if !channel.is_empty() && !chat_id.is_empty() => Ok((
            channel.clone(),
            chat_id.clone(),
            payload.origin_metadata.clone(),
        )),
        _ => Err(MissingOriginError {
            job_id: job.id.clone(),
        }),
    }
}
