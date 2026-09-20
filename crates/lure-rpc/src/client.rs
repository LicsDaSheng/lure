use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use lure_core::LureEvent;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{Instant, MissedTickBehavior};

use crate::RpcError;
use crate::capture::StdoutCapture;
use crate::jsonl::read_json_lines_with_capture;
use crate::normalize::normalize_event;
use crate::protocol::{RequestContext, RpcCommand, RpcImage, RpcModel, RpcSessionState};

const DEFAULT_FRAME_LIMIT: usize = 16 * 1024 * 1024;
const STDERR_EVENT_LIMIT: usize = 512;

#[derive(Debug, Clone)]
pub struct PiProcessConfig {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub handshake_timeout: Duration,
    pub command_timeout: Duration,
    pub frame_limit: usize,
    /// 临时采集：设置后把 Pi stdout 的原始行写入该文件，用于录制 mock 数据。
    pub stdout_capture: Option<PathBuf>,
}

impl PiProcessConfig {
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>, working_directory: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            working_directory: working_directory.into(),
            handshake_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(10),
            frame_limit: DEFAULT_FRAME_LIMIT,
            stdout_capture: None,
        }
    }

    /// 打开 stdout 原样采集，写入给定文件。
    #[must_use]
    pub fn with_stdout_capture(mut self, path: impl Into<PathBuf>) -> Self {
        self.stdout_capture = Some(path.into());
        self
    }

    #[must_use]
    pub fn for_working_directory(working_directory: impl Into<PathBuf>) -> Self {
        let executable =
            std::env::var_os("LURE_PI_PATH").map_or_else(|| PathBuf::from("pi"), PathBuf::from);
        Self::new(executable, working_directory)
    }
}

#[derive(Debug, Clone)]
pub struct PiRpcClient {
    commands: mpsc::Sender<ActorCommand>,
    events: broadcast::Sender<LureEvent>,
    command_timeout: Duration,
}

#[derive(Debug)]
enum ActorCommand {
    Request {
        body: Value,
        context: RequestContext,
        timeout: Duration,
        reply: oneshot::Sender<Result<Value, RpcError>>,
    },
    Notify {
        body: Value,
        reply: oneshot::Sender<Result<(), RpcError>>,
    },
    Stop {
        reply: oneshot::Sender<Result<(), RpcError>>,
    },
}

#[derive(Debug)]
enum SystemMessage {
    ReaderFailed(RpcError),
    StdoutClosed,
    Stderr(String),
}

#[derive(Debug)]
struct PendingRequest {
    command: String,
    context: RequestContext,
    deadline: Instant,
    reply: oneshot::Sender<Result<Value, RpcError>>,
}

