#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::{OsStr, OsString};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use async_channel::{Receiver, Sender};
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, FontStyle, FontWeight, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, PathPromptOptions, Render, Role, SharedString,
    TextAlign, TextRun, TitlebarOptions, UTF16Selection, UnderlineStyle, Window, WindowBounds,
    WindowOptions, canvas, div, point, prelude::*, px, rgb, rgb_to_hsla, rgba, size,
};
use lure_desktop::input::{KeyInput, Modifiers};
use lure_desktop::terminal::{
    CELL_HEIGHT, CELL_WIDTH, TERMINAL_PADDING, TITLEBAR_HEIGHT, TerminalGrid, TerminalSelection,
    terminal_font, terminal_size_for_viewport,
};
use lure_desktop::theme::{ACCENT, BACKGROUND, BORDER, ERROR, FOREGROUND, MUTED, TITLEBAR};
use lure_pty::{
    PtyCommand, PtyEvent, PtyProcess, SpawnEnvResolver, TerminalSize, resolve_pi_executable,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Starting,
    Running,
    Exited,
    Error,
}

enum AppMessage {
    Started {
        generation: u64,
        process: PtyProcess,
        directory: PathBuf,
    },
    StartFailed {
        generation: u64,
        message: String,
    },
    Output {
        generation: u64,
        data: Vec<u8>,
    },
    Exited {
        generation: u64,
        code: u32,
    },
    ReadFailed {
        generation: u64,
        message: String,
    },
}

struct LureApp {
    focus: FocusHandle,
    directory: PathBuf,
    phase: Phase,
    error: String,
    exit_code: Option<u32>,
    terminal: TerminalGrid,
    process: Option<PtyProcess>,
    dimensions: (u16, u16),
    generation: Arc<AtomicU64>,
    messages: Sender<AppMessage>,
    env_resolver: Arc<SpawnEnvResolver>,
    marked_text: String,
    scroll_rows_accumulator: f32,
    selection: Option<TerminalSelection>,
    selection_dragged: bool,
}

