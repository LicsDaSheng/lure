//! Bus 子系统：channel 与 agent 之间的统一消息契约与队列。
//!
//! Phase 7 覆盖 InboundMessage/OutboundMessage、同步内存 MessageBus 与传输无关的
//! outbound 运行时事件 ProgressUpdate。async 队列留待后续。

mod events;
mod progress;
mod queue;

pub use events::{InboundMessage, OutboundMessage};
pub use progress::{ProgressKind, ProgressUpdate};
pub use queue::MessageBus;
