//! 最小**同步** HTTP server：把传输无关的 api 表面接到真实端点。
//!
//! 对齐上游 `nanobot/api/server.py` 的路由与契约，但收敛为同步单请求循环：
//! - `POST /v1/chat/completions`：解析 → 校验 model → 调 runner → 非流式 JSON 或
//!   SSE 流（`stream:true`）。
//! - `GET /health`：返回 `{"status":"ok"}`（不鉴权）。
//!
//! 解析/校验/响应构造复用 [`crate::api`] 现有函数，本模块只做薄传输层。
//!
//! SSE 走逐 token 路径：`stream:true` 时驱动 runner 的内容增量回调，每段文本一条
//! 内容 chunk（跨 tool 轮次不关流），收尾 finish chunk 与 `[DONE]`。
//!
//! Phase 9 不做：multipart/media 上传、并发 session lock、请求超时。

use std::net::SocketAddr;

use serde_json::{Map, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::agent::AgentLoop;
use crate::api::openai::{
    api_session_key, authorize, chat_completion_response, error_body, generate_completion_id,
    models_response, parse_chat_request, sse_content_chunk, sse_finish_chunk, validate_model,
    SSE_DONE,
};
use crate::bus::InboundMessage;

/// API 固定 chat id（对齐上游 `API_CHAT_ID`）。
const API_CHAT_ID: &str = "default";

/// 传输无关的 chat runner 抽象：HTTP 层依赖它而非具体 [`AgentLoop`]，便于用
/// fake/echo provider 驱动的 loop 做端到端测试，也便于后续替换编排实现。
pub trait ChatRunner {
    /// 处理一次 chat：给定 session key 与用户文本，返回最终 assistant 文本。
    fn run(&mut self, session_key: &str, text: &str) -> Result<String, ChatRunError>;

    /// 处理一次 chat 并**逐段**回调内容增量：每产生一段文本即调用 `on_delta`，
    /// 供 SSE 层逐 token 推送。跨 tool 轮次的多段内容全部经此回调，流保持打开。
    ///
    /// 默认实现回退到非流式 [`run`](Self::run)，把整段最终文本作为**单个**增量回调一次——
    /// 让未覆盖流式的 runner 仍可被 SSE 路径统一驱动。真正逐 token 的 runner（如
    /// [`AgentLoop`]）覆盖此方法。
    fn run_streaming(
        &mut self,
        session_key: &str,
        text: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<(), ChatRunError> {
        let content = self.run(session_key, text)?;
        if !content.is_empty() {
            on_delta(&content);
        }
        Ok(())
    }
}

/// runner 处理失败（映射为 500）。
#[derive(Debug, Clone)]
pub struct ChatRunError(pub String);

impl std::fmt::Display for ChatRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chat runner 错误: {}", self.0)
    }
}

impl std::error::Error for ChatRunError {}

impl ChatRunner for AgentLoop {
    fn run(&mut self, session_key: &str, text: &str) -> Result<String, ChatRunError> {
        let mut inbound = InboundMessage::new("api", API_CHAT_ID, text);
        inbound.session_key_override = Some(session_key.to_string());
        self.process(&inbound)
            .map(|outcome| outcome.final_content)
            .map_err(|e| ChatRunError(e.to_string()))
    }

    fn run_streaming(
        &mut self,
        session_key: &str,
        text: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<(), ChatRunError> {
        let mut inbound = InboundMessage::new("api", API_CHAT_ID, text);
        inbound.session_key_override = Some(session_key.to_string());
        self.process_streaming(&inbound, on_delta)
            .map(|_| ())
            .map_err(|e| ChatRunError(e.to_string()))
    }
}

/// server 运行期配置。
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// 对外报告并用于 model 校验的模型名。
    pub model: String,
    /// 可选 API key：为空/`None` 放行，否则要求 `Bearer <key>`。
    pub api_key: Option<String>,
}

/// 绑定到一个端口的最小 chat server。
pub struct ChatServer<R: ChatRunner> {
    server: Server,
    runner: R,
    config: ServerConfig,
}

