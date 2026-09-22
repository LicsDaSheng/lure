use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    #[serde(rename = "PI_NOT_FOUND")]
    PiNotFound,
    #[serde(rename = "INVALID_WORKING_DIRECTORY")]
    InvalidWorkingDirectory,
    #[serde(rename = "ALREADY_CONNECTED")]
    AlreadyConnected,
    #[serde(rename = "NOT_CONNECTED")]
    NotConnected,
    #[serde(rename = "RUN_ALREADY_ACTIVE")]
    RunAlreadyActive,
    #[serde(rename = "SPAWN_FAILED")]
    SpawnFailed,
    #[serde(rename = "HANDSHAKE_TIMEOUT")]
    HandshakeTimeout,
    #[serde(rename = "RPC_COMMAND_REJECTED")]
    RpcCommandRejected,
    #[serde(rename = "RPC_PROTOCOL_ERROR")]
    RpcProtocolError,
    #[serde(rename = "RPC_FRAME_TOO_LARGE")]
    RpcFrameTooLarge,
    #[serde(rename = "PROCESS_EXITED")]
    ProcessExited,
    #[serde(rename = "ATTACHMENT_UNREADABLE")]
    AttachmentUnreadable,
    #[serde(rename = "SESSION_LIST_FAILED")]
    SessionListFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LureError {
    pub code: ErrorCode,
    pub message: String,
}

impl LureError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
