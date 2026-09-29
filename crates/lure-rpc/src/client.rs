use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use lure_core::LureEvent;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{Instant, MissedTickBehavior};

use crate::RpcError;
use crate::env::{SpawnEnv, pi_runtime_path};
use crate::jsonl::read_json_lines;
use crate::normalize::normalize_event;
use crate::protocol::{
    ClearedQueue, RequestContext, RpcCommand, RpcImage, RpcModel, RpcSessionState, SessionEntries,
    SessionSwitch,
};

const DEFAULT_FRAME_LIMIT: usize = 16 * 1024 * 1024;
const STDERR_EVENT_LIMIT: usize = 512;

#[derive(Debug, Clone)]
pub struct PiProcessConfig {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub handshake_timeout: Duration,
    pub command_timeout: Duration,
    pub frame_limit: usize,
    /// 子进程环境来源；默认为继承 Lure 进程环境。
    pub spawn_env: SpawnEnv,
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
            spawn_env: SpawnEnv::Inherit,
        }
    }

    #[must_use]
    pub fn for_working_directory(working_directory: impl Into<PathBuf>) -> Self {
        let executable = std::env::var_os("LURE_PI_PATH").map_or_else(
            || find_pi_executable().unwrap_or_else(|| PathBuf::from("pi")),
            PathBuf::from,
        );
        Self::new(executable, working_directory)
    }
}

fn find_pi_executable() -> Option<PathBuf> {
    let search_path = pi_runtime_path(
        std::env::var_os("HOME").as_deref(),
        std::env::var_os("PATH").as_deref(),
    );
    find_executable_in_path("pi", &search_path)
}

fn find_executable_in_path(executable: &str, search_path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(search_path)
        .map(|directory| directory.join(executable))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
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

        let mut command = tokio::process::Command::new(&config.executable);
        match &config.spawn_env {
            SpawnEnv::Inherit => {
                let runtime_path = pi_runtime_path(
                    std::env::var_os("HOME").as_deref(),
                    std::env::var_os("PATH").as_deref(),
                );
                command.env("PATH", runtime_path);
            }
            SpawnEnv::Provided(vars) => {
                command
                    .env_clear()
                    .envs(vars.iter().map(|(key, value)| (key, value)));
            }
        }
        command
            .args(["--mode", "rpc"])
            .current_dir(&config.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                RpcError::PiNotFound(config.executable.display().to_string())
            } else {
                RpcError::SpawnFailed(error.to_string())
            }
        })?;

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
        tokio::spawn(async move {
            match read_json_lines(stdout, frame_limit, frame_tx).await {
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

    /// 切换到已有的 Pi 会话文件。
    ///
    /// 返回 `SessionSwitch::Cancelled` 表示 Pi 扩展取消了本次切换，此时当前会话保持不变。
    ///
    /// # Errors
    ///
    /// Pi 拒绝切换、返回无效状态或请求失败时返回错误。
    pub async fn switch_session(&self, session_path: &str) -> Result<SessionSwitch, RpcError> {
        let response = self
            .request(
                json!({"type":"switch_session","sessionPath":session_path}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        if response
            .pointer("/data/cancelled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(SessionSwitch::Cancelled);
        }
        let state = self.read_state(self.command_timeout).await?;
        Ok(SessionSwitch::Switched(Box::new(state)))
    }

    /// 读取当前会话的条目，`since` 为上次已见条目 id 时可只取增量。
    ///
    /// # Errors
    ///
    /// Pi 返回无效响应、游标不存在或请求失败时返回错误。
    pub async fn get_entries(&self, since: Option<&str>) -> Result<SessionEntries, RpcError> {
        let body = match since {
            Some(cursor) => json!({"type":"get_entries","since":cursor}),
            None => json!({"type":"get_entries"}),
        };
        let response = self
            .request(body, RequestContext::Plain, self.command_timeout)
            .await?;
        let data = &response["data"];
        let entries = data
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| RpcError::Protocol("get_entries 响应缺少 entries 数组".into()))?;
        Ok(SessionEntries {
            entries,
            leaf_id: data
                .get("leafId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        })
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

    /// 排队一条插队引导消息：当前 assistant 轮次的工具调用结束后、下一次模型调用前交付。
    ///
    /// 与 `prompt` 不同，接受时不会产生 `UserMessageAccepted`；消息被 Pi 实际消费时
    /// 以 `UserMessageObserved` 出现在事件流中，队列归属由 `QueueChanged` 反映。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用、请求超时或命令被拒绝（如扩展命令）时返回错误。
    pub async fn steer(
        &self,
        message: impl Into<String>,
        images: Vec<RpcImage>,
    ) -> Result<(), RpcError> {
        self.queue_message("steer", message, images).await
    }

    /// 排队一条后续消息：当前运行完全结束、且没有更多工具调用或插队消息后交付。
    ///
    /// 事件流语义与 [`Self::steer`] 相同。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用、请求超时或命令被拒绝（如扩展命令）时返回错误。
    pub async fn follow_up(
        &self,
        message: impl Into<String>,
        images: Vec<RpcImage>,
    ) -> Result<(), RpcError> {
        self.queue_message("follow_up", message, images).await
    }

    async fn queue_message(
        &self,
        command: &str,
        message: impl Into<String>,
        images: Vec<RpcImage>,
    ) -> Result<(), RpcError> {
        let mut body = json!({"type":command,"message":message.into()});
        if !images.is_empty() {
            body["images"] = json!(images);
        }
        self.request(body, RequestContext::Plain, self.command_timeout)
            .await?;
        Ok(())
    }

    /// 清空待处理队列（插队引导与排队后续），返回被清空的内容以便恢复草稿。
    ///
    /// # Errors
    ///
    /// Pi 进程不可用、请求超时或响应无效时返回错误。
    pub async fn clear_queue(&self) -> Result<ClearedQueue, RpcError> {
        let response = self
            .request(
                json!({"type":"clear_queue"}),
                RequestContext::Plain,
                self.command_timeout,
            )
            .await?;
        serde_json::from_value(response["data"].clone())
            .map_err(|error| RpcError::Protocol(format!("clear_queue 响应无效：{error}")))
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

#[cfg(test)]
mod tests {
    use super::{find_executable_in_path, pi_runtime_path};
    use std::ffi::OsStr;

    #[cfg(unix)]
    #[test]
    fn finds_pi_in_local_bin_when_shell_path_does_not_include_it() {
        use std::os::unix::fs::PermissionsExt;

        let home = std::env::temp_dir().join(format!("lure-pi-path-test-{}", std::process::id()));
        let local_bin = home.join(".local/bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        let pi = local_bin.join("pi");
        std::fs::write(&pi, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(&pi).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&pi, permissions).unwrap();

        let runtime_path = pi_runtime_path(Some(home.as_os_str()), Some(OsStr::new("/usr/bin")));

        assert_eq!(find_executable_in_path("pi", &runtime_path), Some(pi));
        std::fs::remove_dir_all(home).unwrap();
    }
}
