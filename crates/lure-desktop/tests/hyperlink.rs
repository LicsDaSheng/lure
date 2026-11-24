//! OSC 8 超链接交互契约：终端内核元数据 + 几何命中 + 打开策略。
//!
//! 这里的用例只读 rio-vt 暴露的单元格元数据与纯函数策略，不启动窗口，
//! 因此可以在 CI 中稳定复现 iTerm2 的「按住 Cmd 悬停路径可点击」行为。

use std::path::{Path, PathBuf};

use lure_desktop::links::LinkKind;
use lure_desktop::opener::{OpenError, OpenTarget, resolve_link, resolve_target};
use lure_desktop::terminal::{
    CELL_HEIGHT, CELL_WIDTH, TERMINAL_PADDING, TITLEBAR_HEIGHT, TerminalGeometry, TerminalGrid,
};

const SIDEBAR: f32 = 220.0;

fn grid(rows: u16, columns: u16) -> TerminalGrid {
    TerminalGrid::new(rows, columns)
}

/// 把单元格中心点换算回窗口坐标，验证「像素 → 单元格 → 链接」整条链路。
fn center_of(geometry_offset: (f32, f32), cell: (u16, u16)) -> (f32, f32) {
    (
        geometry_offset.0 + f32::from(cell.1) * CELL_WIDTH + CELL_WIDTH / 2.0,
        geometry_offset.1 + f32::from(cell.0) * CELL_HEIGHT + CELL_HEIGHT / 2.0,
    )
}

#[test]
fn hover_on_file_link_resolves_to_the_system_open_target() {
    let mut terminal = grid(6, 40);
    terminal.process(
        "已修改 \x1b]8;;file:///Users/me/project/src/main.rs\x1b\\src/main.rs\x1b]8;;\x1b\\ 完成"
            .as_bytes(),
    );
    let geometry = terminal.geometry(SIDEBAR);

    let links = terminal.visible_links();
    assert_eq!(links.len(), 1);
    let link = &links[0];
    assert_eq!(link.uri, "file:///Users/me/project/src/main.rs");
    assert_eq!(link.row, 0);

    // 链接起点前的状态文字不命中，链接自身每一列都命中。
    let origin = (
        SIDEBAR + TERMINAL_PADDING / 2.0,
        TITLEBAR_HEIGHT + TERMINAL_PADDING / 2.0,
    );
    let before = center_of(origin, (0, link.column.saturating_sub(1)));
    assert!(geometry.cell_at(before.0, before.1).is_some());
    assert!(
        terminal
            .link_at(geometry.cell_at(before.0, before.1).unwrap())
            .is_none()
    );

    let inside = center_of(origin, (link.row, link.column));
    let cell = geometry
        .cell_at(inside.0, inside.1)
        .expect("链接单元格应在网格内");
    assert_eq!(
        terminal.link_at(cell).map(|link| link.uri),
        Some(link.uri.clone())
    );

    assert_eq!(
        resolve_target(&link.uri),
        Ok(OpenTarget::File(PathBuf::from(
            "/Users/me/project/src/main.rs"
        )))
    );
}

#[test]
fn web_link_follows_the_system_browser_without_rewriting_the_uri() {
    let mut terminal = grid(4, 40);
    terminal.process(b"\x1b]8;;https://example.dev/a?b=1#c\x1b\\docs\x1b]8;;\x1b\\");
    let link = terminal.visible_links().pop().expect("应有链接");
    assert_eq!(
        resolve_target(&link.uri),
        Ok(OpenTarget::Url("https://example.dev/a?b=1#c".to_string()))
    );
    assert_eq!(
        resolve_target(&link.uri).map(|target| target.url()),
        Ok("https://example.dev/a?b=1#c".to_string())
    );
}

#[test]
fn unsupported_schemes_are_ignored_without_starting_the_system_handler() {
    for uri in [
        "javascript:alert(document.cookie)",
        "ssh://host/path",
        "vscode://file/etc/hosts",
        "custom-handler://payload",
    ] {
        assert!(
            matches!(resolve_target(uri), Err(OpenError::UnsupportedScheme(_))),
            "{uri} 不应被交给系统默认处理器"
        );
    }
}

