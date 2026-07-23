//! Bus 子系统：channel 与 agent 之间的统一消息契约与队列。
//!
//! Phase 7 覆盖 InboundMessage/OutboundMessage 与同步内存 MessageBus。
//! progress/outbound runtime 事件、async 队列留待后续。

mod events;
mod queue;

pub use events::{InboundMessage, OutboundMessage};
pub use queue::MessageBus;
