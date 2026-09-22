use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcModel {
    pub provider: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcImage {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSessionState {
    pub model: Option<RpcModel>,
    pub thinking_level: String,
    pub is_streaming: bool,
    pub is_compacting: bool,
    pub steering_mode: String,
    pub follow_up_mode: String,
    pub session_file: Option<String>,
    pub session_id: String,
    pub session_name: Option<String>,
    pub auto_compaction_enabled: bool,
    pub message_count: u64,
    pub pending_message_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestContext {
    Plain,
    Prompt(String),
}

/// 会话条目查询结果。
///
/// `entries` 保持 Pi 返回的原始条目（消息、压缩摘要、分支摘要等），
/// 由消费方决定如何投影；`leaf_id` 是当前活动分支的叶子条目 id。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntries {
    pub entries: Vec<serde_json::Value>,
    pub leaf_id: Option<String>,
}

/// 切换会话的结果：Pi 扩展可以取消切换。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSwitch {
    Switched(Box<RpcSessionState>),
    Cancelled,
}
