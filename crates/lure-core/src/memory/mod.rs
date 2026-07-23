//! Memory 子系统：长期记忆存储、history 与 dream consolidation。
//!
//! Phase 6 覆盖 MemoryStore（MEMORY/SOUL/USER + history.jsonl + dream cursor）、
//! `strip_think`、可替换 `DreamRunner` 整合，以及供 context 注入的记忆块。
//! GitStore、legacy 迁移、真实 LLM dream、autocompact 留待后续。

mod consolidate;
mod store;
mod strip;

pub use consolidate::{ConsolidationOutcome, DreamRunner};
pub use store::{HistoryEntry, MemoryStore};
pub use strip::strip_think;
