//! WebUI WS transport：`webui::ws_server` 真实 WebSocket 接线 MuxSession。

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use lure_core::agent::{AgentLoop, ContextBuilder, ProgressEvent};
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;
use lure_core::webui::mux::{self, MuxSession, TurnRunner};
use lure_core::webui::tokens::TokenIssuer;
use lure_core::webui::ws_server::{AgentTurnRunner, WsServer};
use serde_json::{json, Value};
use tempfile::TempDir;
use tungstenite::{client::connect, Message};

struct ScriptedRunner;

impl TurnRunner for ScriptedRunner {
    fn run_turn(
        &mut self,
        _chat_id: &str,
        _content: &str,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<String, String> {
        on_progress(&ProgressEvent::ContentDelta { text: "你".into() });
        on_progress(&ProgressEvent::ContentDelta { text: "好".into() });
        Ok("你好".to_string())
    }
}

fn bind(
    issuer: Arc<Mutex<TokenIssuer>>,
) -> (
    WsServer<impl FnMut() -> ScriptedRunner, ScriptedRunner>,
    SocketAddr,
) {
    let server = WsServer::bind("127.0.0.1:0", || ScriptedRunner, issuer, None).unwrap();
    let addr = server.local_addr().unwrap();
    (server, addr)
}

fn connect_ws(
    addr: SocketAddr,
    query: &str,
) -> tungstenite::WebSocket<impl std::io::Read + std::io::Write> {
    let url = format!("ws://{addr}/ws{query}");
    let (ws, _) = connect(url).expect("WS 握手失败");
    ws
}

fn read_frame(ws: &mut tungstenite::WebSocket<impl std::io::Read + std::io::Write>) -> Value {
    loop {
        match ws.read().expect("读帧失败") {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("意外帧: {other:?}"),
        }
    }
}

#[test]
fn rejects_missing_or_unknown_token() {
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(3600, 16)));
    let (mut server, addr) = bind(issuer);

    // 无 token。
    let no_token = thread_connect(addr, "");
    server.handle_next().unwrap();
    assert!(no_token.join().unwrap().is_err(), "无 token 必须拒绝");

    // 未知 token。
    let bad = thread_connect(addr, "?token=nope");
    server.handle_next().unwrap();
    assert!(bad.join().unwrap().is_err(), "未知 token 必须拒绝");
}

fn thread_connect(addr: SocketAddr, query: &str) -> std::thread::JoinHandle<Result<(), String>> {
    let url = format!("ws://{addr}/ws{query}");
    std::thread::spawn(move || match connect(url) {
        Ok(_) => Ok(()),
        Err(_) => Err("rejected".to_string()),
    })
}

#[test]
fn full_turn_over_real_websocket() {
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(3600, 16)));
    let token = issuer.lock().unwrap().issue().token;
    let (mut server, addr) = bind(issuer);

    // 连接处理跑在 server 线程：accept 一条连接后服务到结束。
    let handle = std::thread::spawn(move || {
        server.handle_next().unwrap();
        server
    });

    let mut ws = connect_ws(addr, &format!("?token={token}"));

    // ready 帧。
    let ready = read_frame(&mut ws);
    assert_eq!(ready["event"], "ready");
    assert!(ready["chat_id"].as_str().unwrap().len() == 36);

    // attach。
    ws.send(Message::Text(
        json!({"type": "attach", "chat_id": "c1"})
            .to_string()
            .into(),
    ))
    .unwrap();
    let attached = read_frame(&mut ws);
    assert_eq!(attached, json!({"event": "attached", "chat_id": "c1"}));

    // message → 完整 turn 事件序列。
    ws.send(Message::Text(
        json!({"type": "message", "chat_id": "c1", "content": "hello"})
            .to_string()
            .into(),
    ))
    .unwrap();
    let names: Vec<String> = (0..6)
        .map(|_| read_frame(&mut ws)["event"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        vec![
            "goal_status",
            "delta",
            "delta",
            "message",
            "turn_end",
            "session_updated"
        ]
    );

    drop(ws);
    let _ = handle.join();
}

#[test]
fn agent_turn_runner_drives_agent_loop_streaming() {
    let dir = TempDir::new().unwrap();
    let sessions = SessionManager::new(dir.path()).unwrap();
    let agent = AgentLoop::new(
        Box::new(EchoProvider::new()),
        sessions,
        ContextBuilder::new(None),
    );
    let mut runner = AgentTurnRunner::new(agent);

    let mut deltas = Vec::new();
    let final_text = runner
        .run_turn("c1", "ping", &mut |event| {
            if let ProgressEvent::ContentDelta { text } = event {
                deltas.push(text.clone());
            }
        })
        .unwrap();
    assert_eq!(final_text, "echo: ping");

    // session 已落盘（user + assistant 两条）。
    let mut reader = SessionManager::new(dir.path()).unwrap();
    let session = reader.get_or_create("websocket:c1").unwrap();
    assert_eq!(session.messages.len(), 2);
}

#[test]
fn mux_session_accepts_agent_turn_runner_end_to_end() {
    let dir = TempDir::new().unwrap();
    let build = || {
        let sessions = SessionManager::new(dir.path()).unwrap();
        AgentTurnRunner::new(AgentLoop::new(
            Box::new(EchoProvider::new()),
            sessions,
            ContextBuilder::new(None),
        ))
    };
    let mut mux = MuxSession::new(build());
    let out = mux::collect_frames(
        &mut mux,
        &json!({"type": "message", "chat_id": "c1", "content": "ping"}),
    );
    let names: Vec<&str> = out
        .iter()
        .filter_map(|f| f.get("event").and_then(Value::as_str))
        .collect();
    assert_eq!(
        names,
        vec![
            "goal_status",
            "delta",
            "message",
            "turn_end",
            "session_updated"
        ]
    );
    assert_eq!(out[2]["text"], "echo: ping");
}
