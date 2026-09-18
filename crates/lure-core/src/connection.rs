use serde::{Deserialize, Serialize};

use crate::LureError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPhase {
    #[default]
    Disconnected,
    Connecting,
    Ready,
    Running,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSnapshot {
    pub provider: String,
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSnapshot {
    pub phase: ConnectionPhase,
    pub working_directory: Option<String>,
    pub session_id: Option<String>,
    pub session_file: Option<String>,
    pub model: Option<ModelSnapshot>,
    pub thinking_level: Option<String>,
    pub error: Option<LureError>,
}
