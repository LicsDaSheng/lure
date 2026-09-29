use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use lure_core::{ConnectionPhase, ConnectionSnapshot, ErrorCode, LureError};
use lure_rpc::{PiRpcClient, SpawnEnvResolver};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

pub(crate) struct DesktopSession {
    pub client: PiRpcClient,
    pub event_task: JoinHandle<()>,
}

pub(crate) struct AppState {
    pub command_lock: Mutex<()>,
    pub session: Mutex<Option<DesktopSession>>,
    pub snapshot: RwLock<ConnectionSnapshot>,
    pub sequence: AtomicU64,
    /// Pi 子进程环境合成器：登录 shell 环境捕获缓存于此，系统代理每次连接重读。
    pub spawn_env_resolver: SpawnEnvResolver,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            command_lock: Mutex::new(()),
            session: Mutex::new(None),
            snapshot: RwLock::new(ConnectionSnapshot::default()),
            sequence: AtomicU64::new(1),
            spawn_env_resolver: SpawnEnvResolver::new(),
        }
    }
}

pub(crate) type SharedAppState = Arc<AppState>;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Operation {
    Connect,
    NewSession,
    Prompt,
    Abort,
    SwitchSession,
    /// 运行中排队消息（`steer` 插队引导 / `follow_up` 排队后续）。
    QueueMessage,
    ClearQueue,
}

pub(crate) fn ensure_operation_allowed(
    phase: ConnectionPhase,
    operation: Operation,
) -> Result<(), LureError> {
    match (phase, operation) {
        (ConnectionPhase::Disconnected | ConnectionPhase::Failed, Operation::Connect)
        | (
            ConnectionPhase::Ready,
            Operation::NewSession | Operation::Prompt | Operation::SwitchSession,
        )
        | (ConnectionPhase::Running, Operation::Abort | Operation::QueueMessage)
        | (ConnectionPhase::Ready | ConnectionPhase::Running, Operation::ClearQueue) => Ok(()),
        (
            ConnectionPhase::Connecting | ConnectionPhase::Ready | ConnectionPhase::Running,
            Operation::Connect,
        ) => Err(LureError::new(
            ErrorCode::AlreadyConnected,
            "Pi 已连接或正在连接",
        )),
        (
            ConnectionPhase::Running,
            Operation::NewSession | Operation::Prompt | Operation::SwitchSession,
        ) => Err(LureError::new(
            ErrorCode::RunAlreadyActive,
            "Pi 正在执行任务，请先停止当前运行",
        )),
        (ConnectionPhase::Ready, Operation::QueueMessage) => Err(LureError::new(
            ErrorCode::RpcCommandRejected,
            "当前没有正在运行的任务，直接发送消息即可",
        )),
        (
            _,
            Operation::NewSession
            | Operation::Prompt
            | Operation::Abort
            | Operation::SwitchSession
            | Operation::QueueMessage
            | Operation::ClearQueue,
        ) => Err(LureError::new(
            ErrorCode::NotConnected,
            "当前没有可用的 Pi RPC 会话",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Operation, ensure_operation_allowed};
    use lure_core::ConnectionPhase;

    #[test]
    fn only_ready_sessions_accept_prompts() {
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::Prompt).is_ok());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::Prompt).is_err()
        );
        assert!(ensure_operation_allowed(ConnectionPhase::Running, Operation::Prompt).is_err());
    }

    #[test]
    fn only_running_sessions_accept_abort() {
        assert!(ensure_operation_allowed(ConnectionPhase::Running, Operation::Abort).is_ok());
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::Abort).is_err());
    }

    #[test]
    fn only_running_sessions_accept_queued_messages() {
        assert!(
            ensure_operation_allowed(ConnectionPhase::Running, Operation::QueueMessage).is_ok()
        );
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::QueueMessage).is_err());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::QueueMessage)
                .is_err()
        );
    }

    #[test]
    fn clearing_the_queue_is_allowed_whenever_a_session_is_available() {
        assert!(ensure_operation_allowed(ConnectionPhase::Running, Operation::ClearQueue).is_ok());
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::ClearQueue).is_ok());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::ClearQueue).is_err()
        );
    }

    #[test]
    fn active_sessions_reject_duplicate_connects() {
        assert!(ensure_operation_allowed(ConnectionPhase::Connecting, Operation::Connect).is_err());
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::Connect).is_err());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::Connect).is_ok()
        );
    }

    #[test]
    fn only_ready_sessions_can_be_replaced() {
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::NewSession).is_ok());
        assert!(ensure_operation_allowed(ConnectionPhase::Running, Operation::NewSession).is_err());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::NewSession).is_err()
        );
    }

    #[test]
    fn only_idle_ready_sessions_can_switch_to_a_recorded_session() {
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::SwitchSession).is_ok());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Running, Operation::SwitchSession).is_err()
        );
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::SwitchSession)
                .is_err()
        );
    }
}
