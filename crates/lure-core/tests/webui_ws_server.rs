//! WebUI WS transport：`webui::axum_server`（axum WebSocket）真实接线 MuxSession。

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use lure_core::agent::{AgentLoop, ContextBuilder, ProgressEvent};
use lure_core::provider::EchoProvider;
use lure_core::session::SessionManager;
use lure_core::webui::axum_server::{
    AgentTurnRunner, StaticAssets, WebuiServer, WebuiServerConfig,
};
use lure_core::webui::mux::{self, MuxSession, TurnRunner};
use lure_core::webui::tokens::TokenIssuer;
use lure_core::webui::transcript::TranscripStore;
use serde_json::{json, Value};
use tempfile::TempDir;
use tungstenite::{client::connect, Message};

struct ScriptedRunner;

#[async_trait::async_trait]
impl TurnRunner for ScriptedRunner {
    async fn run_turn(
        &mut self,
        _chat_id: &str,
        _content: &str,
        on_progress: &mut (dyn FnMut(ProgressEvent) + Send),
    ) -> Result<String, String> {
        on_progress(ProgressEvent::ContentDelta { text: "你".into() });
        on_progress(ProgressEvent::ContentDelta { text: "好".into() });
        Ok("你好".to_string())
    }
}

/// 空静态资源表（WS 测试不需要 HTTP 资源）。
struct NoAssets;
impl StaticAssets for NoAssets {
    fn asset(&self, _path: &str) -> Option<(Vec<u8>, &'static str)> {
        None
    }
}

/// 绑定 axum WebUI server（HTTP + WS 同一端口），返回 server 与地址。
async fn bind(issuer: Arc<Mutex<TokenIssuer>>) -> (WebuiServer, SocketAddr) {
    let dir = TempDir::new().unwrap();
    let transcript = TranscripStore::new(dir.path().join("webui")).unwrap();
    let server = WebuiServer::bind(
        "127.0.0.1:0",
        NoAssets,
        WebuiServerConfig {
            workspace: dir.path().to_path_buf(),
            config_path: dir.path().join("config.yaml"),
            model_name: None,
            ws_path: "/ws".to_string(),
            ws_url: String::new(),
            token_ttl_secs: 3600,
        },
        lure_core::config::Config::default(),
        issuer,
        transcript,
        || ScriptedRunner,
    )
    .await
    .unwrap();
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

fn thread_connect(addr: SocketAddr, query: &str) -> std::thread::JoinHandle<Result<(), String>> {
    let url = format!("ws://{addr}/ws{query}");
    std::thread::spawn(move || match connect(url) {
        Ok(_) => Ok(()),
        Err(_) => Err("rejected".to_string()),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_missing_or_unknown_token() {
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(3600, 16)));
    let (server, addr) = bind(issuer).await;
    let _serve = tokio::spawn(async move {
        let _ = server.serve_forever().await;
    });
    // 等待 listener 就绪：重试连接直到成功/拒绝（serve task 启动有微小延迟）。
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 无 token。
    let no_token = thread_connect(addr, "");
    assert!(no_token.join().unwrap().is_err(), "无 token 必须拒绝");

    // 未知 token。
    let bad = thread_connect(addr, "?token=nope");
    assert!(bad.join().unwrap().is_err(), "未知 token 必须拒绝");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_turn_over_real_websocket() {
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(3600, 16)));
    let token = issuer.lock().unwrap().issue().token;
    let (server, addr) = bind(issuer).await;
    let _serve = tokio::spawn(async move {
        let _ = server.serve_forever().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hub_push_reaches_attached_connection() {
    // 服务端主动推送（cron 定时产出）→ 已 attach 该会话的在线连接实时收到，无需刷新。
    let issuer = Arc::new(Mutex::new(TokenIssuer::new(3600, 16)));
    let token = issuer.lock().unwrap().issue().token;
    let (server, addr) = bind(issuer).await;
    let hub = server.hub();
    let _serve = tokio::spawn(async move {
        let _ = server.serve_forever().await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut ws = connect_ws(addr, &format!("?token={token}"));
    let _ready = read_frame(&mut ws);

    // attach 到 cron 会话 chat_id；读到 attached 即证明服务端已处理并订阅。
    ws.send(Message::Text(
        json!({"type": "attach", "chat_id": "cron-chat"})
            .to_string()
            .into(),
    ))
    .unwrap();
    let attached = read_frame(&mut ws);
    assert_eq!(
        attached,
        json!({"event": "attached", "chat_id": "cron-chat"})
    );

    // 模拟 cron 推送；订阅可见性理论上有微小竞态，重试直到投递。
    let frame = json!({"event": "message", "chat_id": "cron-chat", "text": "⏰ 定时产出"});
    let mut delivered = 0;
    for _ in 0..50 {
        delivered = hub.push("cron-chat", &frame);
        if delivered > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(delivered, 1, "推送应投递到已 attach 的在线连接");

    // 客户端在读超时 drain 循环内收到推送帧（延迟上界约 READ_POLL）。
    let pushed = read_frame(&mut ws);
    assert_eq!(pushed["event"], "message");
    assert_eq!(pushed["chat_id"], "cron-chat");
    assert_eq!(pushed["text"], "⏰ 定时产出");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_turn_runner_drives_agent_loop_streaming() {
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
        .await
        .unwrap();
    assert_eq!(final_text, "echo: ping");

    // session 已落盘（user + assistant 两条）。
    let mut reader = SessionManager::new(dir.path()).unwrap();
    let session = reader.get_or_create("websocket:c1").unwrap();
    assert_eq!(session.messages.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mux_session_accepts_agent_turn_runner_end_to_end() {
    let dir = TempDir::new().unwrap();
    let build = || {
        let sessions = SessionManager::new(dir.path()).unwrap();
        AgentTurnRunner::new(AgentLoop::new(
            Box::new(EchoProvider::new()),
            sessions,
            ContextBuilder::new(None),
        ))
    };
    let mut mux = MuxSession::new(build(), tokio::runtime::Handle::current());
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
