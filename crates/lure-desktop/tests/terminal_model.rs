use lure_desktop::terminal::{TerminalLine, VisibleRowCache};

use lure_desktop::input::{KeyInput, Modifiers, terminal_input};
use lure_desktop::terminal::{
    TERMINAL_FONT_FALLBACKS, TERMINAL_FONT_FAMILY, TerminalGeometry, TerminalGrid,
    TerminalSelection, terminal_size_for_viewport,
};
use lure_desktop::theme::{
    ACCENT, ANSI_COLORS, BACKGROUND, BORDER, ERROR, FOREGROUND, MUTED, TITLEBAR, indexed_color,
};

#[test]
fn defaults_to_dracula_dark_palette_for_window_and_terminal() {
    assert_eq!(BACKGROUND, 0x0028_2A36);
    assert_eq!(TITLEBAR, 0x0021_222C);
    assert_eq!(FOREGROUND, 0x00F8_F8F2);
    assert_eq!(MUTED, 0x00A6_ABCB);
    assert_eq!(BORDER, 0x0044_475A);
    assert_eq!(ERROR, 0x00FF_5555);
    assert_eq!(ACCENT, 0x00BD_93F9);
    assert_eq!(
        ANSI_COLORS,
        [
            0x0021_222C,
            0x00FF_5555,
            0x0050_FA7B,
            0x00F1_FA8C,
            0x00BD_93F9,
            0x00FF_79C6,
            0x008B_E9FD,
            0x00F8_F8F2,
            0x0062_72A4,
            0x00FF_6E6E,
            0x0069_FF94,
            0x00FF_FFA5,
            0x00D6_ACFF,
            0x00FF_92DF,
            0x00A4_FFFF,
            0x00FF_FFFF,
        ]
    );
    assert_eq!(indexed_color(1), 0x00FF_5555);
    assert_eq!(indexed_color(15), 0x00FF_FFFF);
}

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
fn color_queries_reply_with_the_active_terminal_palette() {
    use lure_desktop::theme::Theme;

    let mut grid = TerminalGrid::new(2, 12);
    grid.set_theme(Theme::Light);
    grid.process(b"\x1b]10;?\x07\x1b]11;?\x1b\\\x1b]4;1;?\x07");
    assert_eq!(
        grid.take_responses(),
        vec![
            "\x1b]10;rgb:0f0f/1717/2a2a\x07",
            "\x1b]11;rgb:f7f7/f8f8/fafa\x1b\\",
            "\x1b]4;1;rgb:b9b9/1c1c/1c1c\x07",
        ]
    );
    assert!(grid.take_responses().is_empty());

    grid.process(b"\x1b]10;?\x07\x1b]11;?\x07\x1b]12;?\x07\x1b]4;1;?\x07");
    grid.set_theme(Theme::Dark);
    assert_eq!(
        grid.take_responses(),
        vec![
            "\x1b]10;rgb:f8f8/f8f8/f2f2\x07",
            "\x1b]11;rgb:2828/2a2a/3636\x07",
            "\x1b]12;rgb:f8f8/f8f8/f2f2\x07",
            "\x1b]4;1;rgb:ffff/5555/5555\x07",
        ]
    );
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
    // 800 × 600：可用 782 × 534，向下取整为 97 × 33。
    assert_eq!(terminal_size_for_viewport(800.0, 600.0), (97, 33));
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
fn control_new_and_resume_shortcuts_submit_pi_commands() {
    for (key, command) in [("n", b"/new\r".as_slice()), ("r", b"/resume\r".as_slice())] {
        assert_eq!(
            terminal_input(&KeyInput::new(
                key,
                None,
                Modifiers {
                    control: true,
                    ..Default::default()
                }
            )),
            Some(command.to_vec())
        );
    }
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
fn kitty_placement_uses_same_absolute_row_origin_as_visible_text_after_history_eviction() {
    for (rows, columns) in [(3, 12), (35, 117), (80, 220)] {
        let mut grid = TerminalGrid::new(rows, columns);
        for _ in 0..usize::from(rows) + 5 {
            grid.process(b"old row\r\n");
        }
        // Pi 重绘时会清理保存的历史；Rio 的 Kitty dest_row 仍使用包含
        // lines_evicted 的绝对行号，可见字符却从当前历史顶部计数。
        grid.process(b"\x1b[3J\x1b[1;1H");
        grid.process(&kitty_rgba(98, 1, [255, 0, 0, 255]));
        assert!(
            grid.graphics()[0].y.abs() < f32::EPSILON,
            "{columns} × {rows} 网格的图片必须与光标所在首行对齐"
        );
        grid.process(b"\x1b[2;1Hmarker");
        assert!(grid.lines()[1].text().starts_with("marker"));
    }
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
        assert_eq!(left.x.to_bits(), right.x.to_bits());
        assert_eq!(left.y.to_bits(), right.y.to_bits());
        assert_eq!(left.width.to_bits(), right.width.to_bits());
        assert_eq!(left.height.to_bits(), right.height.to_bits());
        assert!(
            left.source_rect
                .iter()
                .zip(right.source_rect.iter())
                .all(|(l, r)| l.to_bits() == r.to_bits())
        );
        assert_eq!(left.image.as_bytes(0), right.image.as_bytes(0));
    }
}

#[test]
fn long_ascii_runs_compensate_font_advance_instead_of_accumulating_drift() {
    use lure_desktop::terminal::terminal_run_tracking;
    // 系统中 JetBrainsMono NFM 13px 的自然单字进宽是 7.8px，
    // 100 个字符直接批量塑形会比 8px 网格提前 20px。
    let tracking = terminal_run_tracking(780.0, 100, 100).expect("长 run 需要字距补偿");
    assert!(((780.0 + 99.0 * tracking) - 800.0).abs() < 0.01);
    assert!(terminal_run_tracking(7.8, 1, 1).is_none());
    assert!(terminal_run_tracking(0.0, 100, 100).is_none());
}

#[test]
fn non_ascii_cells_do_not_accumulate_fallback_font_advances() {
    let mut grid = TerminalGrid::new(1, 12);
    grid.process("aé你好b".as_bytes());
    let runs = grid.lines()[0].runs();
    assert!(runs.iter().any(|run| run.text == "é" && run.columns == 1));
    assert!(runs.iter().any(|run| run.text == "你" && run.columns == 2));
    assert!(runs.iter().any(|run| run.text == "好" && run.columns == 2));
    assert!(
        runs.iter()
            .all(|run| run.text.is_ascii() || run.text.chars().count() <= 1)
    );
}

#[test]
fn viewport_grid_never_exceeds_the_visible_content_area() {
    // 960 × 640 窗口：左右 8px 内边距 + 1px 边框，标题栏 48px。
    // 网格必须完全落在 942 × 574 的可视区域内，否则最右/最下会被裁掉。
    let (columns, rows) = terminal_size_for_viewport(960.0, 640.0);
    let (pixel_width, pixel_height) = lure_desktop::terminal::terminal_pixel_size(columns, rows);
    assert!(f32::from(pixel_width) <= 960.0 - 16.0 - 2.0);
    assert!(f32::from(pixel_height) <= 640.0 - 48.0 - 16.0 - 2.0);
    assert_eq!((columns, rows), (117, 35));
}

#[test]
fn pty_size_uses_the_same_cell_grid_as_rendering() {
    let (pixel_width, pixel_height) = lure_desktop::terminal::terminal_pixel_size(117, 35);
    assert_eq!((pixel_width, pixel_height), (936, 560));
}

#[test]
fn replaced_kitty_image_textures_are_released_for_gpu_cleanup() {
    let mut grid = TerminalGrid::new(3, 12);
    grid.process(&kitty_rgba(91, 1, [255, 0, 0, 255]));
    assert!(grid.take_released_images().is_empty());

    // 同一 image id 重传会替换纹理，旧纹理必须交回展示层释放。
    grid.process(&kitty_rgba(91, 2, [0, 255, 0, 255]));
    assert_eq!(grid.take_released_images().len(), 1);
    assert_eq!(grid.graphics_memory_bytes(), 4);

    // 删除图片同样回收纹理。
    grid.process(b"\x1b_Ga=d,d=I,i=91\x1b\\");
    assert_eq!(grid.take_released_images().len(), 1);
    assert_eq!(grid.graphics_memory_bytes(), 0);
}

#[test]
fn release_all_images_frees_a_closed_tab() {
    let mut grid = TerminalGrid::new(3, 12);
    grid.process(&kitty_rgba(92, 1, [255, 0, 0, 255]));
    assert_eq!(grid.release_all_images().len(), 1);
    assert_eq!(grid.graphics_memory_bytes(), 0);
    assert!(grid.graphics().is_empty());
}
#[test]
fn visible_row_cache_rebuilds_only_changed_text_or_style() {
    use std::rc::Rc;

    let mut grid = TerminalGrid::new(3, 12);
    grid.process("alpha\r\n界e\u{301}\r\nomega".as_bytes());
    let mut cache = VisibleRowCache::default();
    let mut builds = 0;
    let mut build = |line: &TerminalLine| {
        builds += 1;
        line.runs()
    };
    let first = grid.cached_visible_rows(&mut cache, &mut build);
    grid.process(b"\x1b[1;1H");
    let cursor_only = grid.cached_visible_rows(&mut cache, &mut build);
    assert!(
        first
            .iter()
            .zip(&cursor_only)
            .all(|(a, b)| Rc::ptr_eq(a, b))
    );
    grid.process(b"\x1b[7ma\x1b[0m");
    let edited = grid.cached_visible_rows(&mut cache, &mut build);
    assert!(!Rc::ptr_eq(&first[0], &edited[0]));
    assert!(Rc::ptr_eq(&first[1], &edited[1]));
    assert!(Rc::ptr_eq(&first[2], &edited[2]));
    grid.process("\x1b[2;3He\u{308}".as_bytes());
    let combined = grid.cached_visible_rows(&mut cache, &mut build);
    assert!(Rc::ptr_eq(&edited[0], &combined[0]));
    assert!(!Rc::ptr_eq(&edited[1], &combined[1]));
    assert_eq!(builds, 5);
    assert_eq!(
        combined
            .iter()
            .map(|row| row.as_ref().clone())
            .collect::<Vec<_>>(),
        grid.lines()
            .iter()
            .map(TerminalLine::runs)
            .collect::<Vec<_>>()
    );
}

#[test]
fn visible_row_cache_reuses_scrolled_rows_but_not_stale_rows() {
    use std::rc::Rc;

    let mut grid = TerminalGrid::new(3, 12);
    grid.process(b"one\r\ntwo\r\nthree\r\nfour");
    let mut cache = VisibleRowCache::default();
    let first = grid.cached_visible_rows(&mut cache, TerminalLine::text);
    grid.scroll_by(1);
    let scrolled = grid.cached_visible_rows(&mut cache, TerminalLine::text);
    assert!(Rc::ptr_eq(&first[0], &scrolled[1]));
    assert!(Rc::ptr_eq(&first[1], &scrolled[2]));
    grid.scroll_to_bottom();
    grid.process(b"\r\nfive");
    let streamed = grid.cached_visible_rows(&mut cache, TerminalLine::text);
    assert!(Rc::ptr_eq(&scrolled[2], &streamed[0]));
    grid.process(b"\x1b[?1049h\x1b[Hnew tab");
    assert_eq!(
        grid.cached_visible_rows(&mut cache, TerminalLine::text)[0].trim(),
        "new tab"
    );
    grid.process(b"\x1b[?1049l");
    grid.resize(4, 16);
    let resized = grid.cached_visible_rows(&mut cache, TerminalLine::text);
    assert_eq!(
        resized.iter().map(|row| row.as_str()).collect::<Vec<_>>(),
        grid.lines()
            .iter()
            .map(TerminalLine::text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn theme_switch_recolors_existing_terminal_without_changing_content_or_cursor() {
    use lure_desktop::theme::Theme;
    let mut grid = TerminalGrid::new(2, 12);
    grid.process(b"a\x1b[31mb\x1b[38;2;18;52;86mc\x1b[0m\x1b[7md");
    let original = grid.lines();
    let cursor = grid.cursor();
    let mut cache = VisibleRowCache::default();
    let dark = grid.cached_visible_rows(&mut cache, Clone::clone);
    grid.set_theme(Theme::Light);
    let light = grid.cached_visible_rows(&mut cache, Clone::clone);
    assert!(!std::rc::Rc::ptr_eq(&dark[0], &light[0]));
    let cells = light[0].cells();
    assert_eq!(cells[0].foreground, Theme::Light.palette().foreground);
    assert_eq!(cells[0].background, Theme::Light.palette().background);
    assert_eq!(cells[1].foreground, Theme::Light.indexed_color(1));
    assert_eq!(cells[2].foreground, 0x0012_3456);
    assert_eq!(cells[3].foreground, Theme::Light.palette().background);
    assert_eq!(cells[3].background, Theme::Light.palette().foreground);
    assert_eq!(light[0].text(), original[0].text());
    assert_eq!(grid.cursor(), cursor);
    grid.set_theme(Theme::Dark);
    assert_eq!(grid.lines(), original);
}

#[test]
fn paste_respects_bracketed_mode_without_submitting() {
    let mut grid = TerminalGrid::new(2, 20);
    assert_eq!(grid.paste_input("/tmp/image.png"), b"/tmp/image.png");
    grid.process(b"\x1b[?2004h");
    assert_eq!(
        grid.paste_input("/tmp/image.png"),
        b"\x1b[200~/tmp/image.png\x1b[201~"
    );
    assert_eq!(
        grid.paste_input("a\x1b[201~b"),
        b"\x1b[200~a[201~b\x1b[201~"
    );
    grid.process(b"\x1b[?2004l");
    assert_eq!(grid.paste_input("text"), b"text");
}

#[test]
fn mouse_drag_reports_motion_only_when_requested() {
    let mut grid = TerminalGrid::new(3, 20);
    assert_eq!(grid.mouse_drag_sequence(1, 2), None);
    grid.process(b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(grid.mouse_drag_sequence(1, 2), None);
    grid.process(b"\x1b[?1002h");
    assert_eq!(
        grid.mouse_drag_sequence(1, 2),
        Some(b"\x1b[<32;3;2M".to_vec())
    );
    grid.process(b"\x1b[?1002l");
    assert_eq!(grid.mouse_drag_sequence(1, 2), None);
    grid.process(b"\x1b[?1003h");
    assert_eq!(
        grid.mouse_drag_sequence(2, 5),
        Some(b"\x1b[<32;6;3M".to_vec())
    );
}

#[test]
fn mouse_drag_supports_legacy_encoding_and_its_coordinate_limit() {
    let mut grid = TerminalGrid::new(3, 300);
    grid.process(b"\x1b[?1002h");
    assert_eq!(grid.mouse_drag_sequence(1, 2), Some(b"\x1b[M@#\"".to_vec()));
    assert_eq!(grid.mouse_drag_sequence(1, 223), None);
}

// OSC 8 超链接：终端内核解析元数据，展示层只按单元格命中，不解析输出文字。
const OSC8_HOSTS: &str = "\x1b]8;;file:///etc/hosts\x1b\\hosts\x1b]8;;\x1b\\";
const OSC8_DOCS: &str = "\x1b]8;;https://example.dev/docs\x1b\\docs\x1b]8;;\x1b\\";

#[test]
fn osc8_hyperlink_spans_are_reported_per_visible_row() {
    let mut grid = TerminalGrid::new(3, 20);
    grid.process(format!("open {OSC8_HOSTS} and {OSC8_DOCS}").as_bytes());

    let links = grid.visible_links();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].row, 0);
    assert_eq!(links[0].column, 5);
    assert_eq!(links[0].columns, 5);
    assert_eq!(links[0].uri, "file:///etc/hosts");
    assert_eq!(links[1].row, 0);
    assert_eq!(links[1].column, 15);
    assert_eq!(links[1].columns, 4);
    assert_eq!(links[1].uri, "https://example.dev/docs");

    assert_eq!(grid.link_at((0, 5)).unwrap().uri, "file:///etc/hosts");
    assert_eq!(grid.link_at((0, 9)).unwrap().uri, "file:///etc/hosts");
    assert!(grid.link_at((0, 10)).is_none(), "链接外的空白不应命中");
    assert!(grid.link_at((0, 4)).is_none(), "链接前的文字不应命中");
    assert!(grid.link_at((1, 5)).is_none(), "其他行不应命中");
    assert!(grid.link_at((5, 5)).is_none(), "网格外的行不应 panic");
}

#[test]
fn osc8_hyperlinks_stay_linked_across_wide_characters() {
    let mut grid = TerminalGrid::new(2, 12);
    // 宽字符占位单元格同样属于链接区间，否则中文路径只能点中左半边。
    grid.process("\x1b]8;;file:///tmp/日志\x1b\\日志文件\x1b]8;;\x1b\\".as_bytes());

    let links = grid.visible_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].uri, "file:///tmp/日志");
    assert_eq!(links[0].columns, 8);
    for column in 0..8 {
        assert_eq!(grid.link_at((0, column)).unwrap().uri, "file:///tmp/日志");
    }
    assert!(grid.link_at((0, 8)).is_none());
}

#[test]
fn osc8_hyperlink_state_does_not_leak_after_close() {
    let mut grid = TerminalGrid::new(2, 16);
    grid.process(format!("{OSC8_DOCS} tail").as_bytes());

    let links = grid.visible_links();
    assert_eq!(links.len(), 1);
    assert!(grid.link_at((0, 3)).is_some(), "最后一个链接单元格仍应命中");
    assert!(
        grid.link_at((0, 4)).is_none(),
        "关闭序列之后的文字不继承链接"
    );
}

#[test]
fn hyperlinks_track_scrollback_and_alternate_screen() {
    let mut grid = TerminalGrid::new(2, 16);
    for _ in 0..4 {
        grid.process(b"plain\r\n");
    }
    grid.process(OSC8_HOSTS.as_bytes());

    // 链接位于实时输出区，尚未滚动时命中行号稳定。
    let links = grid.visible_links();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].row, 1);

    // 滚动历史后链接随所在行移动或移出视口，不应残留在原坐标。
    grid.scroll_by(3);
    for link in grid.visible_links() {
        assert_eq!(grid.link_at((link.row, link.column)).unwrap().uri, link.uri);
    }

    // alternate screen 持有独立的 extras 表，切换后不保留主屏链接。
    grid.process(b"\x1b[?1049h");
    assert!(grid.visible_links().is_empty());
    grid.process(b"\x1b[?1049l");
    grid.scroll_to_bottom();
    assert_eq!(grid.visible_links().len(), 1);
}

