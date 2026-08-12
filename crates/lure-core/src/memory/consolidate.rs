//! Dream consolidation：可替换 runner。
//!
//! 对齐上游 `Consolidator` 的职责边界，但把 LLM 抽象为可替换的 [`DreamRunner`]：
//! 读取 dream cursor 之后的未整合 history，交给 runner 产出新的 MEMORY.md，写回并
//! 推进 dream cursor。测试用 fake runner；[`ProviderDreamRunner`] 提供真实 LLM 驱动。

use crate::memory::store::{HistoryEntry, MemoryStore};
use crate::provider::{CompletionRequest, LlmProvider};

/// 把当前记忆与未整合历史整合为新的长期记忆。
#[async_trait::async_trait]
pub trait DreamRunner: Send + Sync {
    /// 返回整合后的 MEMORY.md 全文。
    async fn consolidate(&self, current_memory: &str, entries: &[HistoryEntry]) -> String;
}

/// `&dyn DreamRunner` 仍满足 `DreamRunner`（委托到 vtable）。
#[async_trait::async_trait]
impl DreamRunner for &dyn DreamRunner {
    async fn consolidate(&self, current_memory: &str, entries: &[HistoryEntry]) -> String {
        (**self).consolidate(current_memory, entries).await
    }
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
    pub async fn consolidate<R: DreamRunner + ?Sized>(
        &self,
        runner: &R,
    ) -> Option<ConsolidationOutcome> {
        let since = self.get_last_dream_cursor();
        let entries = self.read_unprocessed_history(since);
        if entries.is_empty() {
            return None;
        }

        let new_memory = runner.consolidate(&self.read_memory(), &entries).await;
        self.write_memory(&new_memory);

        let new_cursor = entries.iter().map(|e| e.cursor).max().unwrap_or(since);
        self.set_last_dream_cursor(new_cursor);

        Some(ConsolidationOutcome {
            processed: entries.len(),
            new_cursor,
        })
    }

    /// `true` 如果 dream cursor 之后的未整合历史数 >= `min_entries`。
    pub fn should_consolidate(&self, min_entries: usize) -> bool {
        self.read_unprocessed_history(self.get_last_dream_cursor())
            .len()
            >= min_entries
    }
}

/// 真实 LLM 驱动的 dream consolidation runner。
///
/// 构建 prompt：把当前 MEMORY.md 与新 history 条目拼成一条系统提示，
/// 交给 provider 生成更新后的 MEMORY.md 全文。
pub struct ProviderDreamRunner {
    provider: Box<dyn LlmProvider + Send>,
}

impl ProviderDreamRunner {
    pub fn new(provider: Box<dyn LlmProvider + Send>) -> Self {
        Self { provider }
    }
}

#[async_trait::async_trait]
impl DreamRunner for ProviderDreamRunner {
    async fn consolidate(&self, current_memory: &str, entries: &[HistoryEntry]) -> String {
        let user_text = entries
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "你是一个记忆整合助手。下面是当前的长期记忆：\n\n{memory}\n\n下面是最近的对话历史条目，请提取其中有价值的信息并更新上面的长期记忆。\
             直接返回更新后的完整 MEMORY.md 内容，不要加额外解释。\n\n{entries}",
            memory = current_memory,
            entries = user_text,
        );

        let request = CompletionRequest {
            messages: vec![serde_json::json!({"role": "user", "content": prompt})],
            model: String::new(),
            settings: Default::default(),
        };

        match self.provider.complete(&request).await {
            Ok(resp) => {
                let text = resp.content.unwrap_or_default();
                if text.trim().is_empty() {
                    current_memory.to_string()
                } else {
                    text
                }
            }
            Err(_) => current_memory.to_string(),
        }
    }
}
