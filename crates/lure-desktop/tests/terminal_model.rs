use lure_desktop::input::{KeyInput, Modifiers, terminal_input};
use lure_desktop::terminal::{
    TERMINAL_FONT_FALLBACKS, TERMINAL_FONT_FAMILY, TerminalGrid, TerminalSelection,
    terminal_size_for_viewport,
};
use lure_desktop::theme::{ANSI_COLORS, BACKGROUND, FOREGROUND, indexed_color};

#[test]
fn parses_ansi_output_without_exposing_escape_sequences() {
    let mut grid = TerminalGrid::new(3, 12);
    grid.process(b"plain \x1b[31mred\x1b[0m");

    let lines = grid.lines();
    assert_eq!(lines[0].text(), "plain red   ");
    assert_eq!(lines[0].cells()[6].foreground, ANSI_COLORS[1]);
    assert_eq!(lines[0].cells()[0].foreground, FOREGROUND);
    assert_eq!(lines[0].cells()[0].background, BACKGROUND);
}

#[test]
fn preserves_alternate_screen_and_cursor_position() {
    let mut grid = TerminalGrid::new(2, 8);
    grid.process(b"main\x1b[?1049h\x1b[Hfullscreen\x1b[2;3H!");

    let lines = grid.lines();
    assert!(lines[0].text().starts_with("fullscre"));
    assert_eq!(grid.cursor(), (1, 3));

    grid.process(b"\x1b[?1049l");
    assert!(grid.lines()[0].text().starts_with("main"));
}

#[test]
fn terminal_cursor_visibility_tracks_vt_mode_and_scrollback() {
    let mut grid = TerminalGrid::new(2, 12);
    assert!(grid.cursor_visible());
    grid.process(b"\x1b[?25l");
    assert!(!grid.cursor_visible());
    grid.process(b"\x1b[?25h");
    assert!(grid.cursor_visible());
    for _ in 0..5 {
        grid.process(b"line\r\n");
    }
    grid.scroll_by(1);
    assert!(!grid.cursor_visible());
    grid.scroll_to_bottom();
    assert!(grid.cursor_visible());
}

#[test]
fn mouse_click_is_forwarded_only_when_terminal_requests_it() {
    let mut grid = TerminalGrid::new(3, 20);
    assert_eq!(grid.mouse_button_sequence(1, 2, false), None);
    grid.process(b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        grid.mouse_button_sequence(1, 2, false),
        Some(b"\x1b[<0;3;2M".to_vec())
    );
    assert_eq!(
        grid.mouse_button_sequence(1, 2, true),
        Some(b"\x1b[<0;3;2m".to_vec())
    );
    grid.process(b"\x1b[?1000l");
    assert_eq!(grid.mouse_button_sequence(1, 2, false), None);
}

#[test]
fn application_cursor_mode_changes_arrow_key_sequences() {
    let mut grid = TerminalGrid::new(2, 12);
    let up = KeyInput::plain("up", None);
    assert_eq!(grid.keyboard_input(&up), Some(b"\x1b[A".to_vec()));
    grid.process(b"\x1b[?1h");
    assert_eq!(grid.keyboard_input(&up), Some(b"\x1bOA".to_vec()));
    grid.process(b"\x1b[?1l");
    assert_eq!(grid.keyboard_input(&up), Some(b"\x1b[A".to_vec()));
}

#[test]
fn keyboard_protocol_queries_are_answered_through_the_pty_channel() {
    let mut grid = TerminalGrid::new(2, 12);
    grid.process(b"\x1b[?u");
    assert_eq!(grid.take_responses(), vec!["\x1b[?0u"]);
    assert!(grid.take_responses().is_empty());
    grid.process(b"\x1b[>1u\x1b[?u");
    assert_eq!(grid.take_responses(), vec!["\x1b[?1u"]);
}