impl PiRpcClient {
    /// 启动 Pi RPC 子进程并完成 `get_state` 握手。
    ///
    /// # Errors
    ///
    /// 工作目录无效、Pi 无法启动、握手失败或协议错误时返回错误。
    pub async fn connect(mut config: PiProcessConfig) -> Result<(Self, RpcSessionState), RpcError> {
        config.working_directory = canonical_working_directory(&config.working_directory)?;
        verify_pi(&config.executable).await?;

        let mut child = Command::new(&config.executable)
            .args(["--mode", "rpc"])
            .current_dir(&config.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| RpcError::SpawnFailed(error.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RpcError::SpawnFailed("无法打开 Pi stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RpcError::SpawnFailed("无法打开 Pi stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RpcError::SpawnFailed("无法打开 Pi stderr".into()))?;

        let (command_tx, command_rx) = mpsc::channel(32);
        let (event_tx, _) = broadcast::channel(256);
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (system_tx, system_rx) = mpsc::unbounded_channel();

        let reader_system_tx = system_tx.clone();
        let frame_limit = config.frame_limit;
        let capture = match &config.stdout_capture {
            Some(path) => Some(StdoutCapture::open(path).await?),
            None => None,
        };
        tokio::spawn(async move {
            let mut capture = capture;
            let result =
                read_json_lines_with_capture(stdout, frame_limit, frame_tx, capture.as_mut()).await;
            if let Some(failure) = capture.as_mut().and_then(StdoutCapture::take_failure) {
                let _ = reader_system_tx.send(SystemMessage::Stderr(format!(
                    "stdout 采集已中断：{failure}"
                )));
            }
            match result {
                Ok(()) => {
                    let _ = reader_system_tx.send(SystemMessage::StdoutClosed);
                }
                Err(error) => {
                    let _ = reader_system_tx.send(SystemMessage::ReaderFailed(error));
                }
            }
        });

        tokio::spawn(read_stderr(stderr, system_tx));
        tokio::spawn(run_actor(
            child,
            stdin,
            command_rx,
            frame_rx,
            system_rx,
            event_tx.clone(),
        ));

        let client = Self {
            commands: command_tx,
            events: event_tx,
            command_timeout: config.command_timeout,
        };

        let state = client.read_state(config.handshake_timeout).await;
        let state = match state {
            Ok(state) => state,
            Err(error) => {
                let _ = client.stop().await;
                return Err(error);
            }
        };
        Ok((client, state))
    }

    async fn read_state(&self, timeout: Duration) -> Result<RpcSessionState, RpcError> {
        let response = self
            .request(json!({"type":"get_state"}), RequestContext::Plain, timeout)
            .await?;
        serde_json::from_value(response["data"].clone())
            .map_err(|error| RpcError::Protocol(format!("get_state 响应无效：{error}")))
    }

    /// 在当前 Pi RPC 进程中创建空白 session，并返回新的会话状态。
    ///
    /// # Errors
    ///
    /// Pi 拒绝创建、扩展取消操作或返回无效状态时返回错误。
    pub async fn new_session(&self) -> Result<RpcSessionState, RpcError> {
        let response = self
            .request(
                json!({"type":"new_session"}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        if response
            .pointer("/data/cancelled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(RpcError::CommandRejected {
                command: "new_session".into(),
                message: "新建会话已取消".into(),
            });
        }
        self.read_state(self.command_timeout).await
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LureEvent> {
        self.events.subscribe()
    }

    /// 发送文本提示词并等待 Pi 接受。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用、请求超时或命令被拒绝时返回错误。
    pub async fn prompt(&self, message: impl Into<String>) -> Result<(), RpcError> {
        let message = message.into();
        self.request(
            json!({"type":"prompt","message":message}),
            RequestContext::Prompt(message),
            self.command_timeout,
        )
        .await?;
        Ok(())
    }

    /// 发送带图片的提示词并等待 Pi 接受。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用、请求超时或命令被拒绝时返回错误。
    pub async fn prompt_with_images(
        &self,
        message: impl Into<String>,
        images: Vec<RpcImage>,
    ) -> Result<(), RpcError> {
        let message = message.into();
        self.request(
            json!({"type":"prompt","message":message,"images":images}),
            RequestContext::Prompt(message),
            self.command_timeout,
        )
        .await?;
        Ok(())
    }

    /// 获取 Pi 当前可用模型。
    ///
    /// # Errors
    ///
    /// Pi 返回无效响应或请求失败时返回错误。
    pub async fn get_available_models(&self) -> Result<Vec<RpcModel>, RpcError> {
        let response = self
            .request(
                json!({"type":"get_available_models"}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        let data = &response["data"];
        let models = data.get("models").unwrap_or(data).clone();
        serde_json::from_value(models)
            .map_err(|error| RpcError::Protocol(format!("get_available_models 响应无效：{error}")))
    }

    /// 切换当前模型。
    ///
    /// # Errors
    ///
    /// Pi 拒绝模型或返回无效响应时返回错误。
    pub async fn set_model(&self, provider: &str, model_id: &str) -> Result<RpcModel, RpcError> {
        let response = self
            .request(
                json!({"type":"set_model","provider":provider,"modelId":model_id}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        let candidate = response["data"]
            .get("model")
            .unwrap_or(&response["data"])
            .clone();
        serde_json::from_value(candidate).or_else(|_| {
            Ok(RpcModel {
                provider: provider.to_owned(),
                id: model_id.to_owned(),
            })
        })
    }

    /// 切换当前思考强度。
    ///
    /// # Errors
    ///
    /// Pi 拒绝级别时返回错误。
    pub async fn set_thinking_level(&self, level: &str) -> Result<String, RpcError> {
        let response = self
            .request(
                json!({"type":"set_thinking_level","level":level}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        Ok(response
            .pointer("/data/thinkingLevel")
            .and_then(Value::as_str)
            .unwrap_or(level)
            .to_owned())
    }

    /// 获取 Pi 公开的扩展命令、提示词和 skills。
    ///
    /// # Errors
    ///
    /// Pi 返回无效响应或请求失败时返回错误。
    pub async fn get_commands(&self) -> Result<Vec<RpcCommand>, RpcError> {
        let response = self
            .request(
                json!({"type":"get_commands"}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        let data = &response["data"];
        let commands = data.get("commands").unwrap_or(data).clone();
        serde_json::from_value(commands)
            .map_err(|error| RpcError::Protocol(format!("get_commands 响应无效：{error}")))
    }

    /// 回应 Pi extension UI 请求。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用或写入失败时返回错误。
    pub async fn respond_to_extension(
        &self,
        request_id: &str,
        value: Option<Value>,
        cancelled: bool,
    ) -> Result<(), RpcError> {
        let mut body = json!({
            "type":"extension_ui_response",
            "id":request_id,
            "cancelled":cancelled,
        });
        if let Some(value) = value {
            body["value"] = value;
        }
        self.notify(body).await
    }

    /// 中止当前 Pi 运行。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用、请求超时或命令被拒绝时返回错误。
    pub async fn abort(&self) -> Result<(), RpcError> {
        self.request(
            json!({"type":"abort"}),
            RequestContext::Plain,
            self.command_timeout,
        )
        .await?;
        Ok(())
    }

    /// 停止 Pi 子进程。
    ///
    /// # Errors
    ///
    /// actor 仍存活但无法停止子进程时返回错误。
    pub async fn stop(&self) -> Result<(), RpcError> {
        let (reply, response) = oneshot::channel();
        if self
            .commands
            .send(ActorCommand::Stop { reply })
            .await
            .is_err()
        {
            return Ok(());
        }
        response.await.unwrap_or(Ok(()))
    }

    async fn notify(&self, body: Value) -> Result<(), RpcError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(ActorCommand::Notify { body, reply })
            .await
            .map_err(|_| RpcError::ActorStopped)?;
        response.await.map_err(|_| RpcError::ActorStopped)?
    }

    async fn request(
        &self,
        body: Value,
        context: RequestContext,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(ActorCommand::Request {
                body,
                context,
                timeout,
                reply,
            })
            .await
            .map_err(|_| RpcError::ActorStopped)?;
        response.await.map_err(|_| RpcError::ActorStopped)?
    }
}

fn canonical_working_directory(path: &Path) -> Result<PathBuf, RpcError> {
    let canonical = path
        .canonicalize()
        .map_err(|_| RpcError::InvalidWorkingDirectory(path.to_path_buf()))?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(RpcError::InvalidWorkingDirectory(path.to_path_buf()))
    }
}

async fn verify_pi(executable: &Path) -> Result<(), RpcError> {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(executable).arg("--version").output(),
    )
    .await
    .map_err(|_| RpcError::PiNotFound(executable.display().to_string()))?;
    let output = result.map_err(|error| RpcError::PiNotFound(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RpcError::PiNotFound(executable.display().to_string()))
    }
}

async fn read_stderr<R>(reader: R, sender: mpsc::UnboundedSender<SystemMessage>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let message: String = line.chars().take(STDERR_EVENT_LIMIT).collect();
                if sender.send(SystemMessage::Stderr(message)).is_err() {
                    return;
                }
            }
            Ok(None) => return,
            Err(error) => {
                let _ = sender.send(SystemMessage::Stderr(format!("stderr 读取失败：{error}")));
                return;
            }
        }
    }
}

async fn run_actor(
    mut child: Child,
    mut stdin: ChildStdin,
    mut commands: mpsc::Receiver<ActorCommand>,
    mut frames: mpsc::UnboundedReceiver<Value>,
    mut system: mpsc::UnboundedReceiver<SystemMessage>,
    events: broadcast::Sender<LureEvent>,
) {
    let mut pending = HashMap::<String, PendingRequest>::new();
    let mut next_request_id = 1_u64;
    let mut ticker = tokio::time::interval(Duration::from_millis(25));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else {
                    terminate_child(&mut child).await;
                    fail_pending(&mut pending);
                    return;
                };
                match command {
                    ActorCommand::Request { mut body, context, timeout, reply } => {
                        let id = next_request_id.to_string();
                        next_request_id += 1;
                        let command_name = body["type"].as_str().unwrap_or("unknown").to_owned();
                        body["id"] = Value::String(id.clone());
                        match write_frame(&mut stdin, &body).await {
                            Ok(()) => {
                                pending.insert(id, PendingRequest {
                                    command: command_name,
                                    context,
                                    deadline: Instant::now() + timeout,
                                    reply,
                                });
                            }
                            Err(error) => {
                                let _ = reply.send(Err(error));
                            }
                        }
                    }
                    ActorCommand::Notify { body, reply } => {
                        let result = write_frame(&mut stdin, &body).await;
                        let _ = reply.send(result);
                    }
                    ActorCommand::Stop { reply } => {
                        terminate_child(&mut child).await;
                        fail_pending(&mut pending);
                        let _ = reply.send(Ok(()));
                        return;
                    }
                }
            }
            frame = frames.recv() => {
                if let Some(frame) = frame {
                    handle_frame(frame, &mut pending, &mut stdin, &events).await;
                }
            }
            message = system.recv() => {
                if let Some(message) = message {
                    match message {
                        SystemMessage::ReaderFailed(error) => {
                            let _ = events.send(LureEvent::ProtocolError { message: error.to_string() });
                            terminate_child(&mut child).await;
                            fail_pending_with_protocol(&mut pending, &error.to_string());
                            return;
                        }
                        SystemMessage::StdoutClosed => {
                            if let Ok(Some(status)) = child.try_wait() {
                                let _ = events.send(LureEvent::ProcessExited { code: status.code() });
                                fail_pending(&mut pending);
                            } else {
                                let message = "Pi RPC stdout 意外关闭".to_owned();
                                let _ = events.send(LureEvent::ProtocolError { message: message.clone() });
                                terminate_child(&mut child).await;
                                fail_pending_with_protocol(&mut pending, &message);
                            }
                            return;
                        }
                        SystemMessage::Stderr(message) => {
                            let _ = events.send(LureEvent::ProcessStderr { message });
                        }
                    }
                }
            }
            _ = ticker.tick() => {
                expire_requests(&mut pending);
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let _ = events.send(LureEvent::ProcessExited { code: status.code() });
                        fail_pending(&mut pending);
                        return;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = events.send(LureEvent::ProtocolError { message: error.to_string() });
                    }
                }
            }
        }
    }
}

