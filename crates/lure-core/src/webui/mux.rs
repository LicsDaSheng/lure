//! WebUI 复用协议会话 handler（传输无关）。
//!
//! 对齐上游 `nanobot/channels/websocket/runtime.py` 的连接生命周期与事件形状：
//! - 连接建立 → `ready`（默认 chat_id + client_id）。
//! - 入站信封 `{"type": "attach" | "new_chat" | "message", ...}` 分发为出站事件帧。
//! - 每帧即时经 `on_outbound` 回调发出（支持流式 delta 实时推送）。
//! - `message` 驱动一次 agent turn：`goal_status(running)` → 流式 `delta`/`reasoning_delta`
//!   → `message`（最终回复）→ `turn_end` → `session_updated`；失败 → `error` → `turn_end`。
//!
//! 本模块不做：真实 WebSocket 连接管理、workspace scope、fork_chat、
//! 媒体附件、transcript 落盘、ToolInvoked 的 tool_events 表面。

use regex::Regex;
use serde_json::{json, Value};

use crate::agent::ProgressEvent;
use crate::webui::transcript::TranscripStore;

/// 一次 turn 的运行入口（接线层注入；真实实现适配 `AgentLoop::process_streaming`）。
/// 异步版本（Stage 2）：turn 执行为 async，连接线程内经 `Handle::block_on` 驱动。
#[async_trait::async_trait]
pub trait TurnRunner {
    async fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut (dyn FnMut(ProgressEvent) + Send),
    ) -> Result<String, String>;
}

/// `Box<dyn TurnRunner + Send>` 转发到 vtable（axum server 用 trait object 去泛型）。
#[async_trait::async_trait]
impl TurnRunner for Box<dyn TurnRunner + Send> {
    async fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut (dyn FnMut(ProgressEvent) + Send),
    ) -> Result<String, String> {
        (**self).run_turn(chat_id, content, on_progress).await
    }
}

/// 单条 WebUI 复用连接的生命周期状态。
pub struct MuxSession<R: TurnRunner> {
    runner: R,
    /// 连接线程内驱动 async turn 的 runtime 句柄（非 runtime 线程可 `block_on`）。
    runtime: tokio::runtime::Handle,
    client_id: String,
    default_chat_id: String,
    transcript: Option<TranscripStore>,
}

impl<R: TurnRunner> MuxSession<R> {
    pub fn new(runner: R, runtime: tokio::runtime::Handle) -> Self {
        let client_id = format!("anon-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        Self {
            runner,
            runtime,
            client_id,
            default_chat_id: uuid::Uuid::new_v4().to_string(),
            transcript: None,
        }
    }

    pub fn new_with_transcript(
        runner: R,
        transcript: TranscripStore,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        let client_id = format!("anon-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        Self {
            runner,
            runtime,
            client_id,
            default_chat_id: uuid::Uuid::new_v4().to_string(),
            transcript: Some(transcript),
        }
    }

    pub fn ready_frame(&self) -> Value {
        json!({"event": "ready", "chat_id": self.default_chat_id, "client_id": self.client_id})
    }

    pub fn runner(&self) -> &R {
        &self.runner
    }

    /// 处理一帧入站 JSON；每帧出站事件即时经 `on_outbound` 发出。
    pub fn handle_frame(&mut self, frame: &Value, on_outbound: &mut (dyn FnMut(&Value) + Send)) {
        let Some(frame_type) = frame.get("type").and_then(Value::as_str) else {
            on_outbound(&error_event("invalid frame"));
            return;
        };
        match frame_type {
            "attach" => self.handle_attach(frame, on_outbound),
            "new_chat" => self.handle_new_chat(frame, on_outbound),
            "message" => self.handle_message(frame, on_outbound),
            other => on_outbound(&error_event(&format!("unknown type: {other:?}"))),
        }
    }

    fn handle_attach(&mut self, frame: &Value, on_outbound: &mut (dyn FnMut(&Value) + Send)) {
        match valid_chat_id(frame) {
            Some(chat_id) => on_outbound(&json!({"event": "attached", "chat_id": chat_id})),
            None => on_outbound(&error_event("invalid chat_id")),
        }
    }

