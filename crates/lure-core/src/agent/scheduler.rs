//! AgentLoop 异步调度核心（Stage 1）。
//!
//! 对齐上游 `nanobot/agent/loop.py::run()` 的调度语义（loop.py:1161-1254）：
//! 常驻消费异步 bus，每条消息 spawn 独立 dispatch task；同 session 已有活跃任务时
//! 后续消息进入该 session 的 pending 队列（不丢、不乱序、不另起竞争任务）；`/stop`
//! 取消该 session 的活跃任务并清空 pending；消费超时执行心跳钩子（对齐上游
//! `wait_for(consume_inbound, 1.0)` 的超时即心跳）；automation（cron/trigger）消息
//! 在聊天 turn 活跃时同样排队（让位不抢占）。
//!
//! Stage 1 约束：turn 执行为同步 `AgentLoop::process`（包在 tokio Mutex 内），
//! 同 session 串行、不同 session 经任务调度并发；`/stop` 为硬取消（abort 任务）。
//! Stage 2（turn 异步化）后：await 点协作式调度、pending 注入当前 turn、取消精细化。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

use crate::agent::loop_run::AgentLoop;
use crate::bus::{AsyncBusReceiver, InboundMessage};

/// 停止控制命令（对齐上游 /stop 优先级命令）。
pub const STOP_COMMAND: &str = "/stop";

/// 每 session 活跃任务与 pending 队列的共享状态。
#[derive(Default)]
struct SchedulerState {
    /// session → 活跃 dispatch 任务句柄（同 session 串行链至多一个）。
    active: HashMap<String, JoinHandle<()>>,
    /// session → pending 队列（活跃 turn 期间的后续消息）。
    pending: HashMap<String, VecDeque<InboundMessage>>,
}

/// 调度器配置。
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 消费超时（心跳周期）。对齐上游 `wait_for(consume_inbound, 1.0)` 的 1s。
    pub consume_timeout: Duration,
    /// 每 session pending 队列容量上限；满时丢弃新消息并静默（对齐上游 QueueFull
    /// fallback 的丢弃侧；Stage 2 接显式 backpressure）。
    pub pending_capacity: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            consume_timeout: Duration::from_secs(1),
            pending_capacity: 32,
        }
    }
}

/// turn 完成钩子：`(session_key, 用户消息内容)`。测试/观测用。
pub type TurnHook = dyn Fn(&str, &str) + Send + Sync;

/// turn 完成且成功的回调：`(&InboundMessage, 回复文本)`。
///
/// Stage 4：cron turn 经共享调度核心执行后，投递层（transcript 落库 + hub 推送 +
/// `record_run`）由此钩子拿到消息与回复文本；chat 并入 bus（Stage 5）后同样复用。
pub type CompletedHook = dyn Fn(&InboundMessage, &str) + Send + Sync;

/// 心跳钩子：消费超时（空闲）时调用。
pub type HeartbeatHook = dyn Fn() + Send + Sync;

/// 异步调度器：常驻 run 循环 + 每消息 dispatch task。
pub struct AgentLoopScheduler {
    agent: Arc<AsyncMutex<AgentLoop>>,
    state: Arc<AsyncMutex<SchedulerState>>,
    config: SchedulerConfig,
    on_turn: Option<Arc<TurnHook>>,
    on_completed: Option<Arc<CompletedHook>>,
    heartbeat: Option<Arc<HeartbeatHook>>,
}

impl AgentLoopScheduler {
    /// 进入 builder（配置钩子与超时）。
    pub fn builder(agent: AgentLoop) -> SchedulerBuilder {
        SchedulerBuilder {
            agent,
            config: SchedulerConfig::default(),
            on_turn: None,
            on_completed: None,
            heartbeat: None,
        }
    }

    /// 常驻调度循环：消费 bus 直至所有 sender drop（返回）。
    ///
    /// - 超时（空闲）→ 心跳钩子。
    /// - `/stop` 控制消息 → 取消该 session 活跃任务并清空 pending。
    /// - 同 session 活跃 → 消息入 pending 队列。
    /// - 否则 → spawn dispatch task 执行 turn，完成后串行消费同 session pending。
    pub async fn run(self, mut rx: AsyncBusReceiver) {
        loop {
            match tokio::time::timeout(self.config.consume_timeout, rx.consume()).await {
                Err(_) => {
                    if let Some(hb) = &self.heartbeat {
                        hb();
                    }
                    continue;
                }
                Ok(None) => break,
                Ok(Some(msg)) => {
                    let key = msg.session_key();
                    if msg.content.trim() == STOP_COMMAND {
                        self.cancel_session(&key).await;
                        continue;
                    }
                    let mut st = self.state.lock().await;
                    if st.active.contains_key(&key) {
                        let queue = st.pending.entry(key.clone()).or_default();
                        if queue.len() < self.config.pending_capacity {
                            queue.push_back(msg);
                        }
                        // 队列满：丢弃（Stage 2 接 backpressure）。
                        continue;
                    }
                    drop(st);
                    self.spawn_dispatch(key, msg).await;
                }
            }
        }
    }

