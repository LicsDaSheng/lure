//! WebSocket 渠道的 turn 执行侧（Stage 5）。
//!
//! 对齐上游 `channels/websocket/runtime.py` 的「浏览器 WS → bus → 单实例 AgentLoop」：
//! [`BusTurnRunner`] 替代每连接独立 AgentLoop——发布 `InboundMessage` 进共享 `AsyncBus`
//! （携带 `turn_id`），经 `TurnEventRegistry` 等待调度核心流式 turn 事件，把 delta 转发给
//! 连接协议层（mux）的 progress 回调并返回最终文本。连接协议层（attach/new_chat/事件
//! 序列）保持 [`crate::webui::mux`] 不变，本模块只换 turn 执行来源。

use serde_json::json;

use crate::agent::ProgressEvent;
use crate::bus::AsyncBusSender;
use crate::channel::turn_events::{TurnEvent, TurnEventRegistry};
use crate::webui::mux::TurnRunner;

/// 经共享调度核心执行一轮 turn 的渠道 runner。
pub struct BusTurnRunner {
    bus: AsyncBusSender,
    events: TurnEventRegistry,
}

impl BusTurnRunner {
    /// 新建：`bus` 为共享 AsyncBus，`events` 为 turn 事件路由注册表。
    pub fn new(bus: AsyncBusSender, events: TurnEventRegistry) -> Self {
        Self { bus, events }
    }
}

#[async_trait::async_trait]
impl TurnRunner for BusTurnRunner {
    async fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut (dyn FnMut(ProgressEvent) + Send),
    ) -> Result<String, String> {
        let turn_id = uuid::Uuid::new_v4().to_string();
        let (evt_tx, mut evt_rx) = tokio::sync::mpsc::unbounded_channel::<TurnEvent>();
        self.events.register(&turn_id, evt_tx);

        let mut msg = crate::bus::InboundMessage::new("websocket", chat_id, content);
        msg.metadata.insert("turn_id".into(), json!(turn_id));
        if let Err(e) = self.bus.publish(msg).await {
            self.events.unregister(&turn_id);
            return Err(format!("bus 投递失败: {e}"));
        }

        let mut final_text = None;
        let outcome = loop {
            match evt_rx.recv().await {
                None => break Err("turn 事件通道关闭".to_string()),
                Some(TurnEvent::Delta(text)) => {
                    on_progress(ProgressEvent::ContentDelta { text });
                }
                Some(TurnEvent::Reasoning(text)) => {
                    on_progress(ProgressEvent::ReasoningDelta { text });
                }
                Some(TurnEvent::Tool(name)) => {
                    on_progress(ProgressEvent::ToolInvoked { name });
                }
                Some(TurnEvent::Final(text)) => final_text = Some(text),
                Some(TurnEvent::Error(detail)) => break Err(detail),
                Some(TurnEvent::Done) => {
                    break final_text
                        .map(Ok)
                        .unwrap_or_else(|| Err("turn 未产出回复".to_string()));
                }
            }
        };
        self.events.unregister(&turn_id);
        outcome
    }
}
