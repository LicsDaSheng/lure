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
use crate::webui::mux::{MuxSession, TurnRunner};
use crate::webui::tokens::TokenIssuer;

/// 把 [`AgentLoop`] 适配为 mux 的 [`TurnRunner`]（channel 固定 `websocket`）。
pub struct AgentTurnRunner {
    agent: AgentLoop,
}

impl AgentTurnRunner {
    /// 包装一个构建好的 agent loop。
    pub fn new(agent: AgentLoop) -> Self {
        Self { agent }
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
        self.agent
            .process_streaming(&input, on_progress)
            .map(|outcome| outcome.final_content)
            .map_err(|e| e.to_string())
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
    _marker: std::marker::PhantomData<R>,
}

impl<F, R> WsServer<F, R>
where
    F: FnMut() -> R + Send + 'static,
    R: TurnRunner,
{
    /// 绑定地址；`issuer` 与 HTTP server 共享（bootstrap 签发的 token 在此校验）。
    pub fn bind(addr: &str, factory: F, issuer: Arc<Mutex<TokenIssuer>>) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            factory: Arc::new(Mutex::new(factory)),
            issuer,
            _marker: std::marker::PhantomData,
        })
    }

    /// 实际监听地址。
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// 接受一条连接并派生服务线程（立即返回，不等待连接结束）。
    pub fn handle_next(&mut self) -> io::Result<bool> {
        let (stream, _) = self.listener.accept()?;
        let issuer = self.issuer.clone();
        let factory = self.factory.clone();
        thread::spawn(move || {
            let runner = factory.lock().expect("factory 锁中毒")();
            serve_connection(stream, runner, issuer);
        });
        Ok(true)
    }

    /// 持续接受连接直到出错。
    pub fn serve_forever(&mut self) -> io::Result<()> {
        while self.handle_next()? {}
        Ok(())
    }
}

fn serve_connection<R: TurnRunner>(stream: TcpStream, runner: R, issuer: Arc<Mutex<TokenIssuer>>) {
    let mut ws = match accept_hdr(stream, |req: &Request, resp: Response| {
        handshake_auth(req, resp, &issuer)
    }) {
        Ok(ws) => ws,
        Err(_) => return, // 握手失败（含 token 拒绝），连接已被 tungstenite 关闭
    };

    let mut mux = MuxSession::new(runner);
    if !send_json(&mut ws, &mux.ready_frame()) {
        return;
    }

    loop {
        let text = match ws.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => continue, // Ping/Pong/Binary：忽略（ping 由 tungstenite 自动回应）
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            continue; // 非 JSON 帧忽略（对齐上游 legacy 帧静默路径）
        };
        for outbound in mux.handle_frame(&frame) {
            if !send_json(&mut ws, &outbound) {
                return;
            }
        }
    }
}

fn send_json(ws: &mut tungstenite::WebSocket<TcpStream>, frame: &Value) -> bool {
    ws.send(Message::Text(frame.to_string().into())).is_ok()
}

// ErrorResponse 体积由 tungstenite Callback 签名决定，无法装箱。
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