impl<R: ChatRunner> ChatServer<R> {
    /// 在 `addr`（如 `127.0.0.1:0` 表示随机端口）上绑定 server。
    pub fn bind(addr: &str, runner: R, config: ServerConfig) -> std::io::Result<Self> {
        let server = Server::http(addr).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, e.to_string())
        })?;
        Ok(Self {
            server,
            runner,
            config,
        })
    }

    /// 实际监听地址（随机端口时用于拿到分配到的端口）。
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.server.server_addr().to_ip().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "非 IP 监听地址")
        })
    }

    /// 阻塞接收并处理下一个请求；连接关闭返回 `false`。
    pub fn handle_next(&mut self) -> std::io::Result<bool> {
        match self.server.recv() {
            Ok(request) => {
                self.handle(request)?;
                Ok(true)
            }
            Err(e) => Err(e),
        }
    }

    /// 持续处理请求直到出错。
    pub fn serve_forever(&mut self) -> std::io::Result<()> {
        while self.handle_next()? {}
        Ok(())
    }

    /// 路由分发。
    fn handle(&mut self, request: Request) -> std::io::Result<()> {
        let method = request.method().clone();
        let path = request.url().split('?').next().unwrap_or("").to_string();
        match (&method, path.as_str()) {
            (Method::Get, "/health") => {
                respond_json(request, 200, serde_json::json!({"status": "ok"}))
            }
            (Method::Get, "/v1/models") => self.handle_models(request),
            (Method::Post, "/v1/chat/completions") => self.handle_chat(request),
            _ => respond_json(
                request,
                404,
                error_body(404, "Not Found", "invalid_request_error"),
            ),
        }
    }

    /// 处理 `/v1/models`：鉴权 → 返回单条已配置模型。
    fn handle_models(&mut self, request: Request) -> std::io::Result<()> {
        let auth = header_value(&request, "Authorization");
        if let Err(e) = authorize(self.config.api_key.as_deref(), auth.as_deref()) {
            return respond_json(request, e.status, e.body());
        }
        respond_json(request, 200, models_response(&self.config.model))
    }

    /// 处理 chat completions：鉴权 → 解析 → 校验 model → 调 runner → 响应。
    fn handle_chat(&mut self, mut request: Request) -> std::io::Result<()> {
        // 鉴权（复用 api::authorize）。
        let auth = header_value(&request, "Authorization");
        if let Err(e) = authorize(self.config.api_key.as_deref(), auth.as_deref()) {
            return respond_json(request, e.status, e.body());
        }

        // 读取并解析 JSON body。
        let mut raw = String::new();
        request.as_reader().read_to_string(&mut raw)?;
        let body: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => {
                return respond_json(
                    request,
                    400,
                    error_body(400, "Invalid JSON body", "invalid_request_error"),
                )
            }
        };

        // 解析 + 校验 model（复用 api 表面）。
        let parsed = match parse_chat_request(&body) {
            Ok(parsed) => parsed,
            Err(e) => return respond_json(request, e.status, e.body()),
        };
        if let Err(e) = validate_model(parsed.model.as_deref(), &self.config.model) {
            return respond_json(request, e.status, e.body());
        }

        let session_id = body.get("session_id").and_then(Value::as_str);
        let session_key = api_session_key(session_id);

        // 调用注入的 runner。流式与非流式走不同回调路径。
        if parsed.stream {
            respond_sse(
                request,
                &mut self.runner,
                &session_key,
                &parsed.text,
                &self.config.model,
            )
        } else {
            match self.runner.run(&session_key, &parsed.text) {
                Ok(content) => respond_json(
                    request,
                    200,
                    chat_completion_response(&content, &self.config.model, &Map::new()),
                ),
                Err(_) => respond_json(
                    request,
                    500,
                    error_body(500, "Internal server error", "server_error"),
                ),
            }
        }
    }
}

/// 读取某个请求头的值（大小写不敏感）。
fn header_value(request: &Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

/// 以 JSON 应答给定状态码。
fn respond_json(request: Request, status: u16, body: Value) -> std::io::Result<()> {
    let data = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    let response = Response::from_string(data)
        .with_status_code(status)
        .with_header(json_header());
    request.respond(response)
}

/// 以 SSE 流应答：驱动 runner 的逐 token 回调，每个内容增量写一条内容 chunk，
/// 收尾追加 finish chunk 与 `[DONE]`。所有 chunk 共享同一 `chatcmpl-` id；跨 tool
/// 轮次的多段内容全部写入同一响应体，流不在中途关闭。
///
/// 传输仍为同步单次应答：增量先累积进响应体，末尾一次性 `respond`（tiny_http 的
/// `Response::from_string` 语义）。runner 出错且尚未产生任何 chunk 时回退 500。
fn respond_sse<R: ChatRunner>(
    request: Request,
    runner: &mut R,
    session_key: &str,
    text: &str,
    model: &str,
) -> std::io::Result<()> {
    let chunk_id = generate_completion_id();
    let mut body = String::new();
    let stream_result = runner.run_streaming(session_key, text, &mut |delta| {
        if !delta.is_empty() {
            body.push_str(&sse_content_chunk(delta, model, &chunk_id));
        }
    });

    if stream_result.is_err() {
        // 尚未写出任何帧（响应体只在末尾发送），可安全回退结构化 500。
        return respond_json(
            request,
            500,
            error_body(500, "Internal server error", "server_error"),
        );
    }

    body.push_str(&sse_finish_chunk(model, &chunk_id));
    body.push_str(SSE_DONE);

    let response = Response::from_string(body)
        .with_status_code(200)
        .with_header(sse_header())
        .with_header(no_cache_header());
    request.respond(response)
}

fn json_header() -> Header {
    Header::from_bytes(b"Content-Type".as_slice(), b"application/json".as_slice())
        .expect("合法 header")
}

fn sse_header() -> Header {
    Header::from_bytes(b"Content-Type".as_slice(), b"text/event-stream".as_slice())
        .expect("合法 header")
}

fn no_cache_header() -> Header {
    Header::from_bytes(b"Cache-Control".as_slice(), b"no-cache".as_slice()).expect("合法 header")
}
