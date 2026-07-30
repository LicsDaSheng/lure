//! Pairing 子系统：DM 发送者审批的配对码存储。
//!
//! 对齐上游 `nanobot/pairing/`：以 `~/.nanobot/pairing.json` 记录各 channel 的已审批
//! 发送者与待审批配对码。核心为 [`PairingStore`] 与纯派发的 [`PairingStore::handle_pairing_command`]，
//! CLI 与 agent 命令路由（`/pairing`）共用。

mod store;

pub use store::{
    format_expiry, format_pairing_reply, now_secs, ClearCounts, PairingStore, PendingRequest,
    DEFAULT_TTL_SECS,
};

/// 供 channel/命令给配对相关消息打标的 metadata key（对齐上游 `PAIRING_CODE_META_KEY`）。
pub const PAIRING_CODE_META_KEY: &str = "_pairing_code";
/// 供 channel/命令给配对命令消息打标的 metadata key（对齐上游 `PAIRING_COMMAND_META_KEY`）。
pub const PAIRING_COMMAND_META_KEY: &str = "_pairing_command";