    /// 取消某 session 的全部活跃工作：abort 活跃任务、清空 pending。
    async fn cancel_session(&self, key: &str) {
        let handle = {
            let mut st = self.state.lock().await;
            st.pending.remove(key);
            st.active.remove(key)
        };
        if let Some(handle) = handle {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// spawn 一个 dispatch task：执行 turn（tokio Mutex 串行化），完成后串行
    /// 消费同 session 的 pending 队列；无 pending 时移除活跃标记并结束。
    async fn spawn_dispatch(&self, key: String, msg: InboundMessage) {
        let agent = self.agent.clone();
        let state = self.state.clone();
        let on_turn = self.on_turn.clone();
        let on_completed = self.on_completed.clone();
        let task_key = key.clone();
        let handle = tokio::spawn(async move {
            let mut current = Some(msg);
            while let Some(msg) = current.take() {
                let outcome = {
                    let mut agent = agent.lock().await;
                    agent.process(&msg).await
                };
                if let Ok(turn) = &outcome {
                    if let Some(hook) = &on_turn {
                        hook(&msg.session_key(), &msg.content);
                    }
                    if let Some(hook) = &on_completed {
                        hook(&msg, &turn.final_content);
                    }
                }
                // Stage 1：turn 错误仅丢弃（Stage 2 接错误传播/重试策略）。
                // 取同 session 下一条 pending 继续（串行链）；空则清理活跃标记并结束。
                current = {
                    let mut st = state.lock().await;
                    match st.pending.get_mut(&task_key).and_then(VecDeque::pop_front) {
                        Some(next) => Some(next),
                        None => {
                            st.active.remove(&task_key);
                            None
                        }
                    }
                };
            }
        });
        // 登记活跃句柄（供 /stop 取消）。task 可能已结束（极快 turn）→ 直接清理。
        let mut st = self.state.lock().await;
        if handle.is_finished() {
            st.active.remove(&key);
        } else {
            st.active.insert(key, handle);
        }
    }
}

/// 调度器构建器。
pub struct SchedulerBuilder {
    agent: AgentLoop,
    config: SchedulerConfig,
    on_turn: Option<Arc<TurnHook>>,
    on_completed: Option<Arc<CompletedHook>>,
    heartbeat: Option<Arc<HeartbeatHook>>,
}

impl SchedulerBuilder {
    /// 消费超时（心跳周期）。
    pub fn consume_timeout(mut self, timeout: Duration) -> Self {
        self.config.consume_timeout = timeout;
        self
    }

    /// 每 session pending 队列容量。
    pub fn pending_capacity(mut self, capacity: usize) -> Self {
        self.config.pending_capacity = capacity;
        self
    }

    /// turn 完成钩子：`(session_key, 用户消息内容)`。
    pub fn on_turn<F>(mut self, hook: F) -> Self
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        self.on_turn = Some(Arc::new(hook));
        self
    }

    /// turn 完成且成功钩子：`(&InboundMessage, 回复文本)`。
    pub fn on_completed<F>(mut self, hook: F) -> Self
    where
        F: Fn(&InboundMessage, &str) + Send + Sync + 'static,
    {
        self.on_completed = Some(Arc::new(hook));
        self
    }

    /// 心跳钩子：空闲（消费超时）时调用。
    pub fn heartbeat<F>(mut self, hook: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.heartbeat = Some(Arc::new(hook));
        self
    }

    /// 构建调度器。
    pub fn build(self) -> AgentLoopScheduler {
        AgentLoopScheduler {
            agent: Arc::new(AsyncMutex::new(self.agent)),
            state: Arc::new(AsyncMutex::new(SchedulerState::default())),
            config: self.config,
            on_turn: self.on_turn,
            on_completed: self.on_completed,
            heartbeat: self.heartbeat,
        }
    }
}
