//! Pairing store 集成测试。
//!
//! 对照上游 `tests/pairing/test_store.py`：DM 发送者配对码的生成/审批/拒绝/撤销、
//! TTL 过期、`handle_pairing_command` 纯函数派发、数字 sender_id 的字符串化往返、
//! 损坏文件恢复。lure 以 `PairingStore`（按路径参数化、每次调用 load/save）替代上游
//! 模块级全局锁 + monkeypatch `_store_path`。

use lure_core::pairing::{format_pairing_reply, PairingStore};
use tempfile::TempDir;

fn store() -> (TempDir, PairingStore) {
    let dir = TempDir::new().expect("临时目录");
    let path = dir.path().join("pairing.json");
    (dir, PairingStore::new(path))
}

const T0: f64 = 1_000.0;

// —— generate_code —— //

#[tokio::test]
async fn generate_code_has_grouped_format() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    assert_eq!(code.chars().count(), 9, "4 + 1 分隔符 + 4");
    assert_eq!(code.as_bytes()[4], b'-');
    let raw = code.replace('-', "");
    assert!(raw.chars().all(|c| c.is_ascii_alphanumeric()));
    assert!(raw.chars().all(|c| !c.is_ascii_lowercase()));
}

#[tokio::test]
async fn generate_code_is_unique() {
    let (_d, s) = store();
    let mut codes = std::collections::HashSet::new();
    for i in 0..20 {
        codes.insert(
            s.generate_code("telegram", &i.to_string(), 600.0, T0)
                .unwrap(),
        );
    }
    assert_eq!(codes.len(), 20);
}

#[tokio::test]
async fn ttl_governs_expiration() {
    let (_d, s) = store();
    // ttl=1，未推进时钟：仍可审批。
    let code = s.generate_code("telegram", "123", 1.0, T0).unwrap();
    assert_eq!(
        s.approve_code(&code, T0).unwrap(),
        Some(("telegram".into(), "123".into()))
    );
    // ttl=0，时钟前进：过期，审批失败。
    let code2 = s.generate_code("telegram", "456", 0.0, T0).unwrap();
    assert_eq!(s.approve_code(&code2, T0 + 0.1).unwrap(), None);
}

// —— format_pairing_reply —— //

#[tokio::test]
async fn pairing_reply_points_to_webui_with_command_fallback() {
    let reply = format_pairing_reply("ABCD-EFGH");
    assert!(reply.contains("WebUI"));
    assert!(reply.contains("ABCD-EFGH"));
    assert!(reply.contains("/pairing approve ABCD-EFGH"));
}

// —— approve / deny —— //

#[tokio::test]
async fn approve_moves_pending_to_approved() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    assert!(!s.is_approved("telegram", "123").unwrap());

    assert_eq!(
        s.approve_code(&code, T0).unwrap(),
        Some(("telegram".into(), "123".into()))
    );
    assert!(s.is_approved("telegram", "123").unwrap());
    assert_eq!(s.get_approved("telegram").unwrap(), vec!["123".to_string()]);
}

#[tokio::test]
async fn deny_removes_pending() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    assert!(s.deny_code(&code, T0).unwrap());
    assert_eq!(s.approve_code(&code, T0).unwrap(), None);
}

#[tokio::test]
async fn deny_unknown_returns_false() {
    let (_d, s) = store();
    assert!(!s.deny_code("UNKNOWN", T0).unwrap());
}

#[tokio::test]
async fn approve_expired_returns_none() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 0.0, T0).unwrap();
    assert_eq!(s.approve_code(&code, T0 + 0.1).unwrap(), None);
}

// —— revoke / clear —— //

#[tokio::test]
async fn revoke_removes_approved_sender() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    s.approve_code(&code, T0).unwrap();
    assert!(s.is_approved("telegram", "123").unwrap());

    assert!(s.revoke("telegram", "123").unwrap());
    assert!(!s.is_approved("telegram", "123").unwrap());
    assert!(s.get_approved("telegram").unwrap().is_empty());
}

