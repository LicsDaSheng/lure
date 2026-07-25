//! Channel 发送方访问控制：`AccessPolicy::is_allowed`（star > 精确 allowlist > pairing > deny）。
//!
//! 对齐上游 `nanobot/channels/base.py::is_allowed` + `tests/channels/test_base_channel.py`。

use lure_core::channel::{AccessPolicy, ChannelAccessConfig, PairingApprover};

/// 测试用 pairing 批准器：仅批准指定 sender。
struct ApproveOnly(&'static str);
impl PairingApprover for ApproveOnly {
    fn is_approved(&self, sender_id: &str) -> bool {
        sender_id == self.0
    }
}

fn policy(allow: &[&str]) -> AccessPolicy {
    AccessPolicy::new(allow.iter().map(|s| s.to_string()))
}

#[test]
fn exact_match_required() {
    let p = policy(&["allow@email.com"]);
    assert!(p.is_allowed_no_pairing("allow@email.com"));
    // 子串/前缀注入不得放行。
    assert!(!p.is_allowed_no_pairing("attacker|allow@email.com"));
}

#[test]
fn star_allows_all() {
    let p = policy(&["*"]);
    assert!(p.is_allowed_no_pairing("anyone"));
}

#[test]
fn empty_allow_from_denies() {
    let p = policy(&[]);
    assert!(!p.is_allowed_no_pairing("alice"));
}

#[test]
fn pairing_fallback_allows_approved_only() {
    // 不在 allowlist，但 pairing 批准者放行 "paired"。
    let p = policy(&[]);
    let approver = ApproveOnly("paired");
    assert!(p.is_allowed("paired", &approver));
    assert!(!p.is_allowed("unknown", &approver));
}

#[test]
fn star_wins_over_pairing_denial() {
    let p = policy(&["*"]);
    struct DenyAll;
    impl PairingApprover for DenyAll {
        fn is_approved(&self, _: &str) -> bool {
            false
        }
    }
    assert!(p.is_allowed("anyone", &DenyAll));
}

// --- config alias / null ---

fn config_from(json: &str) -> ChannelAccessConfig {
    serde_json::from_str(json).unwrap()
}

#[test]
fn config_supports_snake_and_camel_case_alias() {
    let snake = config_from(r#"{"allow_from": ["alice"]}"#);
    assert!(AccessPolicy::from(&snake).is_allowed_no_pairing("alice"));

    let camel = config_from(r#"{"allowFrom": ["bob"]}"#);
    assert!(AccessPolicy::from(&camel).is_allowed_no_pairing("bob"));
}

#[test]
fn config_null_allow_from_denies() {
    let p = AccessPolicy::from(&config_from(r#"{"allow_from": null}"#));
    assert!(!p.is_allowed_no_pairing("alice"));
    let p2 = AccessPolicy::from(&config_from(r#"{"allowFrom": null}"#));
    assert!(!p2.is_allowed_no_pairing("alice"));
}

#[test]
fn config_missing_allow_from_defaults_empty() {
    let p = AccessPolicy::from(&config_from("{}"));
    assert!(!p.is_allowed_no_pairing("alice"));
}
