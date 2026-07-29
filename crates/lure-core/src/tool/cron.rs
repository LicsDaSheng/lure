//! `cron` 工具：让 agent 排期提醒与循环任务（add/list/remove）。
//!
//! 对齐上游 `nanobot/agent/tools/cron.py`：单一工具 `action`（add/list/remove），
//! add 需非空 `message` 与一种调度（`every_seconds`/`cron_expr`/`at`），并绑定发起会话
//! 的 origin（session_key/channel/chat_id）供定时投递。per-action 必填在 execute 运行时
//! 校验（顶层 schema 保持扁平，兼容不接受 oneOf/enum 根的 provider）。
//!
//! 存储：每次调用 load/save `CronStore`（workspace/cron/jobs.json），无共享可变状态。
//! tz：UTC（默认）或任意可被 chrono-tz 解析的 IANA 名（如 `Asia/Shanghai`），
//! 按其 DST 规则解释；无法识别的名字在 add 时拒绝。

use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::cron::{CronJob, CronJobState, CronPayload, CronSchedule, CronStore};
use crate::tool::registry::Tool;
use crate::tool::result::ToolResult;

/// 发起会话路由：cron job 触发后按此投递回原会话。
#[derive(Debug, Clone)]
pub struct CronToolOrigin {
    /// 会话 key（session-bound 投递）。
    pub session_key: String,
    /// 来源渠道。
    pub channel: String,
    /// 来源 chat id。
    pub chat_id: String,
}

/// cron 排期工具。
pub struct CronTool {
    workspace: PathBuf,
    origin: CronToolOrigin,
    default_tz: String,
}

impl CronTool {
    /// 新建绑定到某会话的 cron 工具。`default_tz` 为默认时区（`UTC` 或 IANA 名）。
    pub fn new(
        workspace: impl Into<PathBuf>,
        origin: CronToolOrigin,
        default_tz: impl Into<String>,
    ) -> Self {
        Self {
            workspace: workspace.into(),
            origin,
            default_tz: default_tz.into(),
        }
    }

    fn now_ms() -> i64 {
        Utc::now().timestamp_millis()
    }

    fn add(&self, args: &Value) -> ToolResult {
        let message = args
            .get("message")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if message.is_empty() {
            return ToolResult::error("Error: cron action='add' 需要非空的 message 参数");
        }

        let tz = args.get("tz").and_then(Value::as_str);
        let cron_expr = args.get("cron_expr").and_then(Value::as_str);
        let every_seconds = args.get("every_seconds").and_then(Value::as_i64);
        let at = args.get("at").and_then(Value::as_str);

        if tz.is_some() && cron_expr.is_none() {
            return ToolResult::error("Error: tz 只能与 cron_expr 搭配使用");
        }

        // 构建调度：every → cron → at（对齐上游优先级）。
        let (schedule, delete_after) = if let Some(secs) = every_seconds.filter(|&s| s > 0) {
            (CronSchedule::every(secs * 1000), false)
        } else if let Some(expr) = cron_expr {
            let effective_tz = tz.unwrap_or(&self.default_tz);
            if !is_supported_tz(effective_tz) {
                return ToolResult::error(format!(
                    "Error: 无法识别时区 '{effective_tz}'（用 UTC 或 IANA 名如 Asia/Shanghai）"
                ));
            }
            (
                CronSchedule::cron(expr, Some(effective_tz.to_string())),
                false,
            )
        } else if let Some(at_str) = at {
            match parse_at_ms(at_str) {
                Some(at_ms) => (CronSchedule::at(at_ms), true),
                None => {
                    return ToolResult::error(format!(
                        "Error: 无效的 ISO 时间 '{at_str}'，期望 YYYY-MM-DDTHH:MM:SS"
                    ))
                }
            }
        } else {
            return ToolResult::error("Error: 需要 every_seconds、cron_expr 或 at 之一");
        };

        let now = Self::now_ms();
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| message.chars().take(30).collect());
        let id = Uuid::new_v4().to_string();

        let job = CronJob {
            id: id.clone(),
            name: name.clone(),
            enabled: true,
            schedule,
            payload: CronPayload {
                message: message.to_string(),
                session_key: Some(self.origin.session_key.clone()),
                origin_channel: Some(self.origin.channel.clone()),
                origin_chat_id: Some(self.origin.chat_id.clone()),
                ..CronPayload::default()
            },
            state: CronJobState::default(),
            created_at_ms: now,
            updated_at_ms: now,
            delete_after_run: delete_after,
        };

