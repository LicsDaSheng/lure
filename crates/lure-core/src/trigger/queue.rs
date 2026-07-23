//! 本地 trigger 投递队列（at-least-once）。
//!
//! 对齐上游 `nanobot/triggers/local_store.py` 的投递语义（inbox → processing →
//! complete，interrupted → recover 重新入队）：
//! - enqueue：写入 pending。
//! - claim：把非忙 session 的 pending 移入 processing 并返回（一次投递）。
//! - complete：确认后从 processing 移除（happy path 恰好一次）。
//! - recover：把 processing 中未完成的投递重新入队（崩溃恢复 → at-least-once）。
//!
//! Phase 8 用内存队列建模语义；真实文件 inbox/processing 目录布局留待接入 gateway
//! 进程时补齐（见 upstream-test-ledger）。

use std::collections::VecDeque;

/// 一条待投递的本地 trigger。
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerDelivery {
    /// 投递 id。
    pub id: u64,
    /// 来源 trigger id。
    pub trigger_id: String,
    /// 目标 session key。
    pub session_key: String,
    /// 投递内容。
    pub content: String,
    /// 投递尝试次数。
    pub attempts: u32,
}

/// 本地 trigger 投递队列。
#[derive(Debug, Default)]
pub struct LocalTriggerQueue {
    pending: VecDeque<TriggerDelivery>,
    processing: Vec<TriggerDelivery>,
    next_id: u64,
}

impl LocalTriggerQueue {
    /// 新建空队列。
    pub fn new() -> Self {
        Self::default()
    }

    /// 入队一条待投递，返回其 id。
    pub fn enqueue(
        &mut self,
        trigger_id: impl Into<String>,
        session_key: impl Into<String>,
        content: impl Into<String>,
    ) -> u64 {
        self.next_id += 1;
        let delivery = TriggerDelivery {
            id: self.next_id,
            trigger_id: trigger_id.into(),
            session_key: session_key.into(),
            content: content.into(),
            attempts: 0,
        };
        self.pending.push_back(delivery);
        self.next_id
    }

    /// 认领至多 `limit` 条**非忙** session 的投递，移入 processing 并返回。
    ///
    /// `is_busy` 返回真的 session 的投递会保留在 pending（等待语义）。
    pub fn claim(&mut self, limit: usize, is_busy: impl Fn(&str) -> bool) -> Vec<TriggerDelivery> {
        let mut claimed = Vec::new();
        let mut remaining = VecDeque::new();
        while let Some(mut delivery) = self.pending.pop_front() {
            if claimed.len() < limit && !is_busy(&delivery.session_key) {
                delivery.attempts += 1;
                self.processing.push(delivery.clone());
                claimed.push(delivery);
            } else {
                remaining.push_back(delivery);
            }
        }
        self.pending = remaining;
        claimed
    }

    /// 确认一条投递已处理，从 processing 移除。返回是否存在。
    pub fn complete(&mut self, id: u64) -> bool {
        let before = self.processing.len();
        self.processing.retain(|d| d.id != id);
        self.processing.len() != before
    }

    /// 把 processing 中未完成的投递重新入队（崩溃恢复）。返回恢复条数。
    pub fn recover(&mut self) -> usize {
        let recovered = std::mem::take(&mut self.processing);
        let count = recovered.len();
        for delivery in recovered {
            self.pending.push_back(delivery);
        }
        count
    }

    /// 待投递数量。
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// 处理中数量。
    pub fn processing_len(&self) -> usize {
        self.processing.len()
    }
}

/// 便捷断言：无 session 为忙。
pub fn never_busy(_session_key: &str) -> bool {
    false
}