impl LureApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let directory = default_workspace().unwrap_or_else(|_| PathBuf::from("."));
        let dimensions = (80, 24);
        let (messages, receiver) = async_channel::unbounded();
        let app = Self {
            focus,
            directory,
            phase: Phase::Starting,
            error: String::new(),
            exit_code: None,
            terminal: TerminalGrid::new(dimensions.1, dimensions.0),
            process: None,
            dimensions,
            generation: Arc::new(AtomicU64::new(0)),
            messages,
            env_resolver: Arc::new(SpawnEnvResolver::new()),
            marked_text: String::new(),
            scroll_rows_accumulator: 0.0,
            selection: None,
            selection_dragged: false,
        };
        Self::receive_messages(receiver, cx);
        app
    }

    fn receive_messages(receiver: Receiver<AppMessage>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while let Ok(message) = receiver.recv().await {
                if this
                    .update(cx, |this, cx| this.handle_message(message, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_message(&mut self, message: AppMessage, cx: &mut Context<Self>) {
        match message {
            AppMessage::Started {
                generation,
                mut process,
                directory,
            } if self.is_current(generation) => {
                if let Err(error) =
                    process.resize(TerminalSize::new(self.dimensions.0, self.dimensions.1))
                {
                    self.phase = Phase::Error;
                    self.error = error.to_string();
                    return;
                }
                self.process = Some(process);
                self.directory = directory;
                self.phase = Phase::Running;
                self.error.clear();
                self.exit_code = None;
                cx.notify();
            }
            AppMessage::StartFailed {
                generation,
                message,
            } if self.is_current(generation) => {
                self.phase = Phase::Error;
                self.error = message;
                self.process = None;
                cx.notify();
            }
            AppMessage::Output { generation, data } if self.is_current(generation) => {
                self.selection = None;
                self.terminal.process(&data);
                for response in self.terminal.take_responses() {
                    if let Some(process) = self.process.as_mut()
                        && let Err(error) = process.write_all(response.as_bytes())
                    {
                        self.phase = Phase::Error;
                        self.error = error.to_string();
                        break;
                    }
                }
                cx.notify();
            }
            AppMessage::Exited { generation, code } if self.is_current(generation) => {
                self.process = None;
                self.phase = Phase::Exited;
                self.exit_code = Some(code);
                cx.notify();
            }
            AppMessage::ReadFailed {
                generation,
                message,
            } if self.is_current(generation) => {
                self.phase = Phase::Error;
                self.error = message;
                cx.notify();
            }
            _ => {}
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.stop_process();
        self.phase = Phase::Starting;
        self.error.clear();
        self.exit_code = None;
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let directory = self.directory.clone();
        let dimensions = self.dimensions;
        let messages = self.messages.clone();
        let resolver = self.env_resolver.clone();
        let start_failure_messages = messages.clone();
        let start_thread = thread::Builder::new()
            .name(format!("lure-start-{generation}"))
            .spawn(move || {
                let result =
                    (|| -> Result<(PtyProcess, std::sync::mpsc::Receiver<PtyEvent>), String> {
                        let runtime = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .map_err(|error| format!("无法创建环境运行时：{error}"))?;
                        let mut environment = runtime.block_on(resolver.resolve());
                        set_environment(&mut environment, "TERM", "xterm-256color");
                        set_environment(&mut environment, "COLORTERM", "truecolor");
                        set_environment(&mut environment, "TERM_PROGRAM", "Lure");
                        let mut command = PtyCommand::new(resolve_pi_executable(), &directory);
                        command.environment = Some(environment);
                        PtyProcess::spawn(command, TerminalSize::new(dimensions.0, dimensions.1))
                            .map_err(|error| error.to_string())
                    })();
                match result {
                    Ok((process, events)) => {
                        let _ = messages.send_blocking(AppMessage::Started {
                            generation,
                            process,
                            directory,
                        });
                        forward_pty_events(generation, &events, &messages);
                    }
                    Err(message) => {
                        let _ = messages.send_blocking(AppMessage::StartFailed {
                            generation,
                            message,
                        });
                    }
                }
            });
        if let Err(error) = start_thread {
            let _ = start_failure_messages.send_blocking(AppMessage::StartFailed {
                generation,
                message: format!("无法启动 PTY 后台线程：{error}"),
            });
        }
        cx.notify();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        self.start(cx);
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.stop_process();
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.phase = Phase::Exited;
        self.exit_code = None;
        cx.notify();
    }

    fn stop_process(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.stop();
        }
    }

    fn choose_directory(cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from("选择 Pi 工作目录")),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.directory = path;
                    this.start(cx);
                });
            }
        })
        .detach();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let modifiers = Modifiers {
            control: event.keystroke.modifiers.control,
            alt: event.keystroke.modifiers.alt,
            shift: event.keystroke.modifiers.shift,
            platform: event.keystroke.modifiers.platform,
        };
        if modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("v") {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.write(text.as_bytes(), cx);
            }
            return;
        }
        let input = KeyInput {
            key: event.keystroke.key.clone(),
            key_char: event.keystroke.key_char.clone(),
            modifiers,
        };
        if let Some(bytes) = self.terminal.keyboard_input(&input) {
            self.write(&bytes, cx);
        }
    }

    fn write(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.terminal.scroll_to_bottom();
        self.scroll_rows_accumulator = 0.0;
        self.selection = None;
        cx.notify();
        let Some(process) = self.process.as_mut() else {
            return;
        };
        if let Err(error) = process.write_all(bytes) {
            self.phase = Phase::Error;
            self.error = error.to_string();
            cx.notify();
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn terminal_cell_at(&self, position: gpui::Point<gpui::Pixels>) -> (u16, u16) {
        let x: f32 = position.x.into();
        let y: f32 = position.y.into();
        let inset = TERMINAL_PADDING / 2.0;
        let column = ((x - inset).max(0.0) / CELL_WIDTH).floor() as u16;
        let row = ((y - TITLEBAR_HEIGHT - inset).max(0.0) / CELL_HEIGHT).floor() as u16;
        (
            row.min(self.dimensions.1.saturating_sub(1)),
            column.min(self.dimensions.0.saturating_sub(1)),
        )
    }

    fn start_selection(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        let point = self.terminal_cell_at(position);
        if let Some(sequence) = self.terminal.mouse_button_sequence(point.0, point.1, false) {
            self.selection = None;
            self.write_repeated(&sequence, 1, cx);
            return;
        }
        self.selection = Some(TerminalSelection::new(point, point));
        self.selection_dragged = false;
        cx.notify();
    }

    fn update_selection(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        let point = self.terminal_cell_at(position);
        self.selection_dragged |= point != selection.anchor();
        self.selection = Some(selection.with_head(point));
        cx.notify();
    }

    fn finish_selection(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let point = self.terminal_cell_at(position);
        if let Some(sequence) = self.terminal.mouse_button_sequence(point.0, point.1, true) {
            self.write_repeated(&sequence, 1, cx);
            return;
        }
        self.update_selection(position, cx);
        if !self.selection_dragged {
            self.selection = None;
            cx.notify();
            return;
        }
        if let Some(selection) = self.selection {
            let text = self.terminal.selection_text(selection);
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn handle_scroll(&mut self, event: &gpui::ScrollWheelEvent, cx: &mut Context<Self>) {
        let rows = match event.delta {
            gpui::ScrollDelta::Lines(delta) => delta.y,
            gpui::ScrollDelta::Pixels(delta) => {
                let pixels: f32 = delta.y.into();
                pixels / CELL_HEIGHT
            }
        };
        if rows == 0.0 {
            return;
        }
        if self.scroll_rows_accumulator.signum() != rows.signum() {
            self.scroll_rows_accumulator = 0.0;
        }
        self.scroll_rows_accumulator = (self.scroll_rows_accumulator + rows).clamp(-100.0, 100.0);
        let whole_rows = self.scroll_rows_accumulator.trunc() as i32;
        if whole_rows == 0 {
            return;
        }
        self.scroll_rows_accumulator -= whole_rows as f32;
        self.selection = None;

        let up = whole_rows > 0;
        let count = whole_rows.unsigned_abs().min(100);
        let (row, column) = self.terminal_cell_at(event.position);
        let sequence = self.terminal.mouse_wheel_sequence(
            row,
            column,
            up,
            event.modifiers.shift,
            event.modifiers.alt,
            event.modifiers.control,
        );
        if let Some(sequence) = sequence {
            self.write_repeated(&sequence, count, cx);
        } else if self.terminal.alternate_screen() {
            self.write_repeated(if up { b"\x1b[A" } else { b"\x1b[B" }, count, cx);
        } else {
            self.terminal.scroll_by(whole_rows);
            cx.notify();
        }
    }

    fn write_repeated(&mut self, bytes: &[u8], count: u32, cx: &mut Context<Self>) {
        let Some(process) = self.process.as_mut() else {
            return;
        };
        for _ in 0..count {
            if let Err(error) = process.write_all(bytes) {
                self.phase = Phase::Error;
                self.error = error.to_string();
                cx.notify();
                break;
            }
        }
    }

    fn sync_size(&mut self, window: &Window) {
        let viewport = window.viewport_size();
        let next = terminal_size_for_viewport(viewport.width.into(), viewport.height.into());
        if next == self.dimensions {
            return;
        }
        self.dimensions = next;
        self.terminal.resize(next.1, next.0);
        if let Some(process) = self.process.as_mut() {
            let _ = process.resize(TerminalSize::new(next.0, next.1));
        }
    }

    fn title_button(
        id: &'static str,
        label: &'static str,
        symbol: &'static str,
        enabled: bool,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .aria_disabled(!enabled)
            .w(px(36.0))
            .h(px(36.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.0))
            .text_size(px(16.0))
            .text_color(rgb(if enabled { FOREGROUND } else { MUTED }))
            .cursor_pointer()
            .when(enabled, |element| {
                element
                    .hover_style(|style| style.bg(rgb(BORDER)))
                    .on_click(handler)
            })
            .child(symbol)
    }

    fn status_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, detail, action) = match self.phase {
            Phase::Starting => (
                "正在启动 Pi…".to_string(),
                "正在创建 PTY 并加载当前工作目录。".to_string(),
                None,
            ),
            Phase::Exited => (
                self.exit_code.map_or_else(
                    || "Pi 已停止".to_string(),
                    |code| format!("Pi 已退出（代码 {code}）"),
                ),
                "终端内容已保留，可以在相同目录重新启动。".to_string(),
                Some("重新启动"),
            ),
            Phase::Error => (
                "终端发生错误".to_string(),
                self.error.clone(),
                Some("重试启动"),
            ),
            Phase::Running => (String::new(), String::new(), None),
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x3c38_361a))
            .child(
                div()
                    .w(px(420.0))
                    .p(px(20.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(0xF2_E5_BC))
                    .text_color(rgb(FOREGROUND))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.0))
                            .text_color(rgb(if self.phase == Phase::Error {
                                ERROR
                            } else {
                                MUTED
                            }))
                            .child(detail),
                    )
                    .when_some(action, |panel, label| {
                        panel.child(
                            div()
                                .id("status-restart")
                                .role(Role::Button)
                                .aria_label(label)
                                .mt(px(16.0))
                                .px(px(12.0))
                                .py(px(8.0))
                                .rounded(px(8.0))
                                .bg(rgb(FOREGROUND))
                                .text_color(rgb(BACKGROUND))
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| this.restart(cx)))
                                .child(label),
                        )
                    }),
            )
    }
}

