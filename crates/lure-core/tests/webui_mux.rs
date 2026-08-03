//! WebUI 复用协议会话 handler（`webui::mux`）——传输无关。
//!
//! 映射上游 `nanobot/channels/websocket/runtime.py` 的连接生命周期：
//! ready → attach/new_chat/message → delta/message/turn_end/error 事件序列。

use lure_core::agent::ProgressEvent;
use lure_core::webui::mux::{self, MuxSession, TurnRunner};
use serde_json::{json, Value};

/// 录制 progress 回调的 fake runner：按脚本产出事件并返回固定终文本。
struct ScriptedRunner {
    progress: Vec<ProgressEvent>,
    final_text: String,
    fail: Option<String>,
    /// 记录收到的 (chat_id, content)。
    calls: Vec<(String, String)>,
}

impl ScriptedRunner {
    fn ok(progress: Vec<ProgressEvent>, final_text: &str) -> Self {
        Self {
            progress,
            final_text: final_text.to_string(),
            fail: None,
            calls: Vec::new(),
        }
    }

    fn failing(detail: &str) -> Self {
        Self {
            progress: Vec::new(),
            final_text: String::new(),
            fail: Some(detail.to_string()),
            calls: Vec::new(),
        }
    }
}

impl TurnRunner for ScriptedRunner {
    fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<String, String> {
        self.calls.push((chat_id.to_string(), content.to_string()));
        for event in &self.progress {
            on_progress(event);
        }
        match &self.fail {
            Some(detail) => Err(detail.clone()),
            None => Ok(self.final_text.clone()),
        }
    }
}

fn events(frames: &[Value]) -> Vec<&str> {
    frames
        .iter()
        .filter_map(|f| f.get("event").and_then(Value::as_str))
        .collect()
}

#[test]
fn ready_frame_announces_default_chat_and_client() {
    let session = MuxSession::new(ScriptedRunner::ok(vec![], ""));
    let ready = session.ready_frame();
    assert_eq!(ready["event"], "ready");
    let chat_id = ready["chat_id"].as_str().unwrap();
    // 上游默认 chat_id 是 uuid4 字符串。
    assert_eq!(chat_id.len(), 36);
    assert!(chat_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    let client_id = ready["client_id"].as_str().unwrap();
    assert!(client_id.starts_with("anon-"));
}

#[test]
fn attach_valid_chat_id_acknowledges() {
    let mut session = MuxSession::new(ScriptedRunner::ok(vec![], ""));
    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "attach", "chat_id": "chat-1"}),
    );
    assert_eq!(out, vec![json!({"event": "attached", "chat_id": "chat-1"})]);
}

#[test]
fn attach_invalid_chat_id_errors() {
    let mut session = MuxSession::new(ScriptedRunner::ok(vec![], ""));
    for bad in [
        json!({"type": "attach", "chat_id": "has space"}),
        json!({"type": "attach", "chat_id": "x".repeat(65)}),
        json!({"type": "attach", "chat_id": 42}),
        json!({"type": "attach"}),
    ] {
        let out = mux::collect_frames(&mut session, &bad);
        assert_eq!(
            out,
            vec![json!({"event": "error", "detail": "invalid chat_id"})],
            "frame: {bad}"
        );
    }
}

#[test]
fn new_chat_provisions_fresh_id_and_marks_metadata() {
    let mut session = MuxSession::new(ScriptedRunner::ok(vec![], ""));
    let default_id = session.ready_frame()["chat_id"]
        .as_str()
        .unwrap()
        .to_string();

    let out = mux::collect_frames(&mut session, &json!({"type": "new_chat"}));
    assert_eq!(events(&out), vec!["attached", "session_updated"]);
    let new_id = out[0]["chat_id"].as_str().unwrap();
    assert_eq!(new_id.len(), 36);
    assert_ne!(new_id, default_id);
    assert_eq!(out[1]["chat_id"], new_id);
    assert_eq!(out[1]["scope"], "metadata");
}

#[test]
fn new_chat_registers_listable_session_and_echoes_workspace_scope() {
    use lure_core::session::SessionManager;
    use lure_core::webui::list_webui_sessions;
    use lure_core::webui::transcript::TranscripStore;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let workspace = dir.path().to_path_buf();
    let transcript = TranscripStore::new(workspace.join("webui")).unwrap();
    let mut session = MuxSession::new_with_transcript(ScriptedRunner::ok(vec![], ""), transcript);

    let scope = json!({
        "access_mode": "full",
        "project_name": "workspace",
        "project_path": "/Users/scottlee/.lure/workspace",
        "restrict_to_workspace": false,
    });
    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "new_chat", "workspace_scope": scope}),
    );
    assert_eq!(events(&out), vec!["attached", "session_updated"]);
    let new_id = out[0]["chat_id"].as_str().unwrap().to_string();

    // session_updated 回带客户端 workspace_scope（对齐上游）。
    assert_eq!(out[1]["workspace_scope"], scope);

    // 关键回归：新会话在首条消息前即出现在 `/api/sessions`，否则前端导航后弹回欢迎页。
    let mut manager = SessionManager::new(&workspace).unwrap();
    let keys: Vec<String> = list_webui_sessions(&mut manager)
        .into_iter()
        .map(|row| row.key)
        .collect();
    assert!(
        keys.contains(&format!("websocket:{new_id}")),
        "新会话应可被列出: {keys:?}"
    );
}

