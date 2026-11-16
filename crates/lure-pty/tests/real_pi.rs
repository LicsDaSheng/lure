use std::time::{Duration, Instant};

use lure_pty::{PtyCommand, PtyEvent, PtyProcess, TerminalSize};

#[test]
fn starts_the_installed_pi_tui_in_a_real_pty() {
    if std::env::var_os("LURE_RUN_REAL_PI_PTY").is_none() {
        eprintln!("跳过真实 Pi PTY 冒烟测试；设置 LURE_RUN_REAL_PI_PTY=1 启用");
        return;
    }

    let executable = std::env::var_os("LURE_PI_PATH").unwrap_or_else(|| "pi".into());
    let command = PtyCommand::new(executable, env!("CARGO_MANIFEST_DIR"));
    let (mut process, events) =
        PtyProcess::spawn(command, TerminalSize::new(100, 30)).expect("应能在 PTY 中启动 Pi");

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut output = Vec::new();
    while Instant::now() < deadline && output.len() < 32 {
        match events.recv_timeout(Duration::from_millis(250)) {
            Ok(PtyEvent::Output(bytes)) => output.extend(bytes),
            Ok(PtyEvent::ReadFailed(message)) => panic!("读取 Pi TUI 失败：{message}"),
            Ok(PtyEvent::Exited(status)) => {
                panic!("Pi 在产生 TUI 输出前退出：{status:?}")
            }
            Err(_) => {}
        }
    }

    process.stop().expect("应能停止真实 Pi PTY");
    assert!(output.len() >= 32, "Pi TUI 没有产生足够的终端输出");
    assert!(output.contains(&0x1b), "Pi TUI 输出应包含 ANSI 转义序列");
}
