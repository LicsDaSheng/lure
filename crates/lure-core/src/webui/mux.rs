//! WebUI 复用协议会话 handler（传输无关）。
//!
//! 对齐上游 `nanobot/channels/websocket/runtime.py` 的连接生命周期与事件形状：
//! - 连接建立 → `ready`（默认 chat_id + client_id）。
//! - 入站信封 `{"type": "attach" | "new_chat" | "message", ...}` 分发为出站事件帧序列。
//! - `message` 驱动一次 agent turn：`goal_status(running)` → 流式 `delta`/`reasoning_delta`
//!   → `message`（最终回复）→ `turn_end` → `session_updated`；失败 → `error` → `turn_end`。
//!
//! 本模块不做：真实 WebSocket 连接管理（10d 接线）、workspace scope、fork_chat、
//! 媒体附件、transcript 落盘、ToolInvoked 的 tool_events 表面（见 upstream-test-ledger）。

use regex::Regex;
use serde_json::{json, Value};

use crate::agent::ProgressEvent;
use crate::webui::transcript::TranscripStore;

/// 一次 turn 的运行入口（接线层注入；真实实现适配 `AgentLoop::process_streaming`）。
pub trait TurnRunner {
    /// 同步执行一次 turn；progress 事件实时回调。
    /// 成功返回最终回复文本；失败返回错误 detail（mux 发 `error` 事件）。
    fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<String, String>;
}

/// 单条 WebUI 复用连接的生命周期状态。
pub struct MuxSession<R: TurnRunner> {
    runner: R,
    client_id: String,
    default_chat_id: String,
    transcript: Option<TranscripStore>,
}

impl<R: TurnRunner> MuxSession<R> {
    /// 建立连接：生成默认 chat_id（uuid4）与 client_id（`anon-<hex12>`，对齐上游）。
    pub fn new(runner: R) -> Self {
        let client_id = format!("anon-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        Self {
            runner,
            client_id,
            default_chat_id: uuid::Uuid::new_v4().to_string(),
            transcript: None,
        }
    }

    /// 建立连接并挂载 transcript 存储（turn 结束时自动写入）。
    pub fn new_with_transcript(runner: R, transcript: TranscripStore) -> Self {
        let client_id = format!("anon-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        Self {
            runner,
            client_id,
            default_chat_id: uuid::Uuid::new_v4().to_string(),
            transcript: Some(transcript),
        }
    }

    /// 连接建立后的首帧。
    pub fn ready_frame(&self) -> Value {
        json!({
            "event": "ready",
            "chat_id": self.default_chat_id,
            "client_id": self.client_id,
        })
    }

    /// 只读访问 runner（测试与接线层检查用）。
    pub fn runner(&self) -> &R {
        &self.runner
    }

    /// 处理一帧入站 JSON，返回按序产出的出站事件帧。
    pub fn handle_frame(&mut self, frame: &Value) -> Vec<Value> {
        let Some(frame_type) = frame.get("type").and_then(Value::as_str) else {
            return vec![error_event("invalid frame")];
        };
        match frame_type {
            "attach" => self.handle_attach(frame),
            "new_chat" => self.handle_new_chat(),
            "message" => self.handle_message(frame),
            other => vec![error_event(&format!("unknown type: {other:?}"))],
        }
    }

    fn handle_attach(&mut self, frame: &Value) -> Vec<Value> {
        match valid_chat_id(frame) {
            Some(chat_id) => vec![json!({"event": "attached", "chat_id": chat_id})],
            None => vec![error_event("invalid chat_id")],
        }
    }

    fn handle_new_chat(&mut self) -> Vec<Value> {
        let chat_id = uuid::Uuid::new_v4().to_string();
        vec![
            json!({"event": "attached", "chat_id": chat_id}),
            json!({"event": "session_updated", "chat_id": chat_id, "scope": "metadata"}),
        ]
    }

    fn handle_message(&mut self, frame: &Value) -> Vec<Value> {
        let Some(chat_id) = valid_chat_id(frame) else {
            return vec![error_event("invalid chat_id")];
        };
        let content = frame.get("content").and_then(Value::as_str).unwrap_or("");
        if content.trim().is_empty() {
            return vec![error_event("missing content")];
        }

        let mut out = vec![json!({
            "event": "goal_status",
            "chat_id": chat_id,
            "status": "running",
            "started_at": chrono::Utc::now().timestamp(),
        })];

        let result = self.runner.run_turn(chat_id, content, &mut |event| {
            let mapped = match event {
                ProgressEvent::ContentDelta { text } => {
                    Some(json!({"event": "delta", "chat_id": chat_id, "text": text}))
                }
                ProgressEvent::ReasoningDelta { text } => {
                    Some(json!({"event": "reasoning_delta", "chat_id": chat_id, "text": text}))
                }
                // TurnStarted/ToolInvoked/FinalResponse 暂无独立 WS 表面（见模块文档）。
                _ => None,
            };
            if let Some(frame) = mapped {
                out.push(frame);
            }
        });

        match result {
            Ok(text) => {
                // 成功 turn → 写入 transcript（用 session key，对齐 http_server 读取路径）。
                if let Some(ref mut t) = self.transcript {
                    let session_key = format!("websocket:{chat_id}");
                    let _ = t.append_turn(&session_key, content, &text);
                }
                out.push(json!({"event": "message", "chat_id": chat_id, "text": text}));
                out.push(json!({"event": "turn_end", "chat_id": chat_id}));
                out.push(json!({"event": "session_updated", "chat_id": chat_id}));
            }
            Err(detail) => {
                out.push(json!({"event": "error", "chat_id": chat_id, "detail": detail}));
                out.push(json!({"event": "turn_end", "chat_id": chat_id}));
            }
        }
        out
    }
}

/// 提取并校验 chat_id：上游 `_CHAT_ID_RE = ^[A-Za-z0-9_:-]{1,64}$`。
fn valid_chat_id(frame: &Value) -> Option<&str> {
    let chat_id = frame.get("chat_id")?.as_str()?;
    let re = Regex::new(r"^[A-Za-z0-9_:-]{1,64}$").expect("合法正则");
    re.is_match(chat_id).then_some(chat_id)
}

fn error_event(detail: &str) -> Value {
    json!({"event": "error", "detail": detail})
}
