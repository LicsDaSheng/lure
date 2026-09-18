use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("工作目录无效：{0}")]
    InvalidWorkingDirectory(PathBuf),
    #[error("未找到可执行的 Pi：{0}")]
    PiNotFound(String),
    #[error("启动 Pi 失败：{0}")]
    SpawnFailed(String),
    #[error("Pi RPC IO 失败：{0}")]
    Io(String),
    #[error("Pi RPC 输出不是有效 UTF-8：{0}")]
    InvalidUtf8(String),
    #[error("Pi RPC 输出不是有效 JSON：{0}")]
    InvalidJson(String),
    #[error("Pi RPC 单条记录超过 {limit} 字节")]
    FrameTooLarge { limit: usize },
    #[error("Pi RPC 命令 {command} 被拒绝：{message}")]
    CommandRejected { command: String, message: String },
    #[error("Pi RPC 请求超时：{0}")]
    Timeout(String),
    #[error("Pi RPC 进程已经退出")]
    ProcessExited,
    #[error("Pi RPC actor 已停止")]
    ActorStopped,
    #[error("Pi RPC 响应格式错误：{0}")]
    Protocol(String),
}

impl From<std::io::Error> for RpcError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
