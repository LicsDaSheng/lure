//! 用真实 Pi 会话输出检验路径识别的误判率。

use lure_desktop::terminal::TerminalGrid;

const PI_TOOL_BYTES: &[u8] = include_bytes!("fixtures/pi_tool.bin");

#[test]
fn real_pi_session_produces_only_plausible_links() {
    let mut grid = TerminalGrid::new(30, 120);
    // 分块喂入，模拟 PTY 任意切分边界（含多字节字符中间）。
    for chunk in PI_TOOL_BYTES.chunks(97) {
        grid.process(chunk);
    }
    let links = grid.visible_links();
    let targets: Vec<&str> = links.iter().map(|link| link.uri.as_str()).collect();

    // 少量合法目标可接受；关键是不得出现界面框架、数字、裸单词。
    for target in &targets {
        assert!(
            target.contains('/') || target.contains('.'),
            "识别出可疑目标：{target:?}"
        );
    }

    let joined = targets.join("\n");
    for false_positive in [
        "Working",
        "tokens",
        "click",
        "lines",
        "Read",
        "1.5.2",
        "image/png",
    ] {
        assert!(
            !joined.contains(false_positive),
            "误判：{false_positive} 出现在 {joined}"
        );
    }

    eprintln!("真实会话识别出的 {} 个链接：{targets:?}", targets.len());
}
