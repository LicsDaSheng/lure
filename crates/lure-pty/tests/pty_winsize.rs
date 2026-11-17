use lure_pty::{PtyCommand, PtyEvent, PtyProcess, TerminalSize};
use std::ffi::OsString;
use std::path::PathBuf;

/// 终端应用（如 Pi）用 `ws_xpixel` / `ws_ypixel` 推导字符单元尺寸，
/// 内联图片的可见尺寸与 TUI 布局都依赖它，因此必须与网格同步。
#[test]
fn pty_reports_pixel_size_to_child() {
    let command = PtyCommand {
        executable: PathBuf::from("/usr/bin/python3"),
        args: vec![
            OsString::from("-c"),
            OsString::from(
                "import fcntl,termios,struct;print(struct.unpack('HHHH', fcntl.ioctl(0, termios.TIOCGWINSZ, b'\\0'*8)))",
            ),
        ],
        working_directory: std::env::temp_dir(),
        environment: None,
    };
    let (mut process, events) =
        PtyProcess::spawn(command, TerminalSize::with_pixels(80, 24, 640, 384)).expect("spawn");
    let mut output = String::new();
    while let Ok(event) = events.recv() {
        match event {
            PtyEvent::Output(bytes) => output.push_str(&String::from_utf8_lossy(&bytes)),
            PtyEvent::Exited { .. } | PtyEvent::ReadFailed { .. } => break,
        }
    }
    let _ = process.stop();
    assert!(
        output.contains("(24, 80, 640, 384)"),
        "子进程应看到 (rows, cols, xpixel, ypixel) 像素尺寸: {output}"
    );
}

#[test]
fn resize_keeps_pixel_size_in_sync() {
    let command = PtyCommand {
        executable: PathBuf::from("/bin/sh"),
        args: vec![OsString::from("-c"), OsString::from("sleep 1")],
        working_directory: std::env::temp_dir(),
        environment: None,
    };
    let (mut process, _events) =
        PtyProcess::spawn(command, TerminalSize::with_pixels(80, 24, 640, 384)).expect("spawn");
    process
        .resize(TerminalSize::with_pixels(120, 40, 960, 640))
        .expect("resize");
    let size = process.size().expect("size");
    assert_eq!(
        (size.columns, size.rows, size.pixel_width, size.pixel_height),
        (120, 40, 960, 640)
    );
    let _ = process.stop();
}
