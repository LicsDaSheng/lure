//! CLI one-shot 闭环验证。
//!
//! 映射上游 `tests/cli/` 的直接 agent 语义：`lure agent -m` 走 one-shot；
//! `lure agent` 进入可持续输入的交互模式。

use std::io::Write;
use std::process::Command;
use std::process::Stdio;

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
fn agent_without_message_enters_interactive_and_exits_on_eof() {
    let output = lure().args(["agent"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Lure interactive mode"));
    assert!(stdout.contains("Goodbye!"));
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

#[test]
fn interactive_mode_processes_multiple_turns_until_exit() {
    let dir = tempdir().unwrap();
    let mut child = lure()
        .args(["agent", "--workspace", dir.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"hello\nsecond\nexit\n").unwrap();
    }

    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Lure interactive mode"));
    assert!(stdout.contains("Assistant: echo: hello"));
    assert!(stdout.contains("Assistant: echo: second"));
    assert!(stdout.contains("Goodbye!"));

    let session_text = std::fs::read_to_string(
        std::fs::read_dir(dir.path().join("sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert!(session_text.contains("\"content\":\"hello\""));
    assert!(session_text.contains("\"content\":\"echo: hello\""));
    assert!(session_text.contains("\"content\":\"second\""));
    assert!(session_text.contains("\"content\":\"echo: second\""));
}

#[test]
fn interactive_mode_ignores_blank_lines_and_accepts_session_alias() {
    let dir = tempdir().unwrap();
    let mut child = lure()
        .args([
            "agent",
            "--session",
            "scratch",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"\nhello\n:q\n").unwrap();
    }

    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("Assistant:").count(), 1);
    assert!(stdout.contains("cli:scratch"));
    assert_eq!(
        std::fs::read_dir(dir.path().join("sessions"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn one_shot_accepts_explicit_session_id() {
    let dir = tempdir().unwrap();
    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--session",
            "cli:custom",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let session_text = std::fs::read_to_string(
        std::fs::read_dir(dir.path().join("sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path(),
    )
    .unwrap();
    assert!(session_text.contains("\"key\":\"cli:custom\""));
}
