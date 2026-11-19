use std::fs;
use std::path::Path;

#[test]
fn workspace_is_pure_rust_and_contains_no_tauri_or_web_frontend_entrypoints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!manifest.to_ascii_lowercase().contains("tauri"));
    assert!(manifest.contains("rio-vt"));
    assert!(
        !manifest.contains("alacritty_terminal"),
        "终端内核必须完成到 rio-vt 的迁移"
    );
    assert!(!manifest.contains("vt100"));
    assert!(!root.join("src-tauri").exists());
    assert!(!root.join("package.json").exists());
    assert!(!root.join("src/main.tsx").exists());
}

#[test]
fn terminal_text_uses_an_explicit_canvas_line_height_without_clipping() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    assert!(main.contains("px(CELL_HEIGHT),") && main.contains("TextAlign::Left"));
    assert!(
        !main.contains(
            ".overflow_hidden()\n                                            .child(run.text)"
        ),
        "终端文本 run 不得由固定行盒裁剪"
    );
}

#[test]
fn terminal_registers_gpui_text_input_and_uses_canvas_shaping() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    assert!(main.contains("impl EntityInputHandler for LureApp"));
    assert!(main.contains("ElementInputHandler::new"));
    assert!(main.contains("window.handle_input("));
    assert!(main.contains(".on_scroll_wheel("));
    assert!(main.contains("self.terminal.scroll_by("));
    assert!(main.contains(".on_mouse_move("));
    assert!(main.contains(".on_mouse_up("));
    assert!(main.contains("this.finish_selection(event.position, cx)"));
    assert!(main.contains("write_to_clipboard(ClipboardItem::new_string"));
    assert!(main.contains(".shape_line("));
    assert!(main.contains(".id(\"back-to-bottom\")"));
    assert!(main.contains("this.terminal.scroll_to_bottom()"));
    assert!(main.contains("self.terminal.mouse_button_sequence("));
    assert!(main.contains("self.terminal.cursor_visible()"));
    assert!(
        !main.contains(".child(run.text)"),
        "终端文本应像 Zed 一样在 canvas 中塑形并绘制，避免固定宽度 div 遮挡中文"
    );
}

#[test]
fn native_window_remains_manageable_by_the_operating_system() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    assert!(main.contains(".titlebar(Some(") && main.contains("TitlebarOptions::default()"));
    assert!(main.contains(".app_id(\"com.lure.desktop\".to_string())"));
    assert!(main.contains("window.titlebar_double_click()"));
    assert!(main.contains("window.minimize_window()"));
    assert!(main.contains("window.zoom_window()"));
    assert!(main.contains("window.remove_window()"));
    assert!(main.contains("#[cfg(not(target_os = \"macos\"))]"));
}

#[test]
fn macos_bundle_uses_the_lure_logo_and_native_application_identity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = fs::read_to_string(root.join("scripts/package-macos.sh")).unwrap();
    let plist = fs::read_to_string(root.join("packaging/macos/Info.plist")).unwrap();
    assert!(script.contains("assets/logo.svg"));
    assert!(root.join("assets/logo.svg").exists());
    assert!(script.contains("iconutil"));
    assert!(script.contains("lure-desktop"));
    assert!(plist.contains("com.lure.desktop"));
    assert!(plist.contains("Lure.icns"));
    assert!(plist.contains("NSHighResolutionCapable"));
}

#[test]
fn terminal_transition_panel_uses_the_current_dark_palette() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    let panel = main
        .split("    fn status_panel(")
        .nth(1)
        .unwrap()
        .split("impl EntityInputHandler")
        .next()
        .unwrap();
    assert!(panel.contains(".bg(rgb(TITLEBAR))"));
    assert!(panel.contains(".bg(gpui::rgba((TITLEBAR << 8) | 0x66))"));
    assert!(!panel.contains("0xF2_E5_BC"));
    assert!(!panel.contains("0x3c38_361a"));
}