#[tokio::test]
async fn revoke_unknown_returns_false() {
    let (_d, s) = store();
    assert!(!s.revoke("telegram", "999").unwrap());
}

#[tokio::test]
async fn clear_channel_removes_approved_and_pending_scoped() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    s.approve_code(&code, T0).unwrap();
    s.generate_code("telegram", "456", 600.0, T0).unwrap();
    s.generate_code("discord", "789", 600.0, T0).unwrap();

    let counts = s.clear_channel("telegram").unwrap();
    assert_eq!((counts.approved, counts.pending), (1, 1));

    assert!(!s.is_approved("telegram", "123").unwrap());
    let pending = s.list_pending(T0).unwrap();
    let channels: Vec<_> = pending.iter().map(|p| p.channel.clone()).collect();
    assert_eq!(channels, vec!["discord".to_string()]);
}

#[tokio::test]
async fn clear_channel_unknown_returns_zero_counts() {
    let (_d, s) = store();
    let counts = s.clear_channel("telegram").unwrap();
    assert_eq!((counts.approved, counts.pending), (0, 0));
}

// —— list_pending —— //

#[tokio::test]
async fn list_pending_empty() {
    let (_d, s) = store();
    assert!(s.list_pending(T0).unwrap().is_empty());
}

#[tokio::test]
async fn list_pending_shows_all_channels() {
    let (_d, s) = store();
    s.generate_code("telegram", "123", 600.0, T0).unwrap();
    s.generate_code("discord", "456", 600.0, T0).unwrap();
    let pending = s.list_pending(T0).unwrap();
    assert_eq!(pending.len(), 2);
    let channels: std::collections::HashSet<_> =
        pending.iter().map(|p| p.channel.as_str()).collect();
    assert_eq!(channels, ["telegram", "discord"].into_iter().collect());
}

#[tokio::test]
async fn list_pending_omits_expired() {
    let (_d, s) = store();
    s.generate_code("telegram", "123", 0.0, T0).unwrap();
    assert!(s.list_pending(T0 + 0.1).unwrap().is_empty());
}

// —— handle_pairing_command —— //

#[tokio::test]
async fn command_list_empty() {
    let (_d, s) = store();
    assert_eq!(
        s.handle_pairing_command("telegram", "list", T0).unwrap(),
        "No pending pairing requests."
    );
}

#[tokio::test]
async fn command_list_shows_pending() {
    let (_d, s) = store();
    s.generate_code("telegram", "123", 600.0, T0).unwrap();
    let reply = s.handle_pairing_command("telegram", "list", T0).unwrap();
    assert!(reply.contains("Pending pairing requests:"));
    assert!(reply.contains("telegram"));
    assert!(reply.contains("123"));
}

#[tokio::test]
async fn command_approve() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    let reply = s
        .handle_pairing_command("telegram", &format!("approve {code}"), T0)
        .unwrap();
    assert!(reply.contains("Approved"));
    assert!(reply.contains("123"));
    assert!(s.is_approved("telegram", "123").unwrap());
}

#[tokio::test]
async fn command_approve_invalid() {
    let (_d, s) = store();
    let reply = s
        .handle_pairing_command("telegram", "approve BAD-CODE", T0)
        .unwrap();
    assert!(reply.contains("Invalid or expired"));
}

#[tokio::test]
async fn command_approve_no_arg() {
    let (_d, s) = store();
    let reply = s.handle_pairing_command("telegram", "approve", T0).unwrap();
    assert!(reply.contains("Usage:"));
}

#[tokio::test]
async fn command_deny() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    let reply = s
        .handle_pairing_command("telegram", &format!("deny {code}"), T0)
        .unwrap();
    assert!(reply.contains("Denied"));
    assert_eq!(s.approve_code(&code, T0).unwrap(), None);
}