#[test]
fn shift_enter_uses_extended_keyboard_mode_only_when_requested() {
    let mut grid = TerminalGrid::new(2, 12);
    let enter = KeyInput::new(
        "enter",
        None,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(grid.keyboard_input(&enter), Some(b"\r".to_vec()));
    grid.process(b"\x1b[>1u");
    assert_eq!(grid.keyboard_input(&enter), Some(b"\x1b[13;2u".to_vec()));
}

#[test]
fn resizes_terminal_from_available_pixels_with_lower_bounds() {
    assert_eq!(terminal_size_for_viewport(800.0, 600.0), (98, 33));
    assert_eq!(terminal_size_for_viewport(1.0, 1.0), (2, 1));
}

#[test]
fn maps_text_navigation_control_and_paste_shortcuts_to_terminal_bytes() {
    assert_eq!(
        terminal_input(&KeyInput::plain("a", Some("a"))),
        Some(b"a".to_vec())
    );
    assert_eq!(
        terminal_input(&KeyInput::plain("enter", None)),
        Some(b"\r".to_vec())
    );
    assert_eq!(
        terminal_input(&KeyInput::plain("up", None)),
        Some(b"\x1b[A".to_vec())
    );
    assert_eq!(
        terminal_input(&KeyInput::new(
            "c",
            None,
            Modifiers {
                control: true,
                ..Default::default()
            }
        )),
        Some(vec![3])
    );
    assert_eq!(
        terminal_input(&KeyInput::new(
            "v",
            None,
            Modifiers {
                platform: true,
                ..Default::default()
            }
        )),
        None,
        "平台粘贴快捷键由剪贴板路径处理"
    );
}

#[test]
fn alt_prefixes_text_and_function_keys_use_xterm_sequences() {
    assert_eq!(
        terminal_input(&KeyInput::new(
            "x",
            Some("x"),
            Modifiers {
                alt: true,
                ..Default::default()
            }
        )),
        Some(b"\x1bx".to_vec())
    );
    let cases = [
        ("f1", "\u{1b}OP"),
        ("f2", "\u{1b}OQ"),
        ("f3", "\u{1b}OR"),
        ("f4", "\u{1b}OS"),
        ("f5", "\u{1b}[15~"),
        ("f6", "\u{1b}[17~"),
        ("f7", "\u{1b}[18~"),
        ("f8", "\u{1b}[19~"),
        ("f9", "\u{1b}[20~"),
        ("f10", "\u{1b}[21~"),
        ("f11", "\u{1b}[23~"),
        ("f12", "\u{1b}[24~"),
    ];
    for (key, expected) in cases {
        assert_eq!(
            terminal_input(&KeyInput::plain(key, None)),
            Some(expected.as_bytes().to_vec())
        );
    }
}

#[test]
fn covers_navigation_and_control_edge_sequences() {
    let cases = [
        ("backspace", "\u{7f}"),
        ("tab", "\t"),
        ("escape", "\u{1b}"),
        ("down", "\u{1b}[B"),
        ("right", "\u{1b}[C"),
        ("left", "\u{1b}[D"),
        ("home", "\u{1b}[H"),
        ("end", "\u{1b}[F"),
        ("pageup", "\u{1b}[5~"),
        ("pagedown", "\u{1b}[6~"),
        ("delete", "\u{1b}[3~"),
        ("insert", "\u{1b}[2~"),
    ];
    for (key, expected) in cases {
        assert_eq!(
            terminal_input(&KeyInput::plain(key, None)),
            Some(expected.as_bytes().to_vec())
        );
    }
    assert_eq!(
        terminal_input(&KeyInput::new(
            "tab",
            None,
            Modifiers {
                shift: true,
                ..Default::default()
            }
        )),
        Some(b"\x1b[Z".to_vec())
    );
    for (key, expected) in [
        ("space", 0),
        ("[", 27),
        ("\\", 28),
        ("]", 29),
        ("^", 30),
        ("_", 31),
    ] {
        assert_eq!(
            terminal_input(&KeyInput::new(
                key,
                None,
                Modifiers {
                    control: true,
                    ..Default::default()
                }
            )),
            Some(vec![expected])
        );
    }
    assert_eq!(terminal_input(&KeyInput::plain("unknown", None)), None);
}

#[test]
fn uses_jetbrains_nerd_font_with_cjk_and_emoji_fallbacks() {
    assert_eq!(TERMINAL_FONT_FAMILY, "JetBrainsMono Nerd Font Mono");
    assert_eq!(
        TERMINAL_FONT_FALLBACKS,
        [
            "Sarasa Mono SC",
            "PingFang SC",
            "Hiragino Sans GB",
            "Apple Color Emoji"
        ]
    );
}

#[test]
fn groups_terminal_cells_into_shaped_text_runs() {
    let mut grid = TerminalGrid::new(1, 17);
    grid.process(b"Context SYSTEM.md");
    let lines = grid.lines();
    let runs = lines[0].runs();

    assert_eq!(runs.len(), 1, "同一样式的字符应作为一个文本 run 排版");
    assert_eq!(runs[0].text, "Context SYSTEM.md");
    assert_eq!(runs[0].columns, 17);
}

#[test]
fn splits_runs_at_style_and_cursor_boundaries_without_adding_wide_character_spaces() {
    let mut grid = TerminalGrid::new(1, 8);
    grid.process("A中\x1b[31mBC".as_bytes());
    let lines = grid.lines();
    let runs = lines[0].runs();

    assert_eq!(runs.iter().map(|run| run.columns).sum::<u16>(), 8);
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        "A中BC   "
    );
    assert!(
        runs.len() >= 2,
        "颜色边界应拆分 run，光标由终端内核单独绘制"
    );
}

