//! Subagent 后台执行的状态模型与登记表。
//!
//! 对齐上游 `nanobot/agent/subagent.py`：`SubagentStatus` 实时状态、label 派生、
//! `_format_partial_progress` 部分进度格式化，以及 `SubagentManager` 的 session→task 簿记
//! （`get_running_count`/`get_running_count_by_session`/`cancel_by_session`）——后者是 `/stop`
//! 级联终止的取消原语。
//!
//! 本模块只覆盖确定性可测的状态/簿记核心。真实后台执行（`spawn`/`_run_subagent` 起 agent turn、
//! `_announce_result` 经 bus 回灌、exec session 级联终止）依赖异步运行时与完整 agent 装配，
//! lure 为同步模型，留待引入异步运行时后回补（见 upstream-test-ledger）。

use std::collections::{BTreeMap, BTreeSet};

/// subagent 执行阶段（对齐上游 `phase` 字段取值域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentPhase {
    Initializing,
    AwaitingTools,
    ToolsCompleted,
    FinalResponse,
    Done,
    Error,
}

/// 单个 tool 事件（对齐上游 `tool_events` 项 `{name,status,detail}`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEvent {
    pub name: String,
    /// `"ok"` 或 `"error"`。
    pub status: String,
    pub detail: String,
}

/// 运行中 subagent 的实时状态（对齐上游 `SubagentStatus`）。
#[derive(Debug, Clone)]
pub struct SubagentStatus {
    pub task_id: String,
    pub label: String,
    pub task_description: String,
    /// 起始时刻（单调秒，对齐上游 `time.monotonic()`）。
    pub started_at: f64,
    pub phase: SubagentPhase,
    pub iteration: u32,
    pub tool_events: Vec<ToolEvent>,
    pub usage: BTreeMap<String, i64>,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
}

impl SubagentStatus {
    /// 新建状态，phase 默认 `Initializing`。
    pub fn new(
        task_id: impl Into<String>,
        label: impl Into<String>,
        task_description: impl Into<String>,
        started_at: f64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            label: label.into(),
            task_description: task_description.into(),
            started_at,
            phase: SubagentPhase::Initializing,
            iteration: 0,
            tool_events: Vec::new(),
            usage: BTreeMap::new(),
            stop_reason: None,
            error: None,
        }
    }
}

/// subagent 执行结果的最小视图（用于部分进度格式化）。
#[derive(Debug, Clone, Default)]
pub struct SubagentRunResult {
    pub tool_events: Vec<ToolEvent>,
    pub error: Option<String>,
}

/// 派生 subagent 显示 label：显式 label 优先，否则任务前 30 字符（超长加 `...`）。
///
/// 对齐上游 `label or task[:30] + ("..." if len(task) > 30 else "")`（按 Unicode 码点截断）。
pub fn derive_label(task: &str, label: Option<&str>) -> String {
    if let Some(label) = label {
        return label.to_string();
    }
    let truncated: String = task.chars().take(30).collect();
    if task.chars().count() > 30 {
        format!("{truncated}...")
    } else {
        truncated
    }
}

/// 格式化部分进度：末 3 个完成步骤 + 最后一个失败事件；无事件时回落 error 或兜底文案。
///
/// 对齐上游 `_format_partial_progress`。
pub fn format_partial_progress(result: &SubagentRunResult) -> String {
    let completed: Vec<&ToolEvent> = result
        .tool_events
        .iter()
        .filter(|e| e.status == "ok")
        .collect();
    let failure = result
        .tool_events
        .iter()
        .rev()
        .find(|e| e.status == "error");

    let mut lines: Vec<String> = Vec::new();
    if !completed.is_empty() {
        lines.push("Completed steps:".to_string());
        let start = completed.len().saturating_sub(3);
        for event in &completed[start..] {
            lines.push(format!("- {}: {}", event.name, event.detail));
        }
    }
    if let Some(failure) = failure {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Failure:".to_string());
        lines.push(format!("- {}: {}", failure.name, failure.detail));
    } else if let Some(error) = &result.error {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Failure:".to_string());
        lines.push(format!("- {error}"));
    }

    if lines.is_empty() {
        result
            .error
            .clone()
            .unwrap_or_else(|| "Error: subagent execution failed.".to_string())
    } else {
        lines.join("\n")
    }
}

