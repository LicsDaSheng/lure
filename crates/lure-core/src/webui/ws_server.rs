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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{accept_hdr, Message};

use crate::agent::AgentLoop;
use crate::bus::InboundMessage;
use crate::memory::DreamRunner;
use crate::webui::hub::WsHub;
use crate::webui::mux::{MuxSession, TurnRunner};
use crate::webui::tokens::TokenIssuer;
use crate::webui::transcript::TranscripStore;

/// 连接读循环的轮询超时：读阻塞至多这么久即回来 drain 一次服务端推送队列，
/// 从而把 cron 推送延迟上界约束在此值内（同时保持单线程持有 socket，无并发写竞争）。
const READ_POLL: Duration = Duration::from_millis(250);

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
    hub: WsHub,
    next_conn_id: Arc<AtomicU64>,
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
            hub: WsHub::new(),
            next_conn_id: Arc::new(AtomicU64::new(1)),
            _marker: std::marker::PhantomData,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// 在线连接注册表的克隆句柄：交给 cron runner 等服务端主动推送方。
    pub fn hub(&self) -> WsHub {
        self.hub.clone()
    }

    pub fn handle_next(&mut self) -> io::Result<bool> {
        let (stream, _) = self.listener.accept()?;
        let issuer = self.issuer.clone();
        let factory = self.factory.clone();
        let transcript = self.transcript.clone();
        let hub = self.hub.clone();
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::Relaxed);
        thread::spawn(move || {
            let runner = factory.lock().expect("factory 锁中毒")();
            serve_connection(stream, runner, issuer, transcript, hub, conn_id);
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
    hub: WsHub,
    conn_id: u64,
) {
    let mut ws = match accept_hdr(
        stream,
        // tungstenite 的 accept_hdr 回调契约固定 Err 为完整 Response（拒绝握手时原样写回），
        // 无法用 Box 瘦身；错误路径为冷路径，故显式放行 result_large_err。
        #[allow(clippy::result_large_err)]
        |req: &Request, resp: Response| handshake_auth(req, resp, &issuer),
    ) {
        Ok(ws) => ws,
        Err(_) => return,
    };
    // 读超时：令读循环周期性回来 drain 服务端推送队列（见 READ_POLL）。
    // 设置失败不致命——退化为纯请求/响应，推送延迟到下次客户端活动。
    let _ = ws.get_ref().set_read_timeout(Some(READ_POLL));

    let mut mux = match transcript {
        Some(t) => MuxSession::new_with_transcript(runner, t),
        None => MuxSession::new(runner),
    };

    // 本连接的服务端推送通道：hub 持 sender，读循环 drain receiver 写回 socket。
    let (tx, rx) = mpsc::channel::<Value>();

    let ready = mux.ready_frame();
    // 订阅默认 chat_id：用户停留在新会话时，其中创建的 cron 产出可直达本连接。
    if let Some(chat_id) = frame_chat_id(&ready) {
        hub.subscribe(chat_id, conn_id, tx.clone());
    }
    if !send_json(&mut ws, &ready) {
        hub.remove_conn(conn_id);
        return;
    }

    let alive = connection_loop(&mut ws, &mut mux, &hub, conn_id, &tx, &rx);
    hub.remove_conn(conn_id);
    let _ = alive;
}

/// 连接主循环：每轮先 drain 服务端推送，再读一帧（至多阻塞 READ_POLL）。
///
/// 客户端每发一帧带 `chat_id`（attach/message）即（幂等）订阅该会话，使后续 cron
/// 推送路由到本连接。返回值仅表示循环因何结束（socket 关闭/出错），调用方据此清理。
fn connection_loop<R: TurnRunner>(
    ws: &mut tungstenite::WebSocket<TcpStream>,
    mux: &mut MuxSession<R>,
    hub: &WsHub,
    conn_id: u64,
    tx: &mpsc::Sender<Value>,
    rx: &Receiver<Value>,
) -> bool {
    loop {
        // 1) drain 服务端主动推送（cron 产出等）。
        while let Ok(pushed) = rx.try_recv() {
            if !send_json(ws, &pushed) {
                return false;
            }
        }
        // 2) 读一帧客户端入站（超时则回到步骤 1 继续 drain）。
        let text = match ws.read() {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => return true,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => return false,
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        // 客户端 attach/message 到某会话 → 订阅它，令 cron 推送可达。
        if let Some(chat_id) = frame_chat_id(&frame) {
            hub.subscribe(chat_id, conn_id, tx.clone());
        }
        mux.handle_frame(&frame, &mut |outbound| {
            let _ = send_json(ws, outbound);
        });
    }
}

/// 从帧里取合法 `chat_id`（非空字符串）。
fn frame_chat_id(frame: &Value) -> Option<&str> {
    frame
        .get("chat_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
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
