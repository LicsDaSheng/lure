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
fn model_flag_routes_through_resolver_and_reports_missing_key() {
    // `--model deepseek-chat` 经 resolver 匹配到 deepseek，缺 key 时报出对应 env 变量。
    let dir = tempdir().unwrap();
    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--model",
            "deepseek-chat",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .env_remove("DEEPSEEK_API_KEY")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("DEEPSEEK_API_KEY"),
        "stderr 应提示缺少 DEEPSEEK_API_KEY，实际: {stderr}"
    );
}

#[test]
fn model_flag_unmatchable_provider_fails_via_resolver() {
    let dir = tempdir().unwrap();
    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--model",
            "mystery-xyz",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("mystery-xyz"),
        "stderr 应提示无法为该模型匹配 provider，实际: {stderr}"
    );
}

#[test]
fn preset_flag_admits_named_preset_from_config_file() {
    // config 文件里的命名 preset 应被 resolver 选中：fast → deepseek，缺 key 报对应 env 变量。
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(
        &config_path,
        r#"{"modelPresets":{"fast":{"model":"deepseek-chat","provider":"auto"}}}"#,
    )
    .unwrap();

    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--config",
            config_path.to_str().unwrap(),
            "--preset",
            "fast",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .env_remove("DEEPSEEK_API_KEY")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("DEEPSEEK_API_KEY"),
        "stderr 应提示缺少 DEEPSEEK_API_KEY，实际: {stderr}"
    );
}

#[test]
fn preset_flag_unknown_preset_reports_not_found() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(
        &config_path,
        r#"{"modelPresets":{"fast":{"model":"deepseek-chat","provider":"auto"}}}"#,
    )
    .unwrap();

    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--config",
            config_path.to_str().unwrap(),
            "--preset",
            "nope",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("nope"),
        "stderr 应提示 preset 'nope' 不存在，实际: {stderr}"
    );
}

#[test]
fn preset_uses_config_api_key_and_api_base_override() {
    // config 提供 apiKey 与 apiBase：resolver 应用 base 覆盖、CLI 用 config key（不再要 env）。
    // apiBase 指向本地未监听端口，出网即刻失败——证明已越过缺 key 检查、走到真实传输。
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(
        &config_path,
        r#"{"providers":{"deepseek":{"apiKey":"sk-test","apiBase":"http://127.0.0.1:1/v1"}},"modelPresets":{"fast":{"model":"deepseek-chat","provider":"auto"}}}"#,
    )
    .unwrap();

    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--config",
            config_path.to_str().unwrap(),
            "--preset",
            "fast",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .env_remove("DEEPSEEK_API_KEY")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stderr.contains("DEEPSEEK_API_KEY"),
        "config 提供 key 后不应再报缺 env key，实际: {stderr}"
    );
    assert!(
        stderr.contains("传输"),
        "应走到真实传输并失败，实际: {stderr}"
    );
}

#[test]
fn exec_tool_invalid_pattern_surfaces_error_before_provider() {
    // config 启用 exec 但 allow 含非法正则：工具注册应在 provider 出网前就失败。
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    std::fs::write(
        &config_path,
        r#"{"tools":{"exec":{"enabled":true,"allow":["["]}},"modelPresets":{"fast":{"model":"deepseek-chat","provider":"auto"}}}"#,
    )
    .unwrap();

    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--config",
            config_path.to_str().unwrap(),
            "--preset",
            "fast",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .env_remove("DEEPSEEK_API_KEY")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("exec"),
        "stderr 应提示 exec 策略正则无效，实际: {stderr}"
    );
}

#[test]
fn preset_and_model_flags_are_mutually_exclusive() {
    let dir = tempdir().unwrap();
    let output = lure()
        .args([
            "agent",
            "-m",
            "hello",
            "--preset",
            "fast",
            "--model",
            "deepseek-chat",
            "--workspace",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("--preset") && stderr.contains("--model"),
        "stderr 应提示 --preset 与 --model 互斥，实际: {stderr}"
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