impl EntityInputHandler for LureApp {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        (!self.marked_text.is_empty()).then(|| 0..self.marked_text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.marked_text.is_empty() {
            self.marked_text.clear();
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked_text.clear();
        if !text.is_empty() {
            self.write(text.as_bytes(), cx);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked_text.clear();
        self.marked_text.push_str(new_text);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<gpui::Pixels>> {
        let cursor = self.terminal.cursor();
        let composition_offset = u16::try_from(range_utf16.start).unwrap_or(u16::MAX);
        Some(Bounds::new(
            point(
                element_bounds.left()
                    + px(CELL_WIDTH * (f32::from(cursor.1) + f32::from(composition_offset))),
                element_bounds.top() + px(CELL_HEIGHT * f32::from(cursor.0)),
            ),
            size(px(CELL_WIDTH), px(CELL_HEIGHT)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Drop for LureApp {
    fn drop(&mut self) {
        self.stop_process();
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl Render for LureApp {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_size(window);
        let cursor = self.terminal.cursor();
        let running = self.phase == Phase::Running;
        let cursor_visible = running && self.terminal.cursor_visible();
        let scrollback = self.terminal.scrollback();
        let terminal_rows = self
            .terminal
            .lines()
            .into_iter()
            .map(|line| line.runs())
            .collect::<Vec<_>>();
        let marked_text = self.marked_text.clone();
        let selection = self.selection;
        let dimensions = self.dimensions;
        let app = cx.entity();
        let input_focus = self.focus.clone();
        let directory = self.directory.display().to_string();
        let starting = self.phase == Phase::Starting;

        div()
            .id("lure-application")
            .role(Role::Application)
            .aria_label("Lure Pi 桌面终端")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event, _, cx| this.handle_key(event, cx)))
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .rounded(px(16.0))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(BACKGROUND))
            .text_color(rgb(FOREGROUND))
            .child(
                div()
                    .h(px(48.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(TITLEBAR))
                    .child(window_controls())
                    .child(
                        div()
                            .ml(px(12.0))
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .on_mouse_down(
                                MouseButton::Left,
                                |event: &MouseDownEvent, window, _| {
                                    if event.click_count == 2 {
                                        window.titlebar_double_click();
                                    } else if event.click_count == 1 {
                                        window.start_window_move();
                                    }
                                },
                            )
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Lure"))
                            .child(div().text_color(rgb(BORDER)).child("/"))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(rgb(MUTED))
                                    .child(directory),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(Self::title_button(
                                "choose-directory",
                                "选择工作目录",
                                "⌂",
                                !starting,
                                cx.listener(|_this, _, _, cx| Self::choose_directory(cx)),
                            ))
                            .child(Self::title_button(
                                "restart",
                                "重新启动 Pi",
                                "↻",
                                !starting,
                                cx.listener(|this, _, _, cx| this.restart(cx)),
                            ))
                            .child(Self::title_button(
                                "stop",
                                "停止 Pi",
                                "■",
                                running,
                                cx.listener(|this, _, _, cx| this.stop(cx)),
                            )),
                    ),
            )
            .child(
                div()
                    .id("pi-terminal")
                    .role(Role::Terminal)
                    .aria_label("Pi 终端工作区")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .p(px(8.0))
                    .bg(rgb(BACKGROUND))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.start_selection(event.position, window, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        if event.dragging() {
                            this.update_selection(event.position, cx);
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.finish_selection(event.position, cx);
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                        this.handle_scroll(event, cx);
                        cx.stop_propagation();
                    }))
                    .child(
                        canvas(
                            move |_bounds, window, _cx| {
                                let mut painted_runs = Vec::new();
                                for (row, runs) in terminal_rows.into_iter().enumerate() {
                                    let mut column = 0_u16;
                                    for run in runs {
                                        let shaped = if run.text.is_empty() {
                                            None
                                        } else {
                                            let mut font = terminal_font();
                                            if run.bold {
                                                font.weight = FontWeight::BOLD;
                                            }
                                            if run.italic {
                                                font.style = FontStyle::Italic;
                                            }
                                            let color = rgb_to_hsla(rgb(run.foreground));
                                            let text_run = TextRun {
                                                len: run.text.len(),
                                                font,
                                                color,
                                                background_color: None,
                                                underline: run.underline.then_some(
                                                    UnderlineStyle {
                                                        thickness: px(1.0),
                                                        color: Some(color),
                                                        wavy: false,
                                                    },
                                                ),
                                                strikethrough: None,
                                                letter_spacing: None,
                                            };
                                            Some(window.text_system().shape_line(
                                                run.text.into(),
                                                px(13.0),
                                                &[text_run],
                                            ))
                                        };
                                        painted_runs.push((
                                            u16::try_from(row).unwrap_or(u16::MAX),
                                            column,
                                            run.columns,
                                            run.background,
                                            shaped,
                                        ));
                                        column += run.columns;
                                    }
                                }
                                let marked = if marked_text.is_empty() {
                                    None
                                } else {
                                    let color = rgb_to_hsla(rgb(FOREGROUND));
                                    let text_run = TextRun {
                                        len: marked_text.len(),
                                        font: terminal_font(),
                                        color,
                                        background_color: None,
                                        underline: Some(UnderlineStyle {
                                            thickness: px(1.0),
                                            color: Some(color),
                                            wavy: false,
                                        }),
                                        strikethrough: None,
                                        letter_spacing: None,
                                    };
                                    Some(window.text_system().shape_line(
                                        marked_text.into(),
                                        px(13.0),
                                        &[text_run],
                                    ))
                                };
                                (painted_runs, marked)
                            },
                            move |bounds, (painted_runs, marked), window, cx| {
                                window.handle_input(
                                    &input_focus,
                                    ElementInputHandler::new(bounds, app),
                                    cx,
                                );

                                for (row, column, columns, background, _) in &painted_runs {
                                    window.paint_quad(
                                        PaintQuad::new(Bounds::new(
                                            point(
                                                bounds.left() + px(CELL_WIDTH * f32::from(*column)),
                                                bounds.top() + px(CELL_HEIGHT * f32::from(*row)),
                                            ),
                                            size(
                                                px(CELL_WIDTH * f32::from(*columns)),
                                                px(CELL_HEIGHT),
                                            ),
                                        ))
                                        .background(rgb(*background)),
                                    );
                                }

                                if let Some(selection) = selection {
                                    for row in 0..dimensions.1 {
                                        if let Some(columns) =
                                            selection.columns_for_row(row, dimensions.0)
                                        {
                                            window.paint_quad(
                                                PaintQuad::new(Bounds::new(
                                                    point(
                                                        bounds.left()
                                                            + px(CELL_WIDTH
                                                                * f32::from(columns.start)),
                                                        bounds.top()
                                                            + px(CELL_HEIGHT * f32::from(row)),
                                                    ),
                                                    size(
                                                        px(CELL_WIDTH
                                                            * f32::from(
                                                                columns.end - columns.start,
                                                            )),
                                                        px(CELL_HEIGHT),
                                                    ),
                                                ))
                                                .background(rgba(0x83a5_9880)),
                                            );
                                        }
                                    }
                                }

                                for (row, column, _, _, shaped) in &painted_runs {
                                    if let Some(shaped) = shaped {
                                        let _ = shaped.paint(
                                            point(
                                                bounds.left() + px(CELL_WIDTH * f32::from(*column)),
                                                bounds.top() + px(CELL_HEIGHT * f32::from(*row)),
                                            ),
                                            px(CELL_HEIGHT),
                                            TextAlign::Left,
                                            None,
                                            window,
                                            cx,
                                        );
                                    }
                                }

                                if cursor_visible {
                                    window.paint_quad(
                                        PaintQuad::new(Bounds::new(
                                            point(
                                                bounds.left()
                                                    + px(CELL_WIDTH * f32::from(cursor.1)),
                                                bounds.top()
                                                    + px(CELL_HEIGHT * f32::from(cursor.0)),
                                            ),
                                            size(px(CELL_WIDTH), px(CELL_HEIGHT)),
                                        ))
                                        .background(rgba(0x4585_8840))
                                        .border_widths(px(1.0))
                                        .border_color(rgb(ACCENT)),
                                    );
                                }

                                if let Some(marked) = marked {
                                    let _ = marked.paint(
                                        point(
                                            bounds.left() + px(CELL_WIDTH * f32::from(cursor.1)),
                                            bounds.top() + px(CELL_HEIGHT * f32::from(cursor.0)),
                                        ),
                                        px(CELL_HEIGHT),
                                        TextAlign::Left,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                            },
                        )
                        .size_full(),
                    )
                    .when(scrollback > 0, |workspace| {
                        workspace.child(
                            div()
                                .id("back-to-bottom")
                                .role(Role::Button)
                                .aria_label("Back to bottom")
                                .absolute()
                                .top(px(12.0))
                                .right(px(16.0))
                                .px(px(12.0))
                                .py(px(8.0))
                                .rounded(px(8.0))
                                .bg(rgb(TITLEBAR))
                                .text_color(rgb(FOREGROUND))
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.terminal.scroll_to_bottom();
                                    cx.notify();
                                }))
                                .child("↓ Back to bottom"),
                        )
                    })
                    .when(self.phase != Phase::Running, |workspace| {
                        workspace.child(self.status_panel(cx))
                    }),
            )
    }
}

// macOS 的交通灯由 AppKit 管理，系统窗口菜单与平铺工具才能识别它们。
#[cfg(target_os = "macos")]
fn window_controls() -> impl IntoElement {
    div().w(px(72.0)).flex_none()
}

#[cfg(not(target_os = "macos"))]
fn window_controls() -> impl IntoElement {
    div()
        .flex()
        .child(window_dot(
            "close-window",
            "关闭窗口",
            0xFF_5F_57,
            |_, window, _| {
                window.remove_window();
            },
        ))
        .child(window_dot(
            "minimize-window",
            "最小化窗口",
            0xFE_BC_2E,
            |_, window, _| {
                window.minimize_window();
            },
        ))
        .child(window_dot(
            "zoom-window",
            "最大化或还原窗口",
            0x28_C8_40,
            |_, window, _| {
                window.zoom_window();
            },
        ))
}

#[cfg(not(target_os = "macos"))]
fn window_dot(
    id: &'static str,
    label: &'static str,
    color: u32,
    handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .w(px(24.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(handler)
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded_full()
                .bg(rgb(color))
                .border_1()
                .border_color(gpui::rgba(0x0000_0033)),
        )
}

fn forward_pty_events(
    generation: u64,
    events: &std::sync::mpsc::Receiver<PtyEvent>,
    messages: &Sender<AppMessage>,
) {
    while let Ok(event) = events.recv() {
        let message = match event {
            PtyEvent::Output(data) => AppMessage::Output { generation, data },
            PtyEvent::Exited(status) => AppMessage::Exited {
                generation,
                code: status.exit_code,
            },
            PtyEvent::ReadFailed(message) => AppMessage::ReadFailed {
                generation,
                message,
            },
        };
        if messages.send_blocking(message).is_err() {
            break;
        }
    }
}

fn set_environment(environment: &mut Vec<(OsString, OsString)>, key: &str, value: &str) {
    if let Some((_, current)) = environment
        .iter_mut()
        .find(|(name, _)| name == OsStr::new(key))
    {
        *current = OsString::from(value);
    } else {
        environment.push((OsString::from(key), OsString::from(value)));
    }
}

fn default_workspace() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "无法确定用户家目录".to_string())?;
    let workspace = PathBuf::from(home).join("lure");
    std::fs::create_dir_all(&workspace)
        .map_err(|error| format!("无法创建默认工作目录 {}：{error}", workspace.display()))?;
    canonical_directory(&workspace)
}

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("工作目录无效 {}：{error}", path.display()))?;
    if !path.is_dir() {
        return Err(format!("工作目录不是文件夹：{}", path.display()));
    }
    Ok(path)
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.set_app_identity("com.lure.desktop", "Lure");
        let bounds = Bounds::centered(None, size(px(960.0), px(640.0)), cx);
        let window = cx
            .open_window(
                WindowOptions::new()
                    .window_bounds(Some(WindowBounds::Windowed(bounds)))
                    .window_min_size(Some(size(px(720.0), px(480.0))))
                    .titlebar(Some(
                        TitlebarOptions::default()
                            .title(SharedString::from("Lure"))
                            .appears_transparent(true)
                            .traffic_light_position(point(px(12.0), px(14.0))),
                    ))
                    .app_id("com.lure.desktop".to_string())
                    .app_owns_titlebar_drag(true),
                |window, cx| cx.new(|cx| LureApp::new(window, cx)),
            )
            .expect("无法创建 Lure 窗口");
        let _ = window.update(cx, |app, _window, cx| app.start(cx));
        cx.activate(true);
    });
}
