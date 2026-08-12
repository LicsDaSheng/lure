//! Subagent 后台执行（Stage 6）。
//!
//! 对齐上游 `nanobot/agent/subagent.py` 的 `spawn`/`_run_subagent`/`_announce_result`：
//! - `run`：fresh AgentLoop 跑一轮 subagent turn，返回最终文本（`_run_subagent`）。
//! - `spawn`：登记进 [`SubagentRegistry`]（持 abort 句柄）→ 后台 task 执行 `run` →
//!   完成后 announce 经 `AsyncBus` 回灌（metadata 标记，不触发新 turn）→ mark_done + finish。
//! - `cancel_session`：abort 该 session 下所有运行中 task（exec 级联终止，`/stop` 原语）。

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::agent::subagent::{SubagentRegistry, SubagentStatus};
use crate::agent::AgentLoop;
use crate::bus::{AsyncBusSender, InboundMessage};

/// 后台 subagent 执行器：持有共享登记表、bus、runtime 句柄与 fresh-agent 工厂。
pub struct SubagentRunner {
    registry: Arc<Mutex<SubagentRegistry>>,
    handle: tokio::runtime::Handle,
    bus: AsyncBusSender,
    /// 每个 subagent 构建一个独立 AgentLoop（对齐上游 `_run_subagent` 的 fresh loop）。
    make_agent: Arc<dyn Fn() -> AgentLoop + Send + Sync>,
}

impl SubagentRunner {
    /// 新建：`make_agent` 为 fresh-agent 工厂（desktop 侧用 `build_agent_loop` 包装）。
    pub fn new(
        handle: tokio::runtime::Handle,
        bus: AsyncBusSender,
        registry: Arc<Mutex<SubagentRegistry>>,
        make_agent: Arc<dyn Fn() -> AgentLoop + Send + Sync>,
    ) -> Self {
        Self {
            registry,
            handle,
            bus,
            make_agent,
        }
    }

    /// 同步式执行一轮 subagent turn（fresh AgentLoop），返回最终文本。
    pub async fn run(&self, session_key: &str, task: &str) -> Result<String, String> {
        let mut agent = (self.make_agent)();
        let msg = InboundMessage::new("subagent", session_key, task);
        let outcome = agent.process(&msg).await.map_err(|e| e.to_string())?;
        Ok(outcome.final_content)
    }

    /// 后台 spawn：登记 → 起 task 执行 → 完成后 announce 经 bus 回灌 → 清理。
    ///
    /// 返回 task id。announce 为 `InboundMessage`（channel `subagent`，带
    /// `subagent_result`/`subagent_task_id`/`subagent_error` 元数据），消费方按标记
    /// 处理为「结果回灌」而非新 turn。
    pub fn spawn(
        &self,
        session_key: &str,
        task: &str,
        label: Option<&str>,
    ) -> Result<String, String> {
        let task_id = uuid::Uuid::new_v4().to_string();
        {
            let mut reg = self.registry.lock().expect("SubagentRegistry 锁中毒");
            let status =
                SubagentStatus::new(&task_id, label.unwrap_or("subagent"), task, now_epoch_f64());
            reg.register(&task_id, Some(session_key), status);
        }

        let bus = self.bus.clone();
        let registry = self.registry.clone();
        let make_agent = self.make_agent.clone();
        let session_key = session_key.to_string();
        let task = task.to_string();
        let tid = task_id.clone();
        let join = self.handle.spawn(async move {
            let result = {
                let mut agent = (make_agent)();
                let msg = InboundMessage::new("subagent", &session_key, &task);
                agent.process(&msg).await.map_err(|e| e.to_string())
            };
            // announce 经 bus 回灌。
            let (content, error) = match result {
                Ok(outcome) => (outcome.final_content, None),
                Err(e) => (String::new(), Some(e.to_string())),
            };
            let mut announce = InboundMessage::new("subagent", &session_key, content);
            // 回灌到父 session（subagent 自身 turn 落在 `subagent:{key}`，结果送达父会话）。
            announce.session_key_override = Some(session_key.clone());
            announce
                .metadata
                .insert("subagent_result".into(), json!(true));
            announce
                .metadata
                .insert("subagent_task_id".into(), json!(tid));
            if let Some(err) = error {
                announce
                    .metadata
                    .insert("subagent_error".into(), json!(err));
            }
            let _ = bus.publish(announce).await;

            let mut reg = registry.lock().expect("SubagentRegistry 锁中毒");
            reg.mark_done(&tid);
            reg.finish(&tid);
        });
        // 挂 abort 句柄（供 /stop 级联终止）。
        self.registry
            .lock()
            .expect("SubagentRegistry 锁中毒")
            .attach_abort(&task_id, join.abort_handle());
        Ok(task_id)
    }

    /// 取消某 session 下所有运行中 subagent（abort + 清理），返回取消数。
    pub fn cancel_session(&self, session_key: &str) -> usize {
        self.registry
            .lock()
            .expect("SubagentRegistry 锁中毒")
            .cancel_by_session(session_key)
    }
}

/// 当前 epoch 秒（f64，对齐上游 started_at 的 time.time()）。
fn now_epoch_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
