//! 命令路由集成测试。
//!
//! 对照上游 `nanobot/command/router.py` + `tests/command/test_router_dispatchable.py`：
//! `normalize_command_text`（剥离 `@bot` 传输后缀）、三层派发（priority / exact / prefix，
//! 最长前缀优先）、`is_priority` / `is_dispatchable_command` 谓词、`/help` 与 `/pairing`
//! 具体处理器。运行时依赖型命令（/dream* /skill /stop /goal…）此阶段仅登记名字使谓词对齐，
//! 处理器行为随对应子系统落地（见 upstream-test-ledger）。

use std::sync::Arc;

use lure_core::command::{
    normalize_command_text, register_builtin_commands, BuiltinDeps, CommandContext, CommandRouter,
};
use lure_core::pairing::PairingStore;
use tempfile::TempDir;

const T0: f64 = 1_000.0;

fn router() -> (TempDir, Arc<PairingStore>, CommandRouter) {
    let dir = TempDir::new().expect("临时目录");
    let pairing = Arc::new(PairingStore::new(dir.path().join("pairing.json")));
    let mut r = CommandRouter::new();
    register_builtin_commands(
        &mut r,
        BuiltinDeps {
            pairing: pairing.clone(),
            clock: Arc::new(|| T0),
        },
    );
    (dir, pairing, r)
}

fn ctx(channel: &str, chat_id: &str, raw: &str) -> CommandContext {
    CommandContext::new(channel, chat_id, format!("{channel}:{chat_id}"), raw)
}

// —— normalize_command_text —— //

#[test]
fn normalize_strips_bot_suffix() {
    assert_eq!(
        normalize_command_text("/trigger@nanobot_bot PR"),
        "/trigger PR"
    );
    assert_eq!(normalize_command_text("/new@bot"), "/new");
    // 非命令原样（去首尾空白）。
    assert_eq!(normalize_command_text("  hello  "), "hello");
    // 无 @ 保持原样。
    assert_eq!(normalize_command_text("/model fast"), "/model fast");
    // @ 在参数里不剥离。
    assert_eq!(
        normalize_command_text("/goal ping @alice"),
        "/goal ping @alice"
    );
}

// —— is_dispatchable_command —— //

#[test]
fn exact_commands_match() {
    let (_d, _p, r) = router();
    for c in [
        "/new",
        "/help",
        "/model",
        "/dream",
        "/dream-log",
        "/dream-restore",
        "/dream-prompt",
        "/goal",
        "/pairing",
    ] {
        assert!(r.is_dispatchable_command(c), "{c} 应可派发");
    }
}

#[test]
fn prefix_commands_match() {
    let (_d, _p, r) = router();
    for c in [
        "/dream-log abc123",
        "/dream-restore def456",
        "/dream-prompt init",
        "/model fast",
        "/goal migrate the database",
        "/pairing list",
        "/pairing approve CODE",
    ] {
        assert!(r.is_dispatchable_command(c), "{c} 应可派发");
    }
}

#[test]
fn priority_commands_not_dispatchable() {
    let (_d, _p, r) = router();
    // priority 层单独由 is_priority 处理，不计入 dispatchable。
    assert!(!r.is_dispatchable_command("/stop"));
    assert!(!r.is_dispatchable_command("/restart"));
    assert!(r.is_priority("/stop"));
    assert!(r.is_priority("/restart"));
}

#[test]
fn regular_text_not_dispatchable() {
    let (_d, _p, r) = router();
    assert!(!r.is_dispatchable_command("hello"));
    assert!(!r.is_dispatchable_command("what is 2+2?"));
    assert!(!r.is_dispatchable_command(""));
}

#[test]
fn dispatchable_is_case_insensitive() {
    let (_d, _p, r) = router();
    assert!(r.is_dispatchable_command("/NEW"));
    assert!(r.is_dispatchable_command("/Help"));
    assert!(r.is_dispatchable_command("/PAIRING"));
}

#[test]
fn dispatchable_strips_whitespace() {
    let (_d, _p, r) = router();
    assert!(r.is_dispatchable_command("  /new  "));
    assert!(r.is_dispatchable_command("  /pairing list  "));
}

#[test]
fn unknown_slash_not_dispatchable() {
    let (_d, _p, r) = router();
    assert!(!r.is_dispatchable_command("/unknown"));
    assert!(!r.is_dispatchable_command("/foo bar"));
}

// —— dispatch: /help —— //

#[test]
fn help_dispatched() {
    let (_d, _p, r) = router();
    let mut c = ctx("test", "chat1", "/help");
    let out = r.dispatch(&mut c).expect("/help 应有输出");
    assert_eq!(out.channel, "test");
    assert_eq!(out.chat_id, "chat1");
    assert_eq!(
        out.metadata.get("render_as").and_then(|v| v.as_str()),
        Some("text")
    );
    assert!(out.content.contains("/new"));
    assert!(out
        .content
        .contains("/pairing [list|approve <code>|deny <code>|revoke <user_id>]"));
}

// —— dispatch: /pairing —— //

#[test]
fn pairing_list_dispatched() {
    let (_d, pairing, r) = router();
    let code = pairing.generate_code("telegram", "123", 600.0, T0).unwrap();

    let mut c = ctx("telegram", "chat1", "/pairing list");
    let out = r.dispatch(&mut c).expect("/pairing list 应有输出");
    assert!(out.content.contains(&code));
    assert_eq!(
        out.metadata
            .get("_pairing_command")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[test]
fn pairing_approve_dispatched() {
    let (_d, pairing, r) = router();
    let code = pairing.generate_code("telegram", "123", 600.0, T0).unwrap();

    let mut c = ctx("telegram", "chat1", &format!("/pairing approve {code}"));
    let out = r.dispatch(&mut c).expect("/pairing approve 应有输出");
    assert_eq!(
        out.content,
        format!("Approved pairing code `{code}` — 123 can now access telegram")
    );
    assert_eq!(
        out.metadata
            .get("_pairing_command")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(pairing.is_approved("telegram", "123").unwrap());
}

#[test]
fn pairing_bare_defaults_to_list() {
    let (_d, pairing, r) = router();
    pairing.generate_code("telegram", "123", 600.0, T0).unwrap();

    // 裸 "/pairing"（exact，args 空）应默认 list。
    let mut c = ctx("telegram", "chat1", "/pairing");
    let out = r.dispatch(&mut c).expect("/pairing 应有输出");
    assert!(out.content.contains("Pending pairing requests:"));
}

// —— dispatch: 非命令 / 前缀 args —— //

#[test]
fn non_command_returns_none() {
    let (_d, _p, r) = router();
    let mut c = ctx("test", "chat1", "hello world");
    assert!(r.dispatch(&mut c).is_none());
}

#[test]
fn prefix_args_populated() {
    let mut r = CommandRouter::new();
    let captured = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = captured.clone();
    r.prefix(
        "/test ",
        Box::new(move |c: &CommandContext| {
            sink.lock().unwrap().push(c.args.clone());
            None
        }),
    );

    let mut c = ctx("test", "c1", "/test hello world");
    r.dispatch(&mut c);
    assert_eq!(*captured.lock().unwrap(), vec!["hello world".to_string()]);
}

// —— dispatch_priority —— //

#[test]
fn priority_bot_suffix_normalized() {
    let (_d, _p, r) = router();
    // 传输后缀经归一后仍命中 priority。
    assert!(r.is_priority("/stop@nanobot_bot"));
}
