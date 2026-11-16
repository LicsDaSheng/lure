use std::ffi::OsString;
use std::time::{Duration, Instant};

use lure_pty::{PtyCommand, PtyEvent, PtyProcess, TerminalSize};

#[test]
fn rejects_zero_terminal_dimensions() {
    let command = PtyCommand::new("ignored", ".");
    let error = PtyProcess::spawn(command, TerminalSize::new(0, 24)).unwrap_err();
    assert!(error.to_string().contains("终端列数和行数必须大于零"));
}

#[cfg(unix)]
#[test]
fn streams_output_and_accepts_input_through_the_pty() {
    let mut command = PtyCommand::new("/bin/sh", ".");
    command.args = vec![
        OsString::from("-c"),
        OsString::from("read value; printf 'echo:%s' \"$value\""),
    ];
    let (mut process, events) = PtyProcess::spawn(command, TerminalSize::new(80, 24)).unwrap();

    process.write_all(b"hello\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut output = Vec::new();
    let mut exited = false;
    while Instant::now() < deadline && !exited {
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(PtyEvent::Output(bytes)) => output.extend(bytes),
            Ok(PtyEvent::Exited(status)) => {
                assert!(status.success);
                exited = true;
            }
            Ok(PtyEvent::ReadFailed(message)) => panic!("PTY 读取失败：{message}"),
            Err(_) => {}
        }
    }

    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("echo:hello"), "实际输出：{text:?}");
    assert!(exited, "子进程未在超时内退出");
}

#[cfg(unix)]
#[test]
fn resizes_and_stops_a_running_process() {
    let mut command = PtyCommand::new("/bin/sh", ".");
    command.args = vec![OsString::from("-c"), OsString::from("sleep 30")];
    let (mut process, events) = PtyProcess::spawn(command, TerminalSize::new(80, 24)).unwrap();

    process.resize(TerminalSize::new(120, 40)).unwrap();
    assert_eq!(process.size().unwrap(), TerminalSize::new(120, 40));
    process.stop().unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(PtyEvent::Exited(_)) = events.recv_timeout(Duration::from_millis(100)) {
            return;
        }
    }
    panic!("停止后没有收到退出事件");
}
