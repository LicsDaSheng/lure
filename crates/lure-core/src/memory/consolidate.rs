//! Dream consolidation：可替换 runner。
//!
//! 对齐上游 `Consolidator` 的职责边界，但把 LLM 抽象为可替换的 [`DreamRunner`]：
//! 读取 dream cursor 之后的未整合 history，交给 runner 产出新的 MEMORY.md，写回并
//! 推进 dream cursor。测试用 fake runner，不接真实 LLM。
//!
//! Phase 6 不做：真实 LLM dream、SOUL/USER 的整合、迭代/批次策略、git 提交。

use crate::memory::store::{HistoryEntry, MemoryStore};

/// 把当前记忆与未整合历史整合为新的长期记忆。
pub trait DreamRunner {
    /// 返回整合后的 MEMORY.md 全文。
    fn consolidate(&self, current_memory: &str, entries: &[HistoryEntry]) -> String;
}

/// 一次整合的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationOutcome {
    /// 本次处理的 history 条数。
    pub processed: usize,
    /// 推进后的 dream cursor。
    pub new_cursor: u64,
}

impl MemoryStore {
    /// 用 runner 整合 dream cursor 之后的历史；无未整合历史返回 `None`。
    pub fn consolidate<R: DreamRunner>(&self, runner: &R) -> Option<ConsolidationOutcome> {
        let since = self.get_last_dream_cursor();
        let entries = self.read_unprocessed_history(since);
        if entries.is_empty() {
            return None;
        }

        let new_memory = runner.consolidate(&self.read_memory(), &entries);
        self.write_memory(&new_memory);

        let new_cursor = entries.iter().map(|e| e.cursor).max().unwrap_or(since);
        self.set_last_dream_cursor(new_cursor);

        Some(ConsolidationOutcome {
            processed: entries.len(),
            new_cursor,
        })
    }
}
