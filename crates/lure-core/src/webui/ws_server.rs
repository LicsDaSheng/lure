//! WebUI WS transport：真实 WebSocket 接线 [`MuxSession`]。
//!
//! 独立 `TcpListener`（desktop 与 HTTP server 各绑一个 loopback 端口，bootstrap 经
//! `ws_url` 报告）。握手时校验 `?token=`（共享 [`TokenIssuer`]）；每条连接一个线程，
//! 连接内创建 [`MuxSession`]：先发 `ready`，随后文本帧进 `handle_frame`、出站事件帧写回。
//!
//! runner 由工厂在连接线程内创建（`AgentLoop` 因 `Box<dyn LlmProvider>` 无 `Send`
//! 约束不能跨线程移动）。

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{accept_hdr, Message};

use crate::agent::AgentLoop;
use crate::bus::InboundMessage;
use crate::memory::DreamRunner;
use crate::webui::mux::{MuxSession, TurnRunner};
use crate::webui::tokens::TokenIssuer;
use crate::webui::transcript::TranscripStore;

/// 把 [`AgentLoop`] 适配为 mux 的 [`TurnRunner`]（channel 固定 `websocket`）。
///
/// 含可选的 dream runner：每轮 turn 完成后按阈值触发 memory consolidation。
pub struct AgentTurnRunner {
    agent: AgentLoop,
    dream_runner: Option<Box<dyn DreamRunner>>,
    dream_threshold: usize,
}

impl AgentTurnRunner {
    /// 不挂 dream runner 的最小实例。
    pub fn new(agent: AgentLoop) -> Self {
        Self {
            agent,
            dream_runner: None,
            dream_threshold: 10,
        }
    }

    /// 挂载 dream runner 并在每轮 turn 后自动检查阈值触发。
    pub fn with_dream(mut self, runner: Box<dyn DreamRunner>, threshold: usize) -> Self {
        self.dream_runner = Some(runner);
        self.dream_threshold = threshold;
        self
    }
}
impl TurnRunner for AgentTurnRunner {
    fn run_turn(
        &mut self,
        chat_id: &str,
        content: &str,
        on_progress: &mut dyn FnMut(&crate::agent::ProgressEvent),
    ) -> Result<String, String> {
        let input = InboundMessage::new("websocket", chat_id, content);
        let outcome = self
            .agent
            .process_streaming(&input, on_progress)
            .map_err(|e| e.to_string())?;

        // 阈值触发 dream consolidation。
        if let Some(ref runner) = self.dream_runner {
            let _ = self
                .agent
                .maybe_consolidate(runner.as_ref(), self.dream_threshold);
        }

        Ok(outcome.final_content)
    }
}

/// WebUI 复用协议 WebSocket server。
pub struct WsServer<F, R>
where
    F: FnMut() -> R + Send + 'static,
    R: TurnRunner,
{
    listener: TcpListener,
    factory: Arc<Mutex<F>>,
    issuer: Arc<Mutex<TokenIssuer>>,
    transcript: Option<TranscripStore>,
    // `fn() -> R` 形态：Send/Sync 只取决于 F，与 R 无关（R 在连接线程内创建使用）。
    _marker: std::marker::PhantomData<fn() -> R>,
}

impl<F, R> WsServer<F, R>
where
    F: FnMut() -> R + Send + 'static,
    R: TurnRunner,
{
    pub fn bind(
        addr: &str,
        factory: F,
        issuer: Arc<Mutex<TokenIssuer>>,
        transcript: Option<TranscripStore>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            factory: Arc::new(Mutex::new(factory)),
            issuer,
            transcript,
            _marker: std::marker::PhantomData,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn handle_next(&mut self) -> io::Result<bool> {
        let (stream, _) = self.listener.accept()?;
        let issuer = self.issuer.clone();
        let factory = self.factory.clone();
        let transcript = self.transcript.clone();
        thread::spawn(move || {
            let runner = factory.lock().expect("factory 锁中毒")();
            serve_connection(stream, runner, issuer, transcript);
        });
        Ok(true)
    }

    pub fn serve_forever(&mut self) -> io::Result<()> {
        while self.handle_next()? {}
        Ok(())
    }
}

fn serve_connection<R: TurnRunner>(
    stream: TcpStream,
    runner: R,
    issuer: Arc<Mutex<TokenIssuer>>,
    transcript: Option<TranscripStore>,
) {
    let mut ws = match accept_hdr(stream, |req: &Request, resp: Response| {
        handshake_auth(req, resp, &issuer)
    }) {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let mut mux = match transcript {
        Some(t) => MuxSession::new_with_transcript(runner, t),
        None => MuxSession::new(runner),
    };
    if !send_json(&mut ws, &mux.ready_frame()) {
        return;
    }

    loop {
        let text = match ws.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        mux.handle_frame(&frame, &mut |outbound| {
            let _ = send_json(&mut ws, outbound);
        });
    }
}

fn send_json(ws: &mut tungstenite::WebSocket<TcpStream>, frame: &Value) -> bool {
    ws.send(Message::Text(frame.to_string().into())).is_ok()
}

#[allow(clippy::result_large_err)]
fn handshake_auth(
    req: &Request,
    resp: Response,
    issuer: &Arc<Mutex<TokenIssuer>>,
) -> Result<Response, ErrorResponse> {
    let token = req
        .uri()
        .query()
        .and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("token="))
        })
        .unwrap_or("");
    let valid = issuer.lock().expect("issuer 锁中毒").check_ws_token(token);
    if valid {
        Ok(resp)
    } else {
        Err(tungstenite::http::Response::builder()
            .status(401)
            .body(Some("unauthorized".to_string()))
            .expect("合法 401 响应"))
    }
}
