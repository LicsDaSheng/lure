//! Cron 类型：schedule、payload、job 与 store。
//!
//! 对齐上游 `nanobot/cron/types.py`。序列化用 camelCase（`atMs`/`everyMs` 等），
//! 反序列化兼容 snake_case。Phase 8 建模 at/every/cron 三种 schedule 与 session-bound
//! delivery 所需的 origin 字段。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// schedule 种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleKind {
    /// 单次定时（`at_ms`）。
    At,
    /// 固定间隔（`every_ms`）。
    Every,
    /// cron 表达式（`expr` + 可选 `tz`）。
    Cron,
}

/// cron job 的调度定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronSchedule {
    /// 调度种类。
    pub kind: ScheduleKind,
    /// `at`：触发时间戳（ms）。
    #[serde(default, alias = "at_ms", skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    /// `every`：间隔（ms）。
    #[serde(default, alias = "every_ms", skip_serializing_if = "Option::is_none")]
    pub every_ms: Option<i64>,
    /// `cron`：表达式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    /// `cron`：时区。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
}

impl CronSchedule {
    /// 构造固定间隔调度。
    pub fn every(every_ms: i64) -> Self {
        Self {
            kind: ScheduleKind::Every,
            at_ms: None,
            every_ms: Some(every_ms),
            expr: None,
            tz: None,
        }
    }

    /// 构造单次定时调度。
    pub fn at(at_ms: i64) -> Self {
        Self {
            kind: ScheduleKind::At,
            at_ms: Some(at_ms),
            every_ms: None,
            expr: None,
            tz: None,
        }
    }

    /// 构造 cron 表达式调度（`tz` 为 IANA 名，`None` 表示本地时区）。
    pub fn cron(expr: impl Into<String>, tz: Option<String>) -> Self {
        Self {
            kind: ScheduleKind::Cron,
            at_ms: None,
            every_ms: None,
            expr: Some(expr.into()),
            tz,
        }
    }
}

/// 运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// 成功。
    Ok,
    /// 失败。
    Error,
    /// 跳过。
    Skipped,
}

/// job 运行时产出的内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CronPayload {
    /// 触发消息。
    #[serde(default)]
    pub message: String,
    /// 原始 session key（用于正确的 session 记录）。
    #[serde(
        default,
        alias = "session_key",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_key: Option<String>,
    /// session-bound 投递的来源渠道。
    #[serde(
        default,
        alias = "origin_channel",
        skip_serializing_if = "Option::is_none"
    )]
    pub origin_channel: Option<String>,
    /// session-bound 投递的来源 chat id。
    #[serde(
        default,
        alias = "origin_chat_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub origin_chat_id: Option<String>,
    /// 来源元数据。
    #[serde(default, alias = "origin_metadata")]
    pub origin_metadata: Map<String, Value>,
}

/// job 的运行时状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CronJobState {
    /// 下次运行时间（ms）。
    #[serde(
        default,
        alias = "next_run_at_ms",
        skip_serializing_if = "Option::is_none"
    )]
    pub next_run_at_ms: Option<i64>,
    /// 上次运行时间（ms）。
    #[serde(
        default,
        alias = "last_run_at_ms",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_run_at_ms: Option<i64>,
    /// 上次状态。
    #[serde(
        default,
        alias = "last_status",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_status: Option<RunStatus>,
}

/// 一个调度 job。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    /// job id。
    pub id: String,
    /// job 名（`heartbeat` 为保留的受保护 job）。
    pub name: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 调度定义。
    pub schedule: CronSchedule,
    /// 运行内容。
    #[serde(default)]
    pub payload: CronPayload,
    /// 运行时状态。
    #[serde(default)]
    pub state: CronJobState,
    /// 创建时间（ms）。
    #[serde(default, alias = "created_at_ms")]
    pub created_at_ms: i64,
    /// 更新时间（ms）。
    #[serde(default, alias = "updated_at_ms")]
    pub updated_at_ms: i64,
    /// 运行后删除（一次性提醒）。
    #[serde(default, alias = "delete_after_run")]
    pub delete_after_run: bool,
}

fn default_true() -> bool {
    true
}

/// 持久化 store 的顶层结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CronStoreData {
    /// 存储版本。
    pub version: u32,
    /// 全部 job。
    pub jobs: Vec<CronJob>,
}

impl Default for CronStoreData {
    fn default() -> Self {
        Self {
            version: 1,
            jobs: Vec::new(),
        }
    }
}
