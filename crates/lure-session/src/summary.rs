use std::path::PathBuf;

use serde::Serialize;

/// 一页 Pi 会话摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    /// 当前页的会话摘要。
    pub sessions: Vec<SessionSummary>,
    /// 当前页之后是否仍有会话。
    pub has_more: bool,
}

/// 单个 Pi 会话文件的只读摘要。
///
/// 字段与 Pi 的 `SessionInfo` 对齐，便于桌面端直接按最近活动时间排序和展示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// 会话 JSONL 文件路径。
    pub path: PathBuf,
    /// 会话 id（来自文件头）。
    pub id: String,
    /// 会话头部记录的工作目录；旧会话可能为空。
    pub cwd: Option<String>,
    /// 用户为该会话设置的显示名（取最新的 `session_info`）。
    pub name: Option<String>,
    /// 派生该会话的父会话路径。
    pub parent_session_path: Option<String>,
    /// 会话创建时间（Unix 毫秒）。
    pub created_at_ms: i64,
    /// 最后一次用户或助手消息活动时间（Unix 毫秒）。
    pub modified_at_ms: i64,
    /// 会话内的消息条目数量。
    pub message_count: usize,
    /// 首条用户消息的文本，用于在列表里提供可读标题。
    pub first_message: Option<String>,
}
