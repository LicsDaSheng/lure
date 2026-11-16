use lure_desktop::input::{KeyInput, Modifiers, terminal_input};
use lure_desktop::terminal::{TerminalGrid, terminal_size_for_viewport};
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
    grid.process(b"main\x1b[?1049hfullscreen\x1b[2;3H!");

    let lines = grid.lines();
    assert!(lines[0].text().starts_with("fullscre"));
    assert_eq!(grid.cursor(), (1, 3));

    grid.process(b"\x1b[?1049l");
    assert!(grid.lines()[0].text().starts_with("main"));
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
