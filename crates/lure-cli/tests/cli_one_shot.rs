//! CLI one-shot 闭环验证。
//!
//! 映射上游 `tests/cli/` 的 one-shot 语义：`lure agent -m` 走 EchoProvider 跑完
//! agent loop，输出回复并持久化 session。interactive 模式属后续。

use std::process::Command;

use tempfile::tempdir;

fn lure() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lure"))
}

#[test]
fn agent_one_shot_prints_reply_and_persists_session() {
    let dir = tempdir().unwrap();
    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "echo: hello"
    );

    let sessions_dir = dir.path().join("sessions");
    let session_files = std::fs::read_dir(&sessions_dir).unwrap().count();
    assert_eq!(session_files, 1, "应持久化一个会话文件");
}

#[test]
fn no_subcommand_prints_version() {
    let output = lure().output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .starts_with("lure "));
}

#[test]
fn agent_without_message_fails() {
    let output = lure().args(["agent"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn show_reasoning_flag_is_accepted_and_stdout_stays_answer() {
    let dir = tempdir().unwrap();
    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--show-reasoning",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    // EchoProvider 无 reasoning：stdout 仍是纯答案，可脚本化。
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "echo: hello"
    );
}