#[test]
fn terminal_geometry_matches_viewport_origin_and_rejects_outside_cells() {
    let geometry = TerminalGeometry::new(220.0, 20, 80);
    // 侧栏 220 + 终端内边距 8；标题栏 48 + 终端内边距 8。
    assert_eq!(geometry.cell_at(228.0, 56.0), Some((0, 0)));
    assert_eq!(geometry.cell_at(235.9, 71.9), Some((0, 0)));
    assert_eq!(geometry.cell_at(236.0, 72.0), Some((1, 1)));
    assert_eq!(geometry.cell_at(227.9, 56.0), None, "内边距内不属于网格");
    assert_eq!(geometry.cell_at(228.0, 55.9), None, "标题栏不属于网格");

    // 展开的侧栏区域、网格右侧与下方都返回 None，光标样式不会被误改。
    assert_eq!(geometry.cell_at(100.0, 200.0), None);
    assert_eq!(geometry.cell_at(228.0 + 8.0 * 80.0, 56.0), None);
    assert_eq!(geometry.cell_at(228.0, 56.0 + 16.0 * 20.0), None);
    assert_eq!(geometry.cell_at(f32::NAN, 56.0), None);

    // 拖选沿用夹取语义：越界位置收敛到最近边界单元格。
    assert_eq!(geometry.cell_at_or_clamped(0.0, 0.0), (0, 0));
    assert_eq!(geometry.cell_at_or_clamped(10_000.0, 10_000.0), (19, 79));
}

#[test]
fn terminal_geometry_collapsed_sidebar_uses_full_width() {
    let geometry = TerminalGeometry::new(0.0, 20, 80);
    assert_eq!(geometry.cell_at(8.0, 56.0), Some((0, 0)));
    assert_eq!(geometry.cell_at(100.0, 200.0), Some((9, 11)));
}