#[test]
fn shared_sidebar_toggle_is_wired_to_titlebar_and_shortcut() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    assert!(main.contains("sidebar_visibility: SidebarVisibility::Expanded"));
    assert!(main.contains("id(\"toggle-sidebar-btn\")"));
    assert!(main.contains("this.sidebar_visibility.toggle()"));
    assert!(main.contains("event.keystroke.key.eq_ignore_ascii_case(\"b\")"));
    assert!(
        main.matches("self.sidebar_visibility == SidebarVisibility::Expanded")
            .count()
            >= 2
    );
    assert!(main.contains("route == Route::Terminal"));
    assert!(main.contains("include_bytes!(\"../../../assets/home/sidebar.svg\")"));
}

#[test]
fn home_icons_use_design_source_svg_not_font_glyphs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    let compact = main
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    for asset in [
        "brand", "search", "grid", "folder", "robot", "plus", "sidebar",
    ] {
        assert!(compact.contains(&format!(
            "include_bytes!(\"../../../assets/home/{asset}.svg\")"
        )));
        let svg = fs::read_to_string(root.join(format!("assets/home/{asset}.svg"))).unwrap();
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("viewBox=\"0 0 24 24\""));
    }
    for placeholder in [
        ".child(\"L\")",
        ".child(\"▣\")",
        ".child(\"▦\")",
        ".child(\"▱\")",
        ".child(\"⌕\")",
    ] {
        assert!(!main.contains(placeholder), "残留字符图标: {placeholder}");
    }
}

#[test]
fn project_cards_constrain_both_text_lines_inside_the_card() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    let card = main
        .split("    fn project_card(")
        .nth(1)
        .unwrap()
        .split("    fn project_draft_panel(")
        .next()
        .unwrap();
    assert!(card.contains(".flex_col()"));
    assert!(card.contains(".overflow_hidden()"));
    assert_eq!(card.matches(".text_ellipsis()").count(), 2);
    // 路径容器和文本均需限制最小宽度。
    assert_eq!(card.matches(".min_w_0()").count(), 3);
}

#[test]
fn closing_a_session_selects_the_next_saved_project() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    assert!(!main.contains("current_order"));
    assert!(!main.contains("session.order"));
    assert!(main.contains("self.projects.projects.iter().find_map("));
    let overview = fs::read_to_string(root.join("docs/overview.md")).unwrap();
    assert!(overview.contains("rio-vt"));
    assert!(overview.contains("projects.yaml"));
    assert!(!overview.contains("创建 `~/lure`"));
    let readme = fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains("rio-vt"));
    assert!(readme.contains("projects.yaml"));
    assert!(!readme.contains("`~/lure`"));
}

#[test]
fn project_home_and_sidebar_session_lifecycle_are_wired() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let main = fs::read_to_string(root.join("crates/lure-desktop/src/main.rs")).unwrap();
    let projects = fs::read_to_string(root.join("crates/lure-desktop/src/projects.rs")).unwrap();
    assert!(main.contains("route: Route::Home"));
    assert!(main.contains("project-card"));
    assert!(main.contains("new-project-home"));
    assert!(main.contains("fn project_sidebar("));
    assert_eq!(
        main.matches("body.child(self.project_sidebar(cx))").count(),
        2
    );
    assert!(main.contains("sidebar-close-session"));
    assert!(main.contains("cx.stop_propagation();\n                                            this.close_session(id, window, cx);"));
    assert!(main.contains("terminal-home"));
    assert!(!main.contains("route == Route::Home || route == Route::Terminal"));
    assert!(!main.contains("project-tab"));
    assert!(!main.contains("new-tab"));
    assert!(
        main.contains("terminal_viewport_width(viewport.width.into(), self.sidebar_visibility)")
    );
    assert!(main.contains("session: SessionId"));
    assert!(main.contains("parked_sessions"));
    assert!(main.contains("directory == project.path"));
    assert!(projects.contains(".join(\".pi\").join(\"projects.yaml\")"));
    assert!(projects.contains("fs::rename(&temporary, path)"));
}

#[test]
fn gpui_ce_is_pinned_to_the_requested_upstream_repository() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("https://github.com/gpui-ce/gpui-ce"));
    assert!(manifest.contains("rev ="));
}
