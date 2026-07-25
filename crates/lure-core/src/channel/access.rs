//! Channel 发送方访问控制。
//!
//! 对齐上游 `nanobot/channels/base.py::is_allowed` 的优先级：
//! `*`（放行全部）> 精确 allowlist 命中 > pairing 批准 > 拒绝。
//! allowlist 条目为不透明 token，必须**精确**匹配（不做子串/前缀匹配，防注入）。

use serde::Deserialize;

/// pairing 批准查询：sender 不在 allowlist 时的兜底放行判定。
pub trait PairingApprover {
    /// 该 sender 是否已通过 pairing 批准。
    fn is_approved(&self, sender_id: &str) -> bool;
}

/// 恒拒绝的 pairing 批准器（无 pairing 场景默认）。
pub struct DenyAllPairing;

impl PairingApprover for DenyAllPairing {
    fn is_approved(&self, _sender_id: &str) -> bool {
        false
    }
}

/// 发送方访问策略：持有 allowlist，按优先级判定 sender 是否放行。
#[derive(Debug, Clone, Default)]
pub struct AccessPolicy {
    allow_from: Vec<String>,
}

impl AccessPolicy {
    /// 用 allowlist 构造。
    pub fn new(allow_from: impl IntoIterator<Item = String>) -> Self {
        Self {
            allow_from: allow_from.into_iter().collect(),
        }
    }

    /// 判定 `sender_id` 是否放行：`*` > 精确命中 > pairing 批准 > 拒绝。
    pub fn is_allowed(&self, sender_id: &str, approver: &dyn PairingApprover) -> bool {
        if self.allow_from.iter().any(|entry| entry == "*") {
            return true;
        }
        if self.allow_from.iter().any(|entry| entry == sender_id) {
            return true;
        }
        approver.is_approved(sender_id)
    }

    /// 无 pairing 兜底的便捷判定（等价于 pairing 恒拒绝）。
    pub fn is_allowed_no_pairing(&self, sender_id: &str) -> bool {
        self.is_allowed(sender_id, &DenyAllPairing)
    }
}

/// channel 访问配置：支持 `allow_from` 与 `allowFrom` 别名；`null`/缺失均视为空。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelAccessConfig {
    /// 放行的发送方 token 列表。
    #[serde(default, alias = "allowFrom", deserialize_with = "de_nullable_vec")]
    pub allow_from: Vec<String>,
}

/// 把 `null` 反序列化为空 `Vec`（缺失由 `#[serde(default)]` 兜底）。
fn de_nullable_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

impl From<&ChannelAccessConfig> for AccessPolicy {
    fn from(config: &ChannelAccessConfig) -> Self {
        Self::new(config.allow_from.iter().cloned())
    }
}