#[test]
fn message_requires_valid_chat_id_and_content() {
    let mut session = MuxSession::new(ScriptedRunner::ok(vec![], ""));
    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "message", "chat_id": "!!", "content": "hi"}),
    );
    assert_eq!(
        out,
        vec![json!({"event": "error", "detail": "invalid chat_id"})]
    );

    let out = mux::collect_frames(&mut session, &json!({"type": "message", "chat_id": "c1"}));
    assert_eq!(
        out,
        vec![json!({"event": "error", "detail": "missing content"})]
    );

    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "message", "chat_id": "c1", "content": "   "}),
    );
    assert_eq!(
        out,
        vec![json!({"event": "error", "detail": "missing content"})]
    );
}

#[test]
fn message_streams_delta_then_message_turn_end_and_session_updated() {
    let runner = ScriptedRunner::ok(
        vec![
            ProgressEvent::ContentDelta { text: "你".into() },
            ProgressEvent::ContentDelta { text: "好".into() },
        ],
        "你好",
    );
    let mut session = MuxSession::new(runner);
    // 不先 attach：首次 message 自动可用（上游 auto-attach 语义）。
    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "message", "chat_id": "c1", "content": "hello"}),
    );

    assert_eq!(
        events(&out),
        vec![
            "goal_status",
            "delta",
            "delta",
            "message",
            "turn_end",
            "session_updated"
        ]
    );
    assert_eq!(out[0]["chat_id"], "c1");
    assert_eq!(out[0]["status"], "running");
    assert!(out[0]["started_at"].as_i64().unwrap() > 0);
    assert_eq!(
        out[1],
        json!({"event": "delta", "chat_id": "c1", "text": "你"})
    );
    assert_eq!(
        out[2],
        json!({"event": "delta", "chat_id": "c1", "text": "好"})
    );
    assert_eq!(
        out[3],
        json!({"event": "message", "chat_id": "c1", "text": "你好"})
    );
    assert_eq!(out[4], json!({"event": "turn_end", "chat_id": "c1"}));
    assert_eq!(out[5], json!({"event": "session_updated", "chat_id": "c1"}));
}

#[test]
fn message_maps_reasoning_delta_events() {
    let runner = ScriptedRunner::ok(
        vec![
            ProgressEvent::ReasoningDelta {
                text: "思考".into(),
            },
            ProgressEvent::ContentDelta { text: "答".into() },
        ],
        "答",
    );
    let mut session = MuxSession::new(runner);
    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "message", "chat_id": "c1", "content": "q"}),
    );
    assert_eq!(
        events(&out),
        vec![
            "goal_status",
            "reasoning_delta",
            "delta",
            "message",
            "turn_end",
            "session_updated"
        ]
    );
    assert_eq!(out[1]["text"], "思考");
}

#[test]
fn message_runner_failure_emits_error_then_turn_end() {
    let mut session = MuxSession::new(ScriptedRunner::failing("provider down"));
    let out = mux::collect_frames(
        &mut session,
        &json!({"type": "message", "chat_id": "c1", "content": "hi"}),
    );
    assert_eq!(events(&out), vec!["goal_status", "error", "turn_end"]);
    assert_eq!(out[1]["chat_id"], "c1");
    assert_eq!(out[1]["detail"], "provider down");
    // 失败 turn 不刷新 session 列表（无 message 落盘）。
    assert!(!events(&out).contains(&"session_updated"));
}

#[test]
fn unknown_frame_type_errors() {
    let mut session = MuxSession::new(ScriptedRunner::ok(vec![], ""));
    let out = mux::collect_frames(&mut session, &json!({"type": "teleport", "chat_id": "c1"}));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["event"], "error");
    assert!(out[0]["detail"].as_str().unwrap().contains("unknown type"));
}

#[test]
fn runner_receives_chat_id_and_content() {
    let runner = ScriptedRunner::ok(vec![], "ok");
    let mut session = MuxSession::new(runner);
    mux::collect_frames(
        &mut session,
        &json!({"type": "message", "chat_id": "room-9", "content": "ping"}),
    );
    assert_eq!(
        session.runner().calls,
        vec![("room-9".to_string(), "ping".to_string())]
    );
}
