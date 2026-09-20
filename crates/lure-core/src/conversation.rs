use serde::{Deserialize, Serialize};

use crate::ConnectionSnapshot;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageBlock {
    pub content_index: u64,
    pub kind: MessageBlockKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageBlockKind {
    Text,
    Thinking,
}

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
        blocks: Vec<MessageBlock>,
        stop_reason: Option<String>,
        error_message: Option<String>,
    },
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
        input: String,
    },
    ToolUpdated {
        tool_call_id: String,
        tool_name: String,
        input: String,
        output: String,
        truncated_lines: Option<u64>,
    },
    ToolCompleted {
        tool_call_id: String,
        tool_name: String,
        input: String,
        output: String,
        truncated_lines: Option<u64>,
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
        delay_ms: Option<u64>,
        message: Option<String>,
    },
    CompactionChanged {
        active: bool,
        reason: Option<String>,
        aborted: Option<bool>,
        summary: Option<String>,
        tokens_before: Option<u64>,
        error_message: Option<String>,
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
