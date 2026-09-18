use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use lure_core::{ConnectionPhase, ConnectionSnapshot, ErrorCode, LureError};
use lure_rpc::PiRpcClient;
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            command_lock: Mutex::new(()),
            session: Mutex::new(None),
            snapshot: RwLock::new(ConnectionSnapshot::default()),
            sequence: AtomicU64::new(1),
        }
    }
}

pub(crate) type SharedAppState = Arc<AppState>;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Operation {
    Connect,
    Prompt,
    Abort,
}

pub(crate) fn ensure_operation_allowed(
    phase: ConnectionPhase,
    operation: Operation,
) -> Result<(), LureError> {
    match (phase, operation) {
        (ConnectionPhase::Disconnected | ConnectionPhase::Failed, Operation::Connect)
        | (ConnectionPhase::Ready, Operation::Prompt)
        | (ConnectionPhase::Running, Operation::Abort) => Ok(()),
        (
            ConnectionPhase::Connecting | ConnectionPhase::Ready | ConnectionPhase::Running,
            Operation::Connect,
        ) => Err(LureError::new(
            ErrorCode::AlreadyConnected,
            "Pi 已连接或正在连接",
        )),
        (ConnectionPhase::Running, Operation::Prompt) => Err(LureError::new(
            ErrorCode::RunAlreadyActive,
            "Pi 正在执行任务，请先停止当前运行",
        )),
        (_, Operation::Prompt | Operation::Abort) => Err(LureError::new(
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
    fn active_sessions_reject_duplicate_connects() {
        assert!(ensure_operation_allowed(ConnectionPhase::Connecting, Operation::Connect).is_err());
        assert!(ensure_operation_allowed(ConnectionPhase::Ready, Operation::Connect).is_err());
        assert!(
            ensure_operation_allowed(ConnectionPhase::Disconnected, Operation::Connect).is_ok()
        );
    }
}