async fn write_frame(stdin: &mut ChildStdin, value: &Value) -> Result<(), RpcError> {
    let mut encoded =
        serde_json::to_vec(value).map_err(|error| RpcError::Protocol(error.to_string()))?;
    encoded.push(b'\n');
    stdin.write_all(&encoded).await?;
    stdin.flush().await?;
    Ok(())
}

async fn handle_frame(
    frame: Value,
    pending: &mut HashMap<String, PendingRequest>,
    stdin: &mut ChildStdin,
    events: &broadcast::Sender<LureEvent>,
) {
    if frame["type"].as_str() == Some("response") {
        if let Some(id) = frame["id"].as_str() {
            if let Some(request) = pending.remove(id) {
                if frame["success"].as_bool() == Some(true) {
                    if let RequestContext::Prompt(message) = request.context {
                        let _ = events.send(LureEvent::UserMessageAccepted {
                            request_id: id.to_owned(),
                            message,
                        });
                    }
                    let _ = request.reply.send(Ok(frame));
                } else {
                    let message = frame["error"]
                        .as_str()
                        .unwrap_or("Pi 拒绝了该命令")
                        .to_owned();
                    let _ = request.reply.send(Err(RpcError::CommandRejected {
                        command: request.command,
                        message,
                    }));
                }
            }
        }
        return;
    }

    let normalized = normalize_event(&frame);
    if let Some(response) = normalized.automatic_response {
        let _ = write_frame(stdin, &response).await;
    }
    for event in normalized.events {
        let _ = events.send(event);
    }
}

fn expire_requests(pending: &mut HashMap<String, PendingRequest>) {
    let now = Instant::now();
    let expired: Vec<String> = pending
        .iter()
        .filter(|(_, request)| request.deadline <= now)
        .map(|(id, _)| id.clone())
        .collect();
    for id in expired {
        if let Some(request) = pending.remove(&id) {
            let _ = request.reply.send(Err(RpcError::Timeout(request.command)));
        }
    }
}

fn fail_pending(pending: &mut HashMap<String, PendingRequest>) {
    for (_, request) in pending.drain() {
        let _ = request.reply.send(Err(RpcError::ProcessExited));
    }
}

fn fail_pending_with_protocol(pending: &mut HashMap<String, PendingRequest>, message: &str) {
    for (_, request) in pending.drain() {
        let _ = request
            .reply
            .send(Err(RpcError::Protocol(message.to_owned())));
    }
}

async fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    if let Some(process_id) = child.id().and_then(|id| i32::try_from(id).ok()) {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let _ = kill(Pid::from_raw(process_id), Signal::SIGTERM);
        if matches!(
            tokio::time::timeout(Duration::from_secs(1), child.wait()).await,
            Ok(Ok(_))
        ) {
            return;
        }
    }

    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
}