#[test]
fn shapes_wide_glyphs_at_individual_grid_columns_instead_of_accumulating_font_advances() {
    let mut grid = TerminalGrid::new(1, 16);
    grid.process("A放松放松东B".as_bytes());
    let runs = grid.lines()[0].runs();
    assert_eq!(runs[0].text, "A");
    assert_eq!(runs[0].columns, 1);
    for (index, glyph) in ["放", "松", "放", "松", "东"].iter().enumerate() {
        assert_eq!(runs[index + 1].text, *glyph);
        assert_eq!(runs[index + 1].columns, 2);
    }
    assert_eq!(runs[6].text, "B    ");
    assert_eq!(runs.iter().map(|run| run.columns).sum::<u16>(), 16);
    assert_eq!(grid.cursor(), (0, 12), "光标应位于终端网格中的第 12 列");
    assert_eq!(runs[..6].iter().map(|run| run.columns).sum::<u16>(), 11);
}

#[test]
fn preserves_complete_cjk_history_in_grid_runs() {
    let text = "加载历史执行信息，中文必须完整展示";
    let mut grid = TerminalGrid::new(2, 40);
    grid.process(text.as_bytes());
    let runs = grid.lines()[0].runs();

    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        format!("{text}{}", " ".repeat(40 - text.chars().count() * 2))
    );
    assert_eq!(runs.iter().map(|run| run.columns).sum::<u16>(), 40);
}

#[test]
fn scrolls_through_retained_history_and_returns_to_live_output() {
    let mut grid = TerminalGrid::new(3, 12);
    for line in 0..8 {
        grid.process(format!("line-{line}\r\n").as_bytes());
    }
    let live = grid.lines();

    grid.scroll_by(2);
    assert_eq!(grid.scrollback(), 2);
    assert_ne!(grid.lines(), live);
    assert!(
        grid.lines()
            .iter()
            .any(|line| line.text().contains("line-5"))
    );

    grid.scroll_by(-1);
    assert_eq!(grid.scrollback(), 1);
    grid.scroll_to_bottom();
    assert_eq!(grid.scrollback(), 0);
    assert_eq!(grid.lines(), live);
}

#[test]
fn scrollback_clamps_at_both_ends() {
    let mut grid = TerminalGrid::new(2, 8);
    for line in 0..5 {
        grid.process(format!("{line}\r\n").as_bytes());
    }

    grid.scroll_by(i32::MAX);
    let top = grid.scrollback();
    assert!(top > 0);
    grid.scroll_by(1);
    assert_eq!(grid.scrollback(), top);

    grid.scroll_by(i32::MIN);
    assert_eq!(grid.scrollback(), 0);
}

#[test]
fn extracts_forward_and_reverse_terminal_selections_for_clipboard() {
    let mut grid = TerminalGrid::new(3, 16);
    grid.process("第一行 alpha\r\n第二行 beta\r\n第三行 gamma".as_bytes());

    let forward = TerminalSelection::new((0, 0), (1, 5));
    assert_eq!(grid.selection_text(forward), "第一行 alpha\n第二行");
    assert_eq!(forward.columns_for_row(0, 16), Some(0..16));
    assert_eq!(forward.columns_for_row(1, 16), Some(0..6));
    assert_eq!(forward.columns_for_row(2, 16), None);

    let reverse = TerminalSelection::new((1, 5), (0, 0));
    assert_eq!(grid.selection_text(reverse), "第一行 alpha\n第二行");
}

#[test]
fn trims_unselected_terminal_padding_when_copying() {
    let mut grid = TerminalGrid::new(2, 12);
    grid.process(b"copy me\r\nnext");

    assert_eq!(
        grid.selection_text(TerminalSelection::new((0, 0), (1, 11))),
        "copy me\nnext"
    );
}

