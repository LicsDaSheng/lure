//! 映射上游 `tests/tools/test_exec_allow_patterns.py` 的 allow/deny 语义。

use lure_core::tool::ExecPolicy;

fn policy(allow: &[&str], deny: &[&str]) -> ExecPolicy {
    ExecPolicy::new(allow, deny).unwrap()
}

#[test]
fn deny_patterns_block_rm_rf_by_default() {
    let result = policy(&[], &[]).guard_command("rm -rf /tmp/build");
    assert!(result.is_some());
    assert!(result
        .unwrap()
        .to_lowercase()
        .contains("deny pattern filter"));
}

#[test]
fn allow_patterns_bypass_deny() {
    let result = policy(&[r"rm\s+-rf\s+/tmp/.*"], &[]).guard_command("rm -rf /tmp/build");
    assert!(result.is_none());
}

#[test]
fn non_matching_allow_does_not_bypass_deny() {
    let result = policy(&[r"rm\s+-rf\s+/opt/"], &[]).guard_command("rm -rf /tmp/build");
    assert!(result.is_some());
    assert!(result
        .unwrap()
        .to_lowercase()
        .contains("deny pattern filter"));
}

#[test]
fn extra_deny_patterns_append_to_builtin() {
    let p = policy(&[], &[r"\bping\b"]);
    assert!(p.guard_command("ping example.com").is_some());
    assert!(p.guard_command("rm -rf /tmp/x").is_some());
}

#[test]
fn allow_patterns_bypass_extra_deny() {
    let result =
        policy(&[r"\bping\s+example\.com\b"], &[r"\bping\b"]).guard_command("ping example.com");
    assert!(result.is_none());
}

#[test]
fn allow_patterns_are_whitelist_only() {
    let p = policy(&[r"echo\s+hello"], &[]);
    assert!(p.guard_command("echo hello").is_none());
    let result = p.guard_command("ls /tmp");
    assert!(result.is_some());
    assert!(result.unwrap().to_lowercase().contains("allowlist"));
}

#[test]
fn allowlist_blocks_non_matching_chained_segment() {
    let p = policy(&[r"\becho\s+allowlisted\b"], &[]);
    let result = p.guard_command("echo allowlisted && touch /tmp/evil");
    assert!(result.is_some());
    assert!(result.unwrap().to_lowercase().contains("allowlist"));
}

#[test]
fn allowlist_blocks_single_ampersand_chained_segment() {
    let p = policy(&[r"echo\s+allowlisted.*"], &[]);
    let result = p.guard_command("echo allowlisted & touch /tmp/evil");
    assert!(result.is_some());
    assert!(result.unwrap().to_lowercase().contains("allowlist"));
}

#[test]
fn allowlist_blocks_trailing_background_operator() {
    let p = policy(&[r"echo\s+allowlisted"], &[]);
    let result = p.guard_command("echo allowlisted &");
    assert!(result.is_some());
    assert!(result.unwrap().to_lowercase().contains("allowlist"));
}

#[test]
fn allowlist_keeps_fd_redirection_ampersand() {
    let p = policy(&[r"echo\s+allowlisted\s+2>&1"], &[]);
    assert!(p.guard_command("echo allowlisted 2>&1").is_none());
}

#[test]
fn deny_searches_original_command_after_quoted_hash() {
    let p = policy(&[], &[r"\brm\s+-rf\s+/"]);
    let result = p.guard_command(r##"echo "#"; rm -rf /"##);
    assert!(result.is_some());
    assert!(result
        .unwrap()
        .to_lowercase()
        .contains("deny pattern filter"));
}

#[test]
fn allow_fullmatch_exempts_exact_denied_command() {
    let result = policy(&[r"rm\s+-rf\s+/tmp/build"], &[]).guard_command("rm -rf /tmp/build");
    assert!(result.is_none());
}

#[test]
fn allowlist_allows_multiple_matching_segments() {
    let p = policy(
        &[r"\becho\s+allowlisted\b", r"\becho\s+also_allowed\b"],
        &[],
    );
    assert!(p
        .guard_command("echo allowlisted && echo also_allowed")
        .is_none());
}

#[test]
fn allowlist_supports_anchored_patterns() {
    let p = policy(&[r"^echo\s+allowlisted$"], &[]);
    assert!(p.guard_command("echo allowlisted").is_none());
}
