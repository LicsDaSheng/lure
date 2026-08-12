//! 异步消息队列：channel → agent 的 async 投递面（Stage 1）。
//!
//! 对齐上游 `nanobot/bus/queue.py` 的 asyncio.Queue 语义：有界 mpsc，满时
//! `publish` 等待（backpressure），全部 sender drop 后 `consume` 返回 `None`。
//! 同步 [`MessageBus`](super::MessageBus) 保持不动（CLI/既有路径继续使用）。

use tokio::sync::mpsc;

use crate::bus::events::InboundMessage;

/// 异步总线发送端（可 clone，多 producer）。
#[derive(Clone)]
pub struct AsyncBusSender {
    tx: mpsc::Sender<InboundMessage>,
}

/// 异步总线接收端（唯一消费方）。
pub struct AsyncBusReceiver {
    rx: mpsc::Receiver<InboundMessage>,
}

/// 创建有界异步消息队列。`capacity` 为队容量（满时 send 等待）。
pub fn async_bus_channel(capacity: usize) -> (AsyncBusSender, AsyncBusReceiver) {
    let (tx, rx) = mpsc::channel(capacity);
    (AsyncBusSender { tx }, AsyncBusReceiver { rx })
}

/// 异步总线错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncBusError {
    /// 接收端已关闭（全部 sender 之外的消费者退出）。
    ReceiverClosed,
    /// 队列已满（仅 `try_publish`；消息被丢弃）。
    Full,
}

impl std::fmt::Display for AsyncBusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsyncBusError::ReceiverClosed => write!(f, "异步总线接收端已关闭"),
            AsyncBusError::Full => write!(f, "异步总线队列已满"),
        }
    }
}

impl std::error::Error for AsyncBusError {}

impl AsyncBusSender {
    /// 发布一条 inbound；队列满时等待（backpressure）。
    pub async fn publish(&self, message: InboundMessage) -> Result<(), AsyncBusError> {
        self.tx
            .send(message)
            .await
            .map_err(|_| AsyncBusError::ReceiverClosed)
    }

    /// 非阻塞发布；满时返回 [`AsyncBusError::Full`]（消息丢弃），通道关闭返回
    /// [`AsyncBusError::ReceiverClosed`]。
    pub fn try_publish(&self, message: InboundMessage) -> Result<(), AsyncBusError> {
        self.tx.try_send(message).map_err(|e| match e {
            tokio::sync::mpsc::error::TrySendError::Full(_) => AsyncBusError::Full,
            tokio::sync::mpsc::error::TrySendError::Closed(_) => AsyncBusError::ReceiverClosed,
        })
    }
}

impl AsyncBusReceiver {
    /// 取出一条消息；全部 sender drop 后返回 `None`（通道关闭）。
    pub async fn consume(&mut self) -> Option<InboundMessage> {
        self.rx.recv().await
    }

    /// 非阻塞取出；空时返回 `None`。
    pub fn try_consume(&mut self) -> Option<InboundMessage> {
        self.rx.try_recv().ok()
    }
}
