//! Trigger 子系统：本地 trigger 投递队列（at-least-once）。
//!
//! Phase 8 覆盖投递队列的 enqueue/claim/complete/recover 与 busy-session 等待语义。
//! trigger 定义存储、文件 inbox 布局、gateway 消费循环留待后续。

mod queue;

pub use queue::{never_busy, LocalTriggerQueue, TriggerDelivery};
