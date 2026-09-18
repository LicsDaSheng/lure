use serde::{Deserialize, Serialize};

use crate::ConnectionSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LureEvent {
    ConnectionChanged {
        snapshot: ConnectionSnapshot,
    },
    SessionReady {
        snapshot: ConnectionSnapshot,
    },
    UserMessageAccepted {
        request_id: String,
        message: String,
    },
    AssistantMessageStarted,
    AssistantTextDelta {
        content_index: u64,
        delta: String,
    },
    AssistantThinkingDelta {
        content_index: u64,
        delta: String,
    },
    AssistantMessageCompleted {
        text: String,
        thinking: String,
    },
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
    },
    ToolUpdated {
        tool_call_id: String,
        tool_name: String,
    },
    ToolCompleted {
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
    },
    RunStarted,
    RunFinished {
        will_retry: bool,
    },
    RunSettled,
    RetryChanged {
        active: bool,
        attempt: Option<u64>,
        max_attempts: Option<u64>,
        message: Option<String>,
    },
    CompactionChanged {
        active: bool,
        reason: Option<String>,
        aborted: Option<bool>,
    },
    ExtensionUiUnsupported {
        method: String,
        title: Option<String>,
    },
    Notification {
        level: String,
        message: String,
    },
    ProcessStderr {
        message: String,
    },
    ProcessExited {
        code: Option<i32>,
    },
    ProtocolError {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub sequence: u64,
    pub event: LureEvent,
}