#[test]
fn expanding_sidebar_shifts_the_hit_test_region() {
    let mut terminal = grid(4, 40);
    // 链接从第 6 列开始，展开/收起侧栏时同一窗口坐标会落到不同单元格。
    terminal.process(b"note: \x1b]8;;https://example.dev\x1b\\x\x1b]8;;\x1b\\");
    let link = terminal.visible_links().pop().expect("应有链接");
    assert_eq!(link.column, 6);

    let expanded = terminal.geometry(SIDEBAR);
    let collapsed = terminal.geometry(0.0);
    let origin = (
        SIDEBAR + TERMINAL_PADDING / 2.0,
        TITLEBAR_HEIGHT + TERMINAL_PADDING / 2.0,
    );
    let point = center_of(origin, (link.row, link.column));

    let hit = |geometry: TerminalGeometry| {
        geometry
            .cell_at(point.0, point.1)
            .and_then(|cell| terminal.link_at(cell))
            .map(|link| link.uri)
    };

    // 侧栏展开时命中链接；收起侧栏后网格整体左移 220px，同一点落到别的列。
    assert_eq!(hit(expanded), Some("https://example.dev".to_string()));
    assert_eq!(hit(collapsed), None);

    let collapsed_origin = (
        TERMINAL_PADDING / 2.0,
        TITLEBAR_HEIGHT + TERMINAL_PADDING / 2.0,
    );
    let collapsed_point = center_of(collapsed_origin, (link.row, link.column));
    assert_eq!(
        collapsed
            .cell_at(collapsed_point.0, collapsed_point.1)
            .and_then(|cell| terminal.link_at(cell))
            .map(|link| link.uri),
        Some("https://example.dev".to_string())
    );
}

#[test]
fn titlebar_sidebar_and_padding_are_never_clickable_link_areas() {
    let mut terminal = grid(4, 40);
    terminal.process(b"\x1b]8;;https://example.dev\x1b\\link\x1b]8;;\x1b\\");
    let geometry = terminal.geometry(SIDEBAR);

    // 标题栏、终端内边距、侧栏与网格右侧空白都不产生单元格，因而不产生链接命中。
    for point in [
        (400.0, 20.0),
        (240.0, TITLEBAR_HEIGHT + 2.0),
        (100.0, 200.0),
        (SIDEBAR + TERMINAL_PADDING + 8.0 * 40.0 + 4.0, 200.0),
    ] {
        assert!(
            geometry
                .cell_at(point.0, point.1)
                .is_none_or(|cell| terminal.link_at(cell).is_none()),
            "{point:?} 不应命中链接"
        );
    }
}

#[test]
fn links_scroll_with_their_rows_and_do_not_pin_to_old_coordinates() {
    let mut terminal = grid(3, 24);
    terminal.process(b"\x1b]8;;https://example.dev\x1b\\top\x1b]8;;\x1b\\\r\n");
    for _ in 0..6 {
        terminal.process(b"more\r\n");
    }
    // 链接已滚出视口，不应在原行残留命中。
    assert!(terminal.visible_links().is_empty());

    // 滚到历史顶部后链接重新进入视口，并跟随所在行移动。
    terminal.scroll_by(100);
    assert!(terminal.scrollback() > 0);
    let links = terminal.visible_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].uri, "https://example.dev");
    assert!(terminal.link_at((links[0].row, links[0].column)).is_some());
    assert!(
        terminal
            .link_at((links[0].row + 1, links[0].column))
            .is_none()
    );
}

#[test]
fn empty_grid_columns_never_produce_link_spans() {
    let mut terminal = grid(1, 2);
    terminal.process(b"\x1b]8;;https://example.dev\x1b\\ab\x1b]8;;\x1b\\");
    let links = terminal.visible_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].columns, 2);
    assert_eq!(terminal.link_at((0, 2)), None, "越界列不应命中");
}

#[test]
fn geometry_reports_no_cell_for_degenerate_grids() {
    let geometry = TerminalGeometry::new(SIDEBAR, 0, 0);
    assert_eq!(geometry.cell_at(400.0, 400.0), None);
}

#[test]
fn real_pi_image_fallback_bytes_resolve_to_an_openable_file_link() {
    // 该序列由 Pi 的 imageFallback 在 PI_HYPERLINKS=1 时产出：
    //   [Image: ESC]8;;file:///tmp/shot.png ESC\ /tmp/shot.png ESC]8;; ESC\ [image/png] ...
    let bytes = b"[Image: \x1b]8;;file:///tmp/shot.png\x1b\\/tmp/shot.png\x1b]8;;\x1b\\ [image/png] 800x450]";
    let mut terminal = grid(4, 80);
    terminal.process(bytes);

    let link = terminal
        .visible_links()
        .pop()
        .expect("Pi 的 OSC 8 应被解析为链接");
    assert_eq!(link.uri, "file:///tmp/shot.png");
    // 链接只覆盖可见路径文字，不覆盖周围的 `[Image: ` 与类型说明。
    assert_eq!(link.column, 8);
    assert_eq!(link.columns, u16::try_from("/tmp/shot.png".len()).unwrap());
    assert_eq!(
        resolve_target(&link.uri),
        Ok(OpenTarget::File(PathBuf::from("/tmp/shot.png")))
    );
}

