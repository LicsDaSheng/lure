//! Session-bound cron 投递上下文。
//!
//! 对齐上游 `nanobot/cron/session_delivery.py`：把 session-bound cron turn 路由回其
//! 来源 session（channel/chat_id/metadata）。缺少 origin 字段时返回错误。

use serde_json::{Map, Value};

use crate::bus::InboundMessage;
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

/// 把到期 cron job 构建为投递进 bus 的 [`InboundMessage`]（submit 语义，Stage 4）。
///
/// 对齐上游 `submit_cron_turn`：job 的消息以来源 session 为会话上下文入队。
/// - channel/chat_id 来自 origin delivery 上下文；
/// - `session_key` 覆盖取 `payload.session_key`（未设时回落 `channel:chat_id`，
///   即 [`InboundMessage::session_key`](crate::bus::InboundMessage::session_key) 的默认派生）。
pub fn cron_submit_message(job: &CronJob) -> Result<InboundMessage, MissingOriginError> {
    let (channel, chat_id, _metadata) = origin_delivery_context(job)?;
    let mut msg = InboundMessage::new(channel, chat_id, &job.payload.message);
    if let Some(session_key) = job.payload.session_key.as_ref().filter(|s| !s.is_empty()) {
        msg.session_key_override = Some(session_key.clone());
    }
    Ok(msg)
}