/// 单条 task 登记。
#[derive(Debug)]
struct TaskEntry {
    session_key: Option<String>,
    /// 是否已完成（但可能尚未 cleanup 移除）。
    done: bool,
    status: SubagentStatus,
    /// 后台 task 的 abort 句柄（Stage 6：exec 级联终止用，spawn 后挂载）。
    abort: Option<tokio::task::AbortHandle>,
}

/// subagent 后台任务登记表：跟踪 task→状态与 session→tasks 索引。
///
/// 对齐上游 `SubagentManager` 的 `_running_tasks` / `_task_statuses` / `_session_tasks` 簿记。
/// lure 同步模型下不持有 asyncio 句柄，用 `done` 标志 + `finish`（cleanup 移除）建模生命周期。
#[derive(Debug, Default)]
pub struct SubagentRegistry {
    tasks: BTreeMap<String, TaskEntry>,
    sessions: BTreeMap<String, BTreeSet<String>>,
}

impl SubagentRegistry {
    /// 空登记表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个已派生的 task；`session_key` 存在时加入 session 索引。
    pub fn register(&mut self, task_id: &str, session_key: Option<&str>, status: SubagentStatus) {
        if let Some(key) = session_key {
            self.sessions
                .entry(key.to_string())
                .or_default()
                .insert(task_id.to_string());
        }
        self.tasks.insert(
            task_id.to_string(),
            TaskEntry {
                session_key: session_key.map(str::to_string),
                done: false,
                status,
                abort: None,
            },
        );
    }

    /// 挂载后台 task 的 abort 句柄（spawn 后调用，供级联终止）。
    pub fn attach_abort(&mut self, task_id: &str, abort: tokio::task::AbortHandle) {
        if let Some(entry) = self.tasks.get_mut(task_id) {
            entry.abort = Some(abort);
        }
    }

    /// 标记 task 已完成（保留在表中，直到 `finish` cleanup）。
    pub fn mark_done(&mut self, task_id: &str) {
        if let Some(entry) = self.tasks.get_mut(task_id) {
            entry.done = true;
            entry.status.phase = SubagentPhase::Done;
        }
    }

    /// cleanup：从表与 session 索引移除 task（对齐上游 done_callback `_cleanup`）。
    pub fn finish(&mut self, task_id: &str) {
        if let Some(entry) = self.tasks.remove(task_id) {
            if let Some(key) = entry.session_key {
                if let Some(ids) = self.sessions.get_mut(&key) {
                    ids.remove(task_id);
                    if ids.is_empty() {
                        self.sessions.remove(&key);
                    }
                }
            }
        }
    }

    /// 当前登记中的 subagent 数（对齐上游 `len(_running_tasks)`）。
    pub fn get_running_count(&self) -> usize {
        self.tasks.len()
    }

    /// 指定 session 下仍在运行（未 done）的 subagent 数。
    pub fn get_running_count_by_session(&self, session_key: &str) -> usize {
        let Some(ids) = self.sessions.get(session_key) else {
            return 0;
        };
        ids.iter()
            .filter(|id| self.tasks.get(*id).is_some_and(|e| !e.done))
            .count()
    }

    /// 取消某 session 下所有未完成 subagent（cleanup 移除），返回取消数量。
    ///
    /// 对齐上游 `cancel_by_session`（取消 + gather + cleanup），是 `/stop` 级联终止的取消原语。
    /// 注：上游还会 `terminate_by_owner` 关联 exec session，此处不建模 exec 运行时。
    pub fn cancel_by_session(&mut self, session_key: &str) -> usize {
        let targets: Vec<String> = self
            .sessions
            .get(session_key)
            .map(|ids| {
                ids.iter()
                    .filter(|id| self.tasks.get(*id).is_some_and(|e| !e.done))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let count = targets.len();
        for id in targets {
            // Stage 6：级联终止——先 abort 后台 task，再 cleanup 登记。
            let abort = self.tasks.get(&id).and_then(|e| e.abort.clone());
            if let Some(abort) = abort {
                abort.abort();
            }
            self.finish(&id);
        }
        count
    }

    /// 查询某 task 的状态（若仍在表中）。
    pub fn status(&self, task_id: &str) -> Option<&SubagentStatus> {
        self.tasks.get(task_id).map(|e| &e.status)
    }
}