#[test]
fn diagnostic_bel_terminated_osc8() {
    // Pi 的 SEGMENT_RESET 使用 BEL（\x07）终止 OSC 8，需确认 rio-vt 是否识别。
    let mut a = grid(2, 40);
    a.process(b"\x1b]8;;https://a.dev\x1b\\txt\x1b]8;;\x1b\\");
    eprintln!("ST  terminated: {:?}", a.visible_links().len());
    let mut b = grid(2, 40);
    b.process(b"\x1b]8;;https://b.dev\x07txt\x1b]8;;\x07");
    eprintln!("BEL terminated: {:?}", b.visible_links().len());
    eprintln!("BEL links = {:?}", b.visible_links());
}

#[test]
fn real_pi_tool_card_bytes_expose_the_path_as_a_clickable_link() {
    // 该片段来自真实 Pi 1.5.2 交互式 TUI 的 Read 工具卡片输出（PTY 原始字节）：
    // `✓ Read crates/lure-desktop/src/opener.rs`，路径是纯文本、没有 OSC 8。
    let bytes = b"\x1b[2;1H\x1b[2K \x1b[38;2;80;220;100m\xe2\x9c\x93\x1b[39m \x1b[39mRead\x1b[39m \x1b[39m\x1b[2mcrates/lure-desktop/src/opener.rs\x1b[22;39m\x1b[39m\x1b[0m\x1b]8;;\x07";
    let mut terminal = grid(6, 100);
    terminal.process(bytes);

    let links = terminal.visible_links();
    assert_eq!(links.len(), 1, "应恰好识别出卡片里的路径：{links:?}");
    let link = &links[0];
    assert_eq!(link.uri, "crates/lure-desktop/src/opener.rs");
    assert_eq!(link.kind, LinkKind::Path);
    // 卡片前缀 ` ✓ Read ` 共 8 列（U+2713 属窄字符，占 1 列），路径从第 8 列开始。
    assert_eq!(link.row, 1);
    assert_eq!(link.column, 8);

    // 相对路径以项目目录为基准解析为可打开的文件。
    assert_eq!(
        resolve_link(link.kind, &link.uri, Path::new("/Users/me/project")),
        Ok(OpenTarget::File(PathBuf::from(
            "/Users/me/project/crates/lure-desktop/src/opener.rs"
        )))
    );
}

#[test]
fn real_pi_startup_bytes_do_not_fake_links_out_of_ui_chrome() {
    // Pi 启动时的界面框架：分隔线、状态行、提示文字都不得变成链接。
    let bytes = b"\x1b[1m\xe2\x94\x80\xe2\x94\x80\xe2\x94\x80\x1b[39m\x1b[2m \xe2\xa0\xbc Working... (\xe2\x86\x93 355 tokens \xc2\xb7 14s)\x1b[39m\r\n  \x1b[2m252 lines loaded\x1b[39m \xc2\xb7 \x1b[22mclick to show more\x1b[0m\r\n";
    let mut terminal = grid(6, 100);
    terminal.process(bytes);
    assert!(
        terminal.visible_links().is_empty(),
        "界面框架不应产生链接：{:?}",
        terminal.visible_links()
    );
}

#[test]
fn soft_wrapped_long_path_is_clickable_across_rows() {
    let mut terminal = grid(4, 24);
    // 24 列太窄，长路径必然软换行。`\x1b[7m` 只为干扰样式，不影响识别。
    terminal.process(b"edit crates/lure-desktop/src/opener.rs now");

    let links: Vec<_> = terminal
        .visible_links()
        .into_iter()
        .filter(|link| link.uri.contains("opener.rs"))
        .collect();
    assert!(!links.is_empty(), "软换行后的长路径应仍被识别");
    // 跨行时按行拆成多个片段，目标一致。
    assert!(links.len() >= 2, "应拆成多行片段：{links:?}");
    for link in &links {
        assert_eq!(link.uri, "crates/lure-desktop/src/opener.rs");
        assert!(terminal.link_at((link.row, link.column)).is_some());
    }
    let rows: Vec<u16> = links.iter().map(|link| link.row).collect();
    assert!(
        rows.windows(2).all(|pair| pair[0] < pair[1]),
        "片段应按行递增"
    );
}

#[test]
fn link_scanning_stays_cheap_on_a_full_viewport() {
    use std::time::Instant;

    let mut terminal = grid(40, 200);
    // 铺满带路径的文本，测量单次全视口识别成本。
    for row in 0..40 {
        terminal.process(
            format!("row {row} crates/lure-desktop/src/opener.rs 中文内容 a/b\r\n").as_bytes(),
        );
    }
    let start = Instant::now();
    for _ in 0..20 {
        let _ = terminal.visible_links();
    }
    let per_scan = start.elapsed() / 20;
    // 全视口 8000 格；缓存保证每帧不会重算，这里只挡明显退化。
    assert!(
        per_scan.as_micros() < 20_000,
        "单次全视口链接识别耗时 {per_scan:?}，超出预期"
    );
}
