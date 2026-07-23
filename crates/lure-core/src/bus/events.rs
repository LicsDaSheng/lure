//! Message bus 事件类型。
//!
//! 对齐上游 `nanobot/bus/events.py` 的 `InboundMessage` / `OutboundMessage`：
//! channel 与 agent 之间的统一消息契约。CLI、WebSocket 等入口都收敛到
//! `InboundMessage`，agent 产出 `OutboundMessage` 交由 channel 投递。

use serde_json::{Map, Value};

/// 从 channel 收到的消息。
#[derive(Debug, Clone, PartialEq)]
pub struct InboundMessage {
    /// 来源渠道（telegram、discord、cli 等）。
    pub channel: String,
    /// 发送者标识。
    pub sender_id: String,
    /// 会话/聊天标识。
    pub chat_id: String,
    /// 消息文本。
    pub content: String,
    /// 媒体 URL 列表。
    pub media: Vec<String>,
    /// 渠道相关的元数据。
    pub metadata: Map<String, Value>,
    /// thread 级会话的 session key 覆盖。
    pub session_key_override: Option<String>,
}

impl InboundMessage {
    /// 构造最小 inbound（sender 默认空、无媒体/元数据/覆盖）。
    pub fn new(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            sender_id: String::new(),
            chat_id: chat_id.into(),
            content: content.into(),
            media: Vec::new(),
            metadata: Map::new(),
            session_key_override: None,
        }
    }

    /// 会话识别用的唯一 key：优先覆盖，否则 `channel:chat_id`。
    pub fn session_key(&self) -> String {
        self.session_key_override
            .clone()
            .unwrap_or_else(|| format!("{}:{}", self.channel, self.chat_id))
    }
}

/// 要发送到 channel 的消息。
#[derive(Debug, Clone, PartialEq)]
pub struct OutboundMessage {
    /// 目标渠道。
    pub channel: String,
    /// 目标聊天标识。
    pub chat_id: String,
    /// 消息文本。
    pub content: String,
    /// 回复目标消息 id。
    pub reply_to: Option<String>,
    /// 媒体 URL 列表。
    pub media: Vec<String>,
    /// 渠道路由上下文。
    pub metadata: Map<String, Value>,
}

impl OutboundMessage {
    /// 构造最小 outbound。
    pub fn new(
        channel: impl Into<String>,
        chat_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            chat_id: chat_id.into(),
            content: content.into(),
            reply_to: None,
            media: Vec::new(),
            metadata: Map::new(),
        }
    }

    /// 由 inbound 与回复文本构造 outbound（回到来源 channel/chat）。
    pub fn reply(inbound: &InboundMessage, content: impl Into<String>) -> Self {
        Self::new(inbound.channel.clone(), inbound.chat_id.clone(), content)
    }
}
