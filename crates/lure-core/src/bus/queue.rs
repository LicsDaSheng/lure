//! 解耦 channel 与 agent 的消息队列。
//!
//! 对齐上游 `nanobot/bus/queue.py::MessageBus` 的职责边界：channel 把消息推入
//! inbound 队列，agent 处理后把响应推入 outbound 队列。
//!
//! Phase 7 采用**同步**内存队列（上游为 asyncio.Queue）；async 运行时留待接入
//! 真实 channel/网络时引入（见 upstream-test-ledger）。

use std::collections::VecDeque;

use crate::bus::events::{InboundMessage, OutboundMessage};

/// 同步内存消息总线。
#[derive(Debug, Default)]
pub struct MessageBus {
    inbound: VecDeque<InboundMessage>,
    outbound: VecDeque<OutboundMessage>,
}

impl MessageBus {
    /// 新建空总线。
    pub fn new() -> Self {
        Self::default()
    }

    /// 发布一条 inbound 消息。
    pub fn publish_inbound(&mut self, message: InboundMessage) {
        self.inbound.push_back(message);
    }

    /// 取出下一条 inbound（无则 `None`）。
    pub fn consume_inbound(&mut self) -> Option<InboundMessage> {
        self.inbound.pop_front()
    }

    /// 发布一条 outbound 消息。
    pub fn publish_outbound(&mut self, message: OutboundMessage) {
        self.outbound.push_back(message);
    }

    /// 取出下一条 outbound（无则 `None`）。
    pub fn consume_outbound(&mut self) -> Option<OutboundMessage> {
        self.outbound.pop_front()
    }

    /// 待处理 inbound 数量。
    pub fn inbound_size(&self) -> usize {
        self.inbound.len()
    }

    /// 待处理 outbound 数量。
    pub fn outbound_size(&self) -> usize {
        self.outbound.len()
    }
}
