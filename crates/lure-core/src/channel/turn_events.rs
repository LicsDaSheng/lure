//! Turn 事件按 `turn_id` 路由（Stage 5）。
//!
//! 共享调度核心（`AgentLoopScheduler`）在入站消息带 `turn_id` 元数据时，把
//! `process_streaming` 的进度事件（delta/reasoning）与最终产出经 [`TurnEventRegistry`]
//! 投递回发起连接。发起方（`WebSocketChannel` 的 turn 侧 [`BusTurnRunner`]）在发布前
//! 注册 receiver、turn 结束后注销。多连接并发时互不串扰（turn_id 唯一）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::UnboundedSender;

/// 一轮 turn 的流式事件（调度核心 → 发起连接）。
#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    /// 内容增量（delta 帧）。
    Delta(String),
    /// 推理增量（reasoning_delta 帧）。
    Reasoning(String),
    /// 调用了工具（仅展示工具名，不改变工具执行契约）。
    Tool(String),
    /// 最终回复文本。
    Final(String),
    /// turn 失败。
    Error(String),
    /// 收尾信号（无论成败）。
    Done,
}

/// 按 turn_id 路由 turn 事件的注册表（线程安全、可 clone 共享）。
#[derive(Clone, Default)]
pub struct TurnEventRegistry(Arc<Mutex<HashMap<String, UnboundedSender<TurnEvent>>>>);

impl TurnEventRegistry {
    /// 新建空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册某 turn_id 的投递通道。
    pub fn register(&self, turn_id: &str, tx: UnboundedSender<TurnEvent>) {
        self.0
            .lock()
            .expect("TurnEventRegistry 锁中毒")
            .insert(turn_id.to_string(), tx);
    }

    /// 投递一条事件到该 turn_id 的通道；无接收方返回 `false`。
    pub fn route(&self, turn_id: &str, event: TurnEvent) -> bool {
        if let Some(tx) = self
            .0
            .lock()
            .expect("TurnEventRegistry 锁中毒")
            .get(turn_id)
        {
            let _ = tx.send(event);
            true
        } else {
            false
        }
    }

    /// 注销某 turn_id 的投递通道（turn 结束后调用）。
    pub fn unregister(&self, turn_id: &str) {
        self.0
            .lock()
            .expect("TurnEventRegistry 锁中毒")
            .remove(turn_id);
    }
}
