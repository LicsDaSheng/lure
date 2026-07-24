//! Outbound 运行时事件：把 agent 的 progress 以传输无关的形式转发给 channel。
//!
//! 对齐上游把处理进度（开始、工具调用、最终回复）流式发给 channel 的职责边界，
//! 但收敛为同步、传输无关的 [`ProgressUpdate`]：gateway 由 agent 的 progress 事件
//! 加上路由信息（channel/chat_id）构造，交给 channel 的 `deliver_progress`。

/// progress 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    /// 本轮开始处理。
    Started,
    /// 流式内容增量。
    ContentDelta,
    /// 调用了某个工具。
    ToolInvoked,
    /// 产生最终回复。
    Final,
}

/// 一条转发给 channel 的 progress 更新。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressUpdate {
    /// 目标渠道。
    pub channel: String,
    /// 目标聊天标识。
    pub chat_id: String,
    /// 事件类型。
    pub kind: ProgressKind,
    /// 事件内容（session key / 工具名 / 最终回复文本，视 `kind` 而定）。
    pub content: String,
}

impl ProgressUpdate {
    /// 构造一条 progress 更新。
    pub fn new(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        kind: ProgressKind,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            chat_id: chat_id.into(),
            kind,
            content: content.into(),
        }
    }
}