        let mut store = match CronStore::load(&self.workspace) {
            Ok(s) => s,
            Err(e) => return ToolResult::error(format!("Error: 加载 cron 存储失败: {e}")),
        };
        match store.add(job, now) {
            Ok(()) => ToolResult::ok(format!("已创建任务 '{name}' (id: {id})")),
            Err(e) => ToolResult::error(format!("Error: 添加任务失败: {e}")),
        }
    }

    fn list(&self) -> ToolResult {
        let store = match CronStore::load(&self.workspace) {
            Ok(s) => s,
            Err(e) => return ToolResult::error(format!("Error: 加载 cron 存储失败: {e}")),
        };
        let jobs = store.jobs();
        if jobs.is_empty() {
            return ToolResult::ok("暂无排期任务。");
        }
        let lines: Vec<String> = jobs
            .iter()
            .map(|j| {
                let status = if j.enabled { "enabled" } else { "disabled" };
                format!(
                    "- {} (id: {}) [{}] {} — {}",
                    j.name,
                    j.id,
                    status,
                    format_timing(&j.schedule),
                    j.payload.message
                )
            })
            .collect();
        ToolResult::ok(lines.join("\n"))
    }

    fn remove(&self, args: &Value) -> ToolResult {
        let job_id = args
            .get("job_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");
        if job_id.is_empty() {
            return ToolResult::error("Error: cron action='remove' 需要 job_id 参数");
        }
        let mut store = match CronStore::load(&self.workspace) {
            Ok(s) => s,
            Err(e) => return ToolResult::error(format!("Error: 加载 cron 存储失败: {e}")),
        };
        match store.remove(job_id) {
            Ok(true) => ToolResult::ok(format!("已删除任务 (id: {job_id})")),
            Ok(false) => ToolResult::error(format!("Error: 未找到任务 (id: {job_id})")),
            Err(e) => ToolResult::error(format!("Error: {e}")),
        }
    }
}

impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "排期提醒与循环任务。action: add/list/remove。add 需 message 与一种调度\
         （every_seconds 循环、cron_expr 表达式、at 单次 ISO 时间）；remove 需 job_id。\
         cron_expr 可搭配 tz（UTC 或 IANA 名如 Asia/Shanghai）。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["add", "list", "remove"], "description": "执行的操作"},
                "name": {"type": "string", "description": "可选的任务短标签，默认取 message 前 30 字"},
                "message": {"type": "string", "description": "action=add 必填：触发时给 agent 的指令"},
                "every_seconds": {"type": "integer", "description": "循环间隔秒数"},
                "cron_expr": {"type": "string", "description": "cron 表达式，如 '0 9 * * *'"},
                "tz": {"type": "string", "description": "cron_expr 的时区：UTC 或 IANA 名（如 Asia/Shanghai），默认 UTC"},
                "at": {"type": "string", "description": "单次执行的 ISO 时间，如 '2026-02-12T10:30:00'"},
                "job_id": {"type": "string", "description": "action=remove 必填：要删除的任务 id（用 list 获取）"}
            },
            "required": ["action"]
        })
    }

    fn execute(&self, args: &Value) -> ToolResult {
        match args.get("action").and_then(Value::as_str) {
            Some("add") => self.add(args),
            Some("list") => self.list(),
            Some("remove") => self.remove(args),
            Some(other) => ToolResult::error(format!("Error: 未知 action '{other}'")),
            None => ToolResult::error("Error: 缺少 action 参数"),
        }
    }
}

/// UTC/Z 或任意可被 chrono-tz 解析的 IANA 名视为受支持。
fn is_supported_tz(tz: &str) -> bool {
    tz.eq_ignore_ascii_case("utc") || tz == "Z" || tz.parse::<chrono_tz::Tz>().is_ok()
}

/// 解析 ISO 时间为 epoch ms：带偏移用之，naive 视为 UTC。
fn parse_at_ms(s: &str) -> Option<i64> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()?;
    Some(naive.and_utc().timestamp_millis())
}

/// 人类可读的调度描述。
fn format_timing(schedule: &CronSchedule) -> String {
    use crate::cron::ScheduleKind;
    match schedule.kind {
        ScheduleKind::Cron => match &schedule.expr {
            Some(expr) => {
                let tz = schedule
                    .tz
                    .as_deref()
                    .map(|t| format!(" ({t})"))
                    .unwrap_or_default();
                format!("cron: {expr}{tz}")
            }
            None => "cron".to_string(),
        },
        ScheduleKind::Every => match schedule.every_ms {
            Some(ms) if ms % 3_600_000 == 0 => format!("every {}h", ms / 3_600_000),
            Some(ms) if ms % 60_000 == 0 => format!("every {}m", ms / 60_000),
            Some(ms) if ms % 1000 == 0 => format!("every {}s", ms / 1000),
            Some(ms) => format!("every {ms}ms"),
            None => "every".to_string(),
        },
        ScheduleKind::At => match schedule.at_ms {
            Some(ms) => format!("at {ms}ms"),
            None => "at".to_string(),
        },
    }
}