#[tokio::test]
async fn command_deny_unknown() {
    let (_d, s) = store();
    let reply = s
        .handle_pairing_command("telegram", "deny BAD-CODE", T0)
        .unwrap();
    assert!(reply.contains("not found"));
}

#[tokio::test]
async fn command_revoke_current_channel() {
    let (_d, s) = store();
    let code = s.generate_code("telegram", "123", 600.0, T0).unwrap();
    s.approve_code(&code, T0).unwrap();
    let reply = s
        .handle_pairing_command("telegram", "revoke 123", T0)
        .unwrap();
    assert!(reply.contains("Revoked"));
    assert!(!s.is_approved("telegram", "123").unwrap());
}

#[tokio::test]
async fn command_revoke_other_channel() {
    let (_d, s) = store();
    let code = s.generate_code("discord", "456", 600.0, T0).unwrap();
    s.approve_code(&code, T0).unwrap();
    let reply = s
        .handle_pairing_command("telegram", "revoke discord 456", T0)
        .unwrap();
    assert!(reply.contains("Revoked"));
    assert!(!s.is_approved("discord", "456").unwrap());
}

#[tokio::test]
async fn command_revoke_unknown() {
    let (_d, s) = store();
    let reply = s
        .handle_pairing_command("telegram", "revoke 999", T0)
        .unwrap();
    assert!(reply.contains("was not in the approved list"));
}

#[tokio::test]
async fn command_revoke_no_arg() {
    let (_d, s) = store();
    let reply = s.handle_pairing_command("telegram", "revoke", T0).unwrap();
    assert!(reply.contains("Usage:"));
}

#[tokio::test]
async fn command_unknown_subcommand() {
    let (_d, s) = store();
    let reply = s.handle_pairing_command("telegram", "foo", T0).unwrap();
    assert!(reply.contains("Unknown pairing command"));
}

#[tokio::test]
async fn command_defaults_to_list() {
    let (_d, s) = store();
    s.generate_code("telegram", "123", 600.0, T0).unwrap();
    let reply = s.handle_pairing_command("telegram", "", T0).unwrap();
    assert!(reply.contains("Pending pairing requests:"));
}

// —— 数字 sender_id 与手工编辑存储 —— //

#[tokio::test]
async fn hand_edited_numeric_sender_id_coerces_to_string() {
    let (dir, s) = store();
    std::fs::write(
        dir.path().join("pairing.json"),
        r#"{"approved": {"telegram": ["111"]}, "pending": {"ABCD-EFGH": {"channel": "telegram", "sender_id": 222, "created_at": 1000.0, "expires_at": 9999999999.0}}}"#,
    )
    .unwrap();
    assert_eq!(
        s.approve_code("ABCD-EFGH", T0).unwrap(),
        Some(("telegram".into(), "222".into()))
    );
    assert!(s.is_approved("telegram", "222").unwrap());
    s.generate_code("telegram", "333", 600.0, T0).unwrap();
    assert_eq!(
        s.get_approved("telegram").unwrap(),
        vec!["111".to_string(), "222".to_string()]
    );
}

#[tokio::test]
async fn hand_edited_numeric_approved_list_coerces_to_string() {
    let (dir, s) = store();
    std::fs::write(
        dir.path().join("pairing.json"),
        r#"{"approved": {"telegram": [12345]}, "pending": {}}"#,
    )
    .unwrap();
    assert!(s.is_approved("telegram", "12345").unwrap());
    assert!(s.revoke("telegram", "12345").unwrap());
    assert!(!s.is_approved("telegram", "12345").unwrap());
}

// —— 损坏恢复 —— //

#[tokio::test]
async fn corrupted_store_recovers_as_empty() {
    let (dir, s) = store();
    std::fs::write(dir.path().join("pairing.json"), "not json{").unwrap();
    assert!(s.list_pending(T0).unwrap().is_empty());
    assert!(!s.is_approved("telegram", "123").unwrap());
}