    fn handle_new_chat(&mut self, frame: &Value, on_outbound: &mut (dyn FnMut(&Value) + Send)) {
        let chat_id = uuid::Uuid::new_v4().to_string();
        let session_key = format!("websocket:{chat_id}");

        // 立即登记一个可列出的空 session（对齐上游 new_chat → persist_scope →
        // `get_or_create("websocket:{chat_id}")`）。否则该会话在首条消息落盘前不出现在
        // `/api/sessions`，前端导航到新会话后因列表查不到而弹回欢迎页（stays on 初始页）。
        self.ensure_listable_session(&session_key);

        on_outbound(&json!({"event": "attached", "chat_id": chat_id}));

        // session_updated 回带客户端声明的 workspace_scope（对齐上游 runtime.py：
        // `session_updated(scope="metadata", workspace_scope=scope.payload())`）。
        let mut updated = json!({
            "event": "session_updated",
            "chat_id": chat_id,
            "scope": "metadata",
        });
        if let Some(scope) = frame.get("workspace_scope") {
            updated["workspace_scope"] = scope.clone();
        }
        on_outbound(&updated);
    }

    /// 持久化一个空 session 使其可被 `/api/sessions` 列出（best-effort，失败不影响握手）。
    ///
    /// session store 与 transcript 同处 workspace（`webui/` 的父目录）；无 transcript
    /// （纯内存测试装配）时跳过。
    fn ensure_listable_session(&self, session_key: &str) {
        let Some(workspace) = self.transcript.as_ref().and_then(|t| t.workspace_dir()) else {
            return;
        };
        let Ok(mut manager) = crate::session::SessionManager::new(workspace) else {
            return;
        };
        if manager.get_or_create(session_key).is_ok() {
            let _ = manager.save(session_key, false);
        }
    }

    fn handle_message(&mut self, frame: &Value, on_outbound: &mut (dyn FnMut(&Value) + Send)) {
        let Some(chat_id_val) = valid_chat_id(frame) else {
            on_outbound(&error_event("invalid chat_id"));
            return;
        };
        let chat_id = chat_id_val.to_string();
        let content = frame.get("content").and_then(Value::as_str).unwrap_or("");
        if content.trim().is_empty() {
            on_outbound(&error_event("missing content"));
            return;
        }

        // goal_status(running) 首帧立即发出。
        on_outbound(&json!({
            "event": "goal_status",
            "chat_id": chat_id,
            "status": "running",
            "started_at": chrono::Utc::now().timestamp(),
        }));

        // 流式 delta 经 progress 回调即时发出（不缓冲到 turn 结束）。
        // 连接线程非 runtime worker，经 Handle::block_on 驱动 async turn。
        let result = crate::runtime::block_on(
            &self.runtime,
            self.runner.run_turn(&chat_id, content, &mut |event| {
                let mapped = match event {
                    ProgressEvent::ContentDelta { text } => {
                        Some(json!({"event": "delta", "chat_id": chat_id, "text": text}))
                    }
                    ProgressEvent::ReasoningDelta { text } => Some(json!({
                        "event": "reasoning_delta",
                        "chat_id": chat_id,
                        "text": text
                    })),
                    _ => None,
                };
                if let Some(frame) = mapped {
                    on_outbound(&frame);
                }
            }),
        );

        match result {
            Ok(text) => {
                if let Some(ref mut t) = self.transcript {
                    let session_key = format!("websocket:{chat_id}");
                    let _ = t.append_turn(&session_key, content, &text);
                }
                on_outbound(&json!({"event": "message", "chat_id": chat_id, "text": text}));
                on_outbound(&json!({"event": "turn_end", "chat_id": chat_id}));
                on_outbound(&json!({"event": "session_updated", "chat_id": chat_id}));
            }
            Err(detail) => {
                on_outbound(&json!({"event": "error", "chat_id": chat_id, "detail": detail}));
                on_outbound(&json!({"event": "turn_end", "chat_id": chat_id}));
            }
        }
    }
}

fn valid_chat_id(frame: &Value) -> Option<&str> {
    let chat_id = frame.get("chat_id")?.as_str()?;
    let re = Regex::new(r"^[A-Za-z0-9_:-]{1,64}$").expect("合法正则");
    re.is_match(chat_id).then_some(chat_id)
}

fn error_event(detail: &str) -> Value {
    json!({"event": "error", "detail": detail})
}

/// 便捷方法：收集所有出站帧到一个 Vec（仅供测试使用）。
pub fn collect_frames<R: TurnRunner>(session: &mut MuxSession<R>, frame: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    session.handle_frame(frame, &mut |f| out.push(f.clone()));
    out
}
