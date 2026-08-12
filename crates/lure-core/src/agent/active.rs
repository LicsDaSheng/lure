//! 跨实例「活跃 session」注册表（Stage 4 defer 协调）。
//!
//! 对齐上游 `automation coordinator.defer_if_active` 的让位语义：cron/trigger 在目标
//! session 已有活跃 turn 时让位。当前架构 chat turn 走 per-connection AgentLoop（Stage 5
//! channel 化前不进共享 bus），故需一个**跨实例**共享注册表：per-connection 聊天侧在
//! turn 期间 `mark`，cron submit 侧 `is_busy` 判断是否让位。Stage 5 后 chat 并入 bus，
//! 该注册表由调度核心内部 pending 队列取代，本模块退役。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// 活跃 session key 集合（`channel:chat_id` 形式），线程安全、可 clone 共享。
#[derive(Clone, Default)]
pub struct SessionBusy(Arc<Mutex<HashSet<String>>>);

impl SessionBusy {
    /// 新建空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 标记 session 活跃（turn 开始）。
    pub fn mark(&self, session_key: &str) {
        self.0
            .lock()
            .expect("SessionBusy 锁中毒")
            .insert(session_key.to_string());
    }

    /// 取消标记（turn 结束）。
    pub fn unmark(&self, session_key: &str) {
        self.0
            .lock()
            .expect("SessionBusy 锁中毒")
            .remove(session_key);
    }

    /// 该 session 当前是否活跃。
    pub fn is_busy(&self, session_key: &str) -> bool {
        self.0
            .lock()
            .expect("SessionBusy 锁中毒")
            .contains(session_key)
    }
}