#[test]
fn encodes_terminal_mouse_wheel_for_sgr_mouse_tracking() {
    let mut grid = TerminalGrid::new(4, 20);
    assert_eq!(
        grid.mouse_wheel_sequence(1, 2, true, false, false, false),
        None
    );

    grid.process(b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        grid.mouse_wheel_sequence(1, 2, true, false, false, false),
        Some(b"\x1b[<64;3;2M".to_vec())
    );
    assert_eq!(
        grid.mouse_wheel_sequence(1, 2, false, true, true, true),
        Some(b"\x1b[<93;3;2M".to_vec())
    );
}

#[test]
fn wide_inverse_cells_share_the_same_background_without_color_gaps() {
    let mut grid = TerminalGrid::new(2, 12);
    grid.process("\x1b[7m中文\x1b[0m".as_bytes());
    let line = &grid.lines()[0];
    for column in [0, 1, 2, 3] {
        assert_eq!(line.cells()[column].background, FOREGROUND);
        assert_eq!(line.cells()[column].foreground, BACKGROUND);
    }
    assert_eq!(line.cells()[0].text, "中");
    assert!(line.cells()[1].text.is_empty());
    assert_eq!(line.cells()[2].text, "文");
    assert!(line.cells()[3].text.is_empty());
}

#[test]
fn reports_alternate_screen_for_tui_scroll_fallback() {
    let mut grid = TerminalGrid::new(4, 20);
    assert!(!grid.alternate_screen());
    grid.process(b"\x1b[?1049h");
    assert!(grid.alternate_screen());
}

#[test]
fn maps_ansi_palette_cube_grayscale_and_truecolor() {
    assert_eq!(indexed_color(0), ANSI_COLORS[0]);
    assert_eq!(indexed_color(15), ANSI_COLORS[15]);
    assert_eq!(indexed_color(16), 0x0000_0000);
    assert_eq!(indexed_color(21), 0x0000_00FF);
    assert_eq!(indexed_color(231), 0x00FF_FFFF);
    assert_eq!(indexed_color(232), 0x0008_0808);
    assert_eq!(indexed_color(255), 0x00EE_EEEE);

    let mut grid = TerminalGrid::new(1, 2);
    grid.process(b"\x1b[38;2;1;2;3;48;2;4;5;6mX");
    let lines = grid.lines();
    let cell = &lines[0].cells()[0];
    assert_eq!(cell.foreground, 0x0001_0203);
    assert_eq!(cell.background, 0x0004_0506);
}

fn kitty_rgba(image_id: u32, placement_id: u32, pixel: [u8; 4]) -> Vec<u8> {
    let encoded = match pixel {
        [255, 0, 0, 255] => "/wAA/w==",
        [0, 255, 0, 255] => "AP8A/w==",
        _ => panic!("测试只使用预编码像素"),
    };
    format!("\x1b_Ga=T,f=32,s=1,v=1,i={image_id},p={placement_id},c=1,r=1;{encoded}\x1b\\")
        .into_bytes()
}

#[test]
fn kitty_graphics_survive_arbitrary_pty_chunks_and_delete_cleanly() {
    let bytes = kitty_rgba(41, 7, [255, 0, 0, 255]);
    let mut grid = TerminalGrid::new(3, 12);
    for chunk in bytes.chunks(3) {
        grid.process(chunk);
    }

    let graphics = grid.graphics();
    assert_eq!(graphics.len(), 1);
    assert_eq!(graphics[0].image_id, 41);
    assert_eq!(graphics[0].placement_id, 7);
    assert_eq!((graphics[0].pixel_width, graphics[0].pixel_height), (1, 1));
    assert_eq!(graphics[0].image.as_bytes(0), Some(&[0, 0, 255, 255][..]));

    grid.process(b"\x1b_Ga=d,d=I,i=41\x1b\\");
    assert!(grid.graphics().is_empty());
    assert_eq!(grid.graphics_memory_bytes(), 0);
}

#[test]
fn kitty_graphics_keep_main_and_alternate_screen_state_separate() {
    let mut grid = TerminalGrid::new(3, 12);
    grid.process(&kitty_rgba(1, 1, [255, 0, 0, 255]));
    assert_eq!(grid.graphics()[0].image_id, 1);

    grid.process(b"\x1b[?1049h");
    assert!(grid.graphics().is_empty());
    grid.process(&kitty_rgba(2, 2, [0, 255, 0, 255]));
    assert_eq!(grid.graphics()[0].image_id, 2);

    grid.process(b"\x1b[?1049l");
    assert_eq!(grid.graphics()[0].image_id, 1);
}

