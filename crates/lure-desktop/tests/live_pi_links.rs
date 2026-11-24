//! 用真实运行的 Pi 进程验证链接通道。
//!
//! 默认跳过；设置 `LURE_RUN_LIVE_PI_LINKS=1` 启用。该用例在 PTY 中启动真实
//! Pi，喂一轮工具调用，再从终端网格上取链接，确认「路径可点」在真实输出上成立。

use std::time::{Duration, Instant};

use lure_desktop::links::LinkKind;
use lure_desktop::terminal::TerminalGrid;
use lure_pty::{
    PtyCommand, PtyEvent, PtyProcess, SpawnEnvResolver, TerminalSize, prune_host_terminal_marks,
};

#[test]
fn live_pi_tool_card_path_becomes_a_clickable_link() {
    if std::env::var_os("LURE_RUN_LIVE_PI_LINKS").is_none() {
        eprintln!("跳过真实 Pi 链接冒烟测试；设置 LURE_RUN_LIVE_PI_LINKS=1 启用");
        return;
    }

    let mut command = PtyCommand::new(
        std::env::var_os("LURE_PI_PATH").unwrap_or_else(|| "pi".into()),
        env!("CARGO_MANIFEST_DIR"),
    );
    // 完全复刻 Lure 的 spawn 环境：登录 shell 合成 + 系统代理，
    // 清理宿主终端标记后只声明超链接能力。缺代理会让 Pi 无法联网。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("应能创建测试运行时");
    let mut environment = runtime.block_on(SpawnEnvResolver::new().resolve());
    environment = prune_host_terminal_marks(environment);
    for (key, value) in [
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
        ("TERM_PROGRAM", "Lure"),
        ("PI_HYPERLINKS", "1"),
    ] {
        environment.retain(|(name, _)| name != key);
        environment.push((key.into(), value.into()));
    }
    command.environment = Some(environment);

    let (mut process, events) =
        PtyProcess::spawn(command, TerminalSize::new(120, 40)).expect("应能启动真实 Pi");
    let mut grid = TerminalGrid::new(40, 120);

    // 排空启动画面。
    let settle = Instant::now() + Duration::from_secs(6);
    while Instant::now() < settle {
        if let Ok(PtyEvent::Output(bytes)) = events.recv_timeout(Duration::from_millis(200)) {
            grid.process(&bytes);
        }
    }

    process
        .write_all(b"read the file crates/lure-desktop/src/opener.rs\r")
        .expect("应能向 Pi 输入");

    // 工具卡片里的路径是纯文本；输入行也会被识别，因此必须等工具真正执行完，
    // 并在「以卡片标记开头」的行上断言，才证明纯文本路径通道有效。
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut found = None;
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(300)) {
            Ok(PtyEvent::Output(bytes)) => {
                grid.process(&bytes);
                let lines = grid.lines();
                found = grid.visible_links().into_iter().find(|link| {
                    link.uri.contains("crates/lure-desktop/src/opener.rs")
                        && lines.get(usize::from(link.row)).is_some_and(|line| {
                            let text = line.text();
                            // 只接受 Read 工具卡片，不把用户输入或助手复述当作成功。
                            let card = text.trim_start();
                            (card.starts_with('✓') || card.starts_with('✔'))
                                && card.contains("Read")
                        })
                });
                if found.is_some() {
                    break;
                }
            }
            Ok(PtyEvent::Exited(status)) => panic!("Pi 提前退出：{status:?}"),
            Ok(PtyEvent::ReadFailed(message)) => panic!("PTY 读取失败：{message}"),
            Err(_) => {}
        }
    }

    let _ = process.stop();
    // 网络不可用时 Pi 无法完成工具调用；这属于外部条件而非链接通道缺陷。
    let screen: String = grid
        .lines()
        .iter()
        .map(lure_desktop::terminal::TerminalLine::text)
        .collect();
    if screen.contains("Connect Timeout")
        || screen.contains("fetch failed")
        || screen.contains("ENOTFOUND")
    {
        eprintln!("跳过：当前网络无法连接模型服务，未能触发工具卡片");
        return;
    }
    let Some(link) = found else {
        // 诊断：打印最终可见屏幕，便于判断是 Pi 未完成还是识别遗漏。
        for (index, line) in grid.lines().iter().enumerate() {
            let text = line.text();
            if !text.trim().is_empty() {
                eprintln!("{index:3}: {}", text.trim_end());
            }
        }
        panic!("真实 Pi 的 Read 卡片里应能识别出路径链接");
    };
    // 打印命中行，确认链接来自工具卡片而不是其他位置。
    let lines = grid.lines();
    if let Some(line) = lines.get(usize::from(link.row)) {
        eprintln!("命中的可见行：{:?}", line.text().trim_end());
    }
    eprintln!("识别出的链接：{link:?}");
    assert!(
        link.uri.contains("crates/lure-desktop/src/opener.rs"),
        "识别结果应指向卡片路径：{link:?}"
    );
    assert_eq!(link.kind, LinkKind::Path, "纯文本路径应标记为 Path 类型");
    assert!(
        grid.link_at((link.row, link.column)).is_some(),
        "该单元格必须可命中"
    );
}
