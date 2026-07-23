//! Session 子系统：key 助手、内存模型、存储与 goal 派生视图。
//!
//! Phase 2 覆盖 session key、JSONL 存储与恢复、`last_consolidated` clamp、有界 LRU
//! 缓存、原子写 + fsync、goal_state 派生视图。turn continuation、webui/automation
//! turns、weak-overflow 身份保留留待对应 phase。

pub mod goal_state;
pub mod keys;
mod model;
mod store;

pub use keys::{session_key_for_channel, UNIFIED_SESSION_KEY};
pub use model::{Session, FILE_MAX_MESSAGES};
pub use store::{SessionError, SessionManager, SESSION_CACHE_MAX_SIZE};