#[test]
fn kitty_placement_tracks_scrollback_and_resize() {
    let mut grid = TerminalGrid::new(3, 12);
    grid.process(&kitty_rgba(3, 9, [255, 0, 0, 255]));
    let initial = grid.graphics()[0].clone();
    assert_eq!((initial.x, initial.y), (0.0, 0.0));

    for _ in 0..4 {
        grid.process(b"\r\n");
    }
    assert!(grid.graphics().is_empty());
    grid.scroll_by(4);
    assert!(!grid.graphics().is_empty());

    grid.resize(4, 20);
    let resized = grid.graphics()[0].clone();
    assert_eq!((resized.width, resized.height), (8.0, 16.0));
}

#[test]
fn graphics_budget_can_be_reduced_and_evicts_existing_images() {
    let mut grid = TerminalGrid::new(3, 12);
    grid.process(&kitty_rgba(4, 1, [255, 0, 0, 255]));
    assert!(!grid.graphics().is_empty());
    grid.set_graphics_memory_limit(1);
    assert!(grid.graphics().is_empty());
    assert_eq!(grid.graphics_memory_bytes(), 0);
}

#[test]
fn independent_terminal_grids_do_not_share_kitty_image_ids() {
    let mut first = TerminalGrid::new(2, 8);
    let mut second = TerminalGrid::new(2, 8);
    first.process(&kitty_rgba(77, 1, [255, 0, 0, 255]));
    second.process(&kitty_rgba(77, 1, [0, 255, 0, 255]));

    assert_eq!(
        first.graphics()[0].image.as_bytes(0),
        Some(&[0, 0, 255, 255][..])
    );
    assert_eq!(
        second.graphics()[0].image.as_bytes(0),
        Some(&[0, 255, 0, 255][..])
    );
}

/// 真实 Pi 录制样本体积较大，只保留在本地，不进入版本追踪；缺失时跳过。
fn real_pi_recording() -> Option<Vec<u8>> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pi-terminal.raw"
    ))
    .ok()
}

#[test]
fn real_pi_recording_decodes_kitty_image_and_final_placement() {
    let Some(bytes) = real_pi_recording() else {
        eprintln!("跳过：本地不存在 tests/fixtures/pi-terminal.raw 录制样本");
        return;
    };

    let mut grid = TerminalGrid::new(60, 160);
    grid.process(&bytes);

    let graphics = grid.graphics();
    assert!(
        !graphics.is_empty(),
        "录制样本回放后必须存在可见图片 placement"
    );
    let graphic = graphics
        .iter()
        .find(|graphic| graphic.image_id == 570_670_258)
        .expect("必须包含录制中的图片 570670258");
    assert_eq!((graphic.pixel_width, graphic.pixel_height), (1828, 1382));
    assert!(graphic.placement_id >= 0x8000_0000, "内部 placement id");
    assert_eq!(graphic.z_index, 0);
}

#[test]
fn real_pi_recording_survives_arbitrary_pty_chunk_boundaries() {
    let Some(bytes) = real_pi_recording() else {
        eprintln!("跳过：本地不存在 tests/fixtures/pi-terminal.raw 录制样本");
        return;
    };

    let mut whole = TerminalGrid::new(60, 160);
    whole.process(&bytes);

    let mut chunked = TerminalGrid::new(60, 160);
    for chunk in bytes.chunks(137) {
        chunked.process(chunk);
    }

    let whole_graphics = whole.graphics();
    let chunked_graphics = chunked.graphics();
    assert_eq!(whole_graphics.len(), chunked_graphics.len());
    for (left, right) in whole_graphics.iter().zip(chunked_graphics.iter()) {
        assert_eq!(left.image_id, right.image_id);
        assert_eq!(left.placement_id, right.placement_id);
        assert_eq!(left.pixel_width, right.pixel_width);
        assert_eq!(left.pixel_height, right.pixel_height);
        assert!(left.x.to_bits() == right.x.to_bits());
        assert!(left.y.to_bits() == right.y.to_bits());
        assert!(left.width.to_bits() == right.width.to_bits());
        assert!(left.height.to_bits() == right.height.to_bits());
        assert!(
            left.source_rect
                .iter()
                .zip(right.source_rect.iter())
                .all(|(l, r)| l.to_bits() == r.to_bits())
        );
        assert_eq!(left.image.as_bytes(0), right.image.as_bytes(0));
    }
}
