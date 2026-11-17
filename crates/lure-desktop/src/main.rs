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
use lure_desktop::projects::{
    ProjectEntry, ProjectList, backup_invalid_projects, load_projects, projects_file_from_home,
    save_projects,
};
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

type SessionId = u64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Home,
    Terminal,
}

enum AppMessage {
    Started {
        session: SessionId,
        generation: u64,
        process: PtyProcess,
        directory: PathBuf,
    },
    StartFailed {
        session: SessionId,
        generation: u64,
        message: String,
    },
    Output {
        session: SessionId,
        generation: u64,
        data: Vec<u8>,
    },
    Exited {
        session: SessionId,
        generation: u64,
        code: u32,
    },
    ReadFailed {
        session: SessionId,
        generation: u64,
        message: String,
    },
}

struct SessionState {
    id: SessionId,
    order: u64,
    name: String,
    directory: PathBuf,
    phase: Phase,
    error: String,
    exit_code: Option<u32>,
    terminal: TerminalGrid,
    process: Option<PtyProcess>,
    dimensions: (u16, u16),
    generation: Arc<AtomicU64>,
    marked_text: String,
    scroll_rows_accumulator: f32,
    selection: Option<TerminalSelection>,
    selection_dragged: bool,
}

struct ProjectDraft {
    path: PathBuf,
    name: String,
    marked_text: String,
    open_after_create: bool,
}

struct LureApp {
    focus: FocusHandle,
    route: Route,
    current_id: Option<SessionId>,
    current_order: u64,
    current_name: String,
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
    parked_sessions: Vec<SessionState>,
    next_session_id: SessionId,
    projects: ProjectList,
    projects_path: PathBuf,
    projects_error: Option<String>,
    project_draft: Option<ProjectDraft>,
    project_picker_open: bool,
}

impl LureApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let dimensions = (80, 24);
        let (messages, receiver) = async_channel::unbounded();
        let projects_path = std::env::var_os("HOME").map_or_else(
            || PathBuf::from(".pi/projects.yaml"),
            |home| projects_file_from_home(Path::new(&home)),
        );
        let pi_directory_exists = projects_path.parent().is_some_and(Path::is_dir);
        let (projects, projects_error) = match load_projects(&projects_path) {
            Ok(projects) if pi_directory_exists => (projects, None),
            Ok(projects) => (
                projects,
                Some("未检测到 Pi 配置目录，请先完成 Pi 安装与初始化".to_string()),
            ),
            Err(error) => (ProjectList::default(), Some(error)),
        };
        let app = Self {
            focus,
            route: Route::Home,
            current_id: None,
            current_order: 0,
            current_name: String::new(),
            directory: PathBuf::new(),
            phase: Phase::Exited,
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
            parked_sessions: Vec::new(),
            next_session_id: 1,
            projects,
            projects_path,
            projects_error,
            project_draft: None,
            project_picker_open: false,
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

    fn take_current(&mut self) -> Option<SessionState> {
        let id = self.current_id.take()?;
        Some(SessionState {
            id,
            order: self.current_order,
            name: std::mem::take(&mut self.current_name),
            directory: std::mem::take(&mut self.directory),
            phase: self.phase,
            error: std::mem::take(&mut self.error),
            exit_code: self.exit_code.take(),
            terminal: std::mem::replace(
                &mut self.terminal,
                TerminalGrid::new(self.dimensions.1, self.dimensions.0),
            ),
            process: self.process.take(),
            dimensions: self.dimensions,
            generation: std::mem::replace(&mut self.generation, Arc::new(AtomicU64::new(0))),
            marked_text: std::mem::take(&mut self.marked_text),
            scroll_rows_accumulator: self.scroll_rows_accumulator,
            selection: self.selection.take(),
            selection_dragged: self.selection_dragged,
        })
    }

    fn restore_session(&mut self, session: SessionState) {
        self.current_id = Some(session.id);
        self.current_order = session.order;
        self.current_name = session.name;
        self.directory = session.directory;
        self.phase = session.phase;
        self.error = session.error;
        self.exit_code = session.exit_code;
        self.terminal = session.terminal;
        self.process = session.process;
        self.dimensions = session.dimensions;
        self.generation = session.generation;
        self.marked_text = session.marked_text;
        self.scroll_rows_accumulator = session.scroll_rows_accumulator;
        self.selection = session.selection;
        self.selection_dragged = session.selection_dragged;
        self.route = Route::Terminal;
    }

    fn activate_session(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_id == Some(id) {
            self.route = Route::Terminal;
            self.focus.focus(window, cx);
            return;
        }
        self.marked_text.clear();
        let Some(index) = self
            .parked_sessions
            .iter()
            .position(|session| session.id == id)
        else {
            return;
        };
        if let Some(current) = self.take_current() {
            self.parked_sessions.push(current);
        }
        let session = self.parked_sessions.swap_remove(index);
        self.restore_session(session);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn open_project(&mut self, project: ProjectEntry, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_id.is_some() && self.directory == project.path {
            self.project_picker_open = false;
            self.route = Route::Terminal;
            self.focus.focus(window, cx);
            cx.notify();
            return;
        }
        if let Some(id) = self
            .parked_sessions
            .iter()
            .find(|session| session.directory == project.path)
            .map(|session| session.id)
        {
            self.project_picker_open = false;
            self.activate_session(id, window, cx);
            return;
        }
        self.marked_text.clear();
        if let Some(current) = self.take_current() {
            self.parked_sessions.push(current);
        }
        let id = self.next_session_id;
        self.next_session_id += 1;
        self.restore_session(SessionState {
            id,
            order: id,
            name: project.display_name(),
            directory: project.path,
            phase: Phase::Starting,
            error: String::new(),
            exit_code: None,
            terminal: TerminalGrid::new(self.dimensions.1, self.dimensions.0),
            process: None,
            dimensions: self.dimensions,
            generation: Arc::new(AtomicU64::new(0)),
            marked_text: String::new(),
            scroll_rows_accumulator: 0.0,
            selection: None,
            selection_dragged: false,
        });
        self.project_picker_open = false;
        self.focus.focus(window, cx);
        self.start(cx);
    }

    fn close_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_process();
        self.generation.fetch_add(1, Ordering::AcqRel);
        let closed_order = self.current_order;
        self.current_id = None;
        let next_index = self
            .parked_sessions
            .iter()
            .enumerate()
            .filter(|(_, session)| session.order > closed_order)
            .min_by_key(|(_, session)| session.order)
            .map(|(index, _)| index)
            .or_else(|| {
                self.parked_sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, session)| session.order < closed_order)
                    .max_by_key(|(_, session)| session.order)
                    .map(|(index, _)| index)
            });
        if let Some(index) = next_index {
            let session = self.parked_sessions.swap_remove(index);
            self.restore_session(session);
            self.focus.focus(window, cx);
        } else {
            self.route = Route::Home;
            self.current_name.clear();
            self.directory.clear();
            self.project_picker_open = false;
        }
        cx.notify();
    }

    fn close_session(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_id == Some(id) {
            self.close_current(window, cx);
            return;
        }
        if let Some(index) = self
            .parked_sessions
            .iter()
            .position(|session| session.id == id)
        {
            let mut session = self.parked_sessions.swap_remove(index);
            if let Some(mut process) = session.process.take() {
                let _ = process.stop();
            }
            session.generation.fetch_add(1, Ordering::AcqRel);
            cx.notify();
        }
    }

    fn choose_project_directory(open_after_create: bool, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from("选择项目文件夹")),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| match canonical_directory(&path) {
                    Ok(path) => {
                        if this
                            .projects
                            .projects
                            .iter()
                            .any(|project| project.path == path)
                        {
                            this.projects_error = Some("该项目已存在".to_string());
                            return;
                        }
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or_default()
                            .to_string();
                        this.project_draft = Some(ProjectDraft {
                            path,
                            name,
                            marked_text: String::new(),
                            open_after_create,
                        });
                        cx.notify();
                    }
                    Err(error) => this.projects_error = Some(error),
                });
            }
        })
        .detach();
    }

    fn save_project_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.project_draft.take() else {
            return;
        };
        let project = ProjectEntry {
            name: draft.name,
            path: draft.path,
        };
        self.projects.projects.push(project.clone());
        if let Err(error) = backup_invalid_projects(&self.projects_path)
            .and_then(|_| save_projects(&self.projects_path, &self.projects))
        {
            self.projects.projects.pop();
            self.projects_error = Some(error);
            cx.notify();
            return;
        }
        self.projects_error = None;
        if draft.open_after_create {
            self.open_project(project, window, cx);
        } else {
            cx.notify();
        }
    }

    fn handle_parked_message(&mut self, message: AppMessage, cx: &mut Context<Self>) {
        let session_id = match &message {
            AppMessage::Started { session, .. }
            | AppMessage::StartFailed { session, .. }
            | AppMessage::Output { session, .. }
            | AppMessage::Exited { session, .. }
            | AppMessage::ReadFailed { session, .. } => *session,
        };
        let Some(state) = self
            .parked_sessions
            .iter_mut()
            .find(|state| state.id == session_id)
        else {
            if let AppMessage::Started { mut process, .. } = message {
                let _ = process.stop();
            }
            return;
        };
        match message {
            AppMessage::Started {
                generation,
                mut process,
                directory,
                ..
            } if state.generation.load(Ordering::Acquire) == generation => {
                if let Err(error) =
                    process.resize(TerminalSize::new(state.dimensions.0, state.dimensions.1))
                {
                    state.phase = Phase::Error;
                    state.error = error.to_string();
                } else {
                    state.process = Some(process);
                    state.directory = directory;
                    state.phase = Phase::Running;
                    state.error.clear();
                }
            }
            AppMessage::StartFailed {
                generation,
                message,
                ..
            } if state.generation.load(Ordering::Acquire) == generation => {
                state.phase = Phase::Error;
                state.error = message;
                state.process = None;
            }
            AppMessage::Output {
                generation, data, ..
            } if state.generation.load(Ordering::Acquire) == generation => {
                state.selection = None;
                state.terminal.process(&data);
                for response in state.terminal.take_responses() {
                    if let Some(process) = state.process.as_mut()
                        && let Err(error) = process.write_all(response.as_bytes())
                    {
                        state.phase = Phase::Error;
                        state.error = error.to_string();
                        break;
                    }
                }
            }
            AppMessage::Exited {
                generation, code, ..
            } if state.generation.load(Ordering::Acquire) == generation => {
                state.process = None;
                state.phase = Phase::Exited;
                state.exit_code = Some(code);
            }
            AppMessage::ReadFailed {
                generation,
                message,
                ..
            } if state.generation.load(Ordering::Acquire) == generation => {
                state.phase = Phase::Error;
                state.error = message;
            }
            _ => {}
        }
        cx.notify();
    }

    fn handle_message(&mut self, message: AppMessage, cx: &mut Context<Self>) {
        let session = match &message {
            AppMessage::Started { session, .. }
            | AppMessage::StartFailed { session, .. }
            | AppMessage::Output { session, .. }
            | AppMessage::Exited { session, .. }
            | AppMessage::ReadFailed { session, .. } => *session,
        };
        if self.current_id != Some(session) {
            self.handle_parked_message(message, cx);
            return;
        }
        match message {
            AppMessage::Started {
                generation,
                mut process,
                directory,
                ..
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
                ..
            } if self.is_current(generation) => {
                self.phase = Phase::Error;
                self.error = message;
                self.process = None;
                cx.notify();
            }
            AppMessage::Output {
                generation, data, ..
            } if self.is_current(generation) => {
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
            AppMessage::Exited {
                generation, code, ..
            } if self.is_current(generation) => {
                self.process = None;
                self.phase = Phase::Exited;
                self.exit_code = Some(code);
                cx.notify();
            }
            AppMessage::ReadFailed {
                generation,
                message,
                ..
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
        let Some(session) = self.current_id else {
            return;
        };
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
                            session,
                            generation,
                            process,
                            directory,
                        });
                        forward_pty_events(session, generation, &events, &messages);
                    }
                    Err(message) => {
                        let _ = messages.send_blocking(AppMessage::StartFailed {
                            session,
                            generation,
                            message,
                        });
                    }
                }
            });
        if let Err(error) = start_thread {
            let _ = start_failure_messages.send_blocking(AppMessage::StartFailed {
                session,
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

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if let Some(draft) = self.project_draft.as_mut() {
            if event.keystroke.modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("a") {
                draft.name.clear();
                draft.marked_text.clear();
                cx.notify();
                return;
            }
            if event.keystroke.modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("v") {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    draft.name.push_str(&text);
                    cx.notify();
                }
                return;
            }
            match event.keystroke.key.as_str() {
                "backspace" => {
                    draft.name.pop();
                    cx.notify();
                }
                "escape" => {
                    self.project_draft = None;
                    cx.notify();
                }
                _ => {
                    if let Some(text) = event.keystroke.key_char.as_deref() {
                        draft.name.push_str(text);
                        cx.notify();
                    }
                }
            }
            return;
        }
        if event.keystroke.key == "escape" && self.project_picker_open {
            self.project_picker_open = false;
            cx.notify();
            return;
        }
        if self.route != Route::Terminal {
            return;
        }
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
        if next != self.dimensions {
            self.dimensions = next;
            self.terminal.resize(next.1, next.0);
            if let Some(process) = self.process.as_mut() {
                let _ = process.resize(TerminalSize::new(next.0, next.1));
            }
        }
        for session in &mut self.parked_sessions {
            if session.dimensions == next {
                continue;
            }
            session.dimensions = next;
            session.terminal.resize(next.1, next.0);
            if let Some(process) = session.process.as_mut() {
                let _ = process.resize(TerminalSize::new(next.0, next.1));
            }
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

    fn project_card(
        index: usize,
        project: &ProjectEntry,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let name = project.display_name();
        let path = project.path.display().to_string();
        div()
            .id(("project-card", index))
            .role(Role::Button)
            .aria_label(format!("打开项目 {name}"))
            .w(px(240.0))
            .h(px(112.0))
            .p(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(TITLEBAR))
            .cursor_pointer()
            .hover_style(|style| style.border_color(rgb(ACCENT)))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(project) = this.projects.projects.get(index).cloned() {
                    this.open_project(project, window, cx);
                }
            }))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
            .child(
                div()
                    .mt(px(10.0))
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path),
            )
            .into_any_element()
    }

    fn project_draft_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = self.project_draft.as_ref();
        let name = draft.map_or_else(String::new, |draft| {
            format!("{}{}", draft.name, draft.marked_text)
        });
        let path = draft.map_or_else(String::new, |draft| draft.path.display().to_string());
        let draft_entity = cx.entity();
        let focus = self.focus.clone();
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x3c38_3650))
            .child(
                div()
                    .w(px(460.0))
                    .p(px(20.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(BACKGROUND))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("创建项目"))
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(11.0))
                            .text_color(rgb(MUTED))
                            .child(path),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .h(px(42.0))
                            .relative()
                            .flex()
                            .items_center()
                            .px(px(12.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(rgb(ACCENT))
                            .bg(rgb(TITLEBAR))
                            .child(if name.is_empty() {
                                "项目名称（可为空）".to_string()
                            } else {
                                name
                            })
                            .child(
                                canvas(
                                    move |_, _, _| (),
                                    move |bounds, (), window, cx| {
                                        window.handle_input(
                                            &focus,
                                            ElementInputHandler::new(bounds, draft_entity),
                                            cx,
                                        );
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("cancel-project")
                                    .role(Role::Button)
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .rounded(px(8.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.project_draft = None;
                                        cx.notify();
                                    }))
                                    .child("取消"),
                            )
                            .child(
                                div()
                                    .id("create-project-confirm")
                                    .role(Role::Button)
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .rounded(px(8.0))
                                    .bg(rgb(FOREGROUND))
                                    .text_color(rgb(BACKGROUND))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save_project_draft(window, cx);
                                    }))
                                    .child("创建"),
                            ),
                    ),
            )
    }

    fn home_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = self
            .projects
            .projects
            .iter()
            .enumerate()
            .map(|(index, project)| Self::project_card(index, project, cx))
            .collect::<Vec<_>>();
        let empty = cards.is_empty();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .p(px(28.0))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("项目"),
                    )
                    .child(
                        div()
                            .id("new-project-home")
                            .role(Role::Button)
                            .px(px(14.0))
                            .py(px(9.0))
                            .rounded(px(8.0))
                            .bg(rgb(FOREGROUND))
                            .text_color(rgb(BACKGROUND))
                            .cursor_pointer()
                            .on_click(cx.listener(|_this, _, _, cx| {
                                Self::choose_project_directory(false, cx);
                            }))
                            .child("+ 新建项目"),
                    ),
            )
            .when_some(self.projects_error.clone(), |page, error| {
                page.child(div().mt(px(12.0)).text_color(rgb(ERROR)).child(error))
            })
            .when(empty, |page| {
                page.child(
                    div()
                        .mt(px(80.0))
                        .text_color(rgb(MUTED))
                        .child("还没有项目，点击右上角新建项目"),
                )
            })
            .child(
                div()
                    .mt(px(24.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(16.0))
                    .children(cards),
            )
            .when(self.project_draft.is_some(), |page| {
                page.child(self.project_draft_panel(cx))
            })
    }

    fn project_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self
            .projects
            .projects
            .iter()
            .enumerate()
            .map(|(index, project)| {
                let name = project.display_name();
                let path = project.path.display().to_string();
                let opened = self.directory == project.path
                    || self
                        .parked_sessions
                        .iter()
                        .any(|session| session.directory == project.path);
                div()
                    .id(("picker-project", index))
                    .role(Role::Button)
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .hover_style(|style| style.bg(rgb(BORDER)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(project) = this.projects.projects.get(index).cloned() {
                            this.open_project(project, window, cx);
                        }
                    }))
                    .child(div().child(if opened {
                        format!("{name} · 已打开")
                    } else {
                        name
                    }))
                    .child(div().text_size(px(10.0)).text_color(rgb(MUTED)).child(path))
            })
            .collect::<Vec<_>>();
        div()
            .absolute()
            .inset_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.project_picker_open = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(44.0))
                    .right(px(80.0))
                    .w(px(320.0))
                    .max_h(px(420.0))
                    .p(px(8.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(BACKGROUND))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(entries)
                    .child(
                        div()
                            .id("new-project-picker")
                            .role(Role::Button)
                            .mt(px(6.0))
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(rgb(TITLEBAR))
                            .cursor_pointer()
                            .on_click(cx.listener(|_this, _, _, cx| {
                                Self::choose_project_directory(true, cx);
                            }))
                            .child("+ 新建项目…"),
                    ),
            )
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
        if let Some(draft) = self.project_draft.as_ref() {
            return (!draft.marked_text.is_empty())
                .then(|| 0..draft.marked_text.encode_utf16().count());
        }
        (!self.marked_text.is_empty()).then(|| 0..self.marked_text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.project_draft.as_mut() {
            draft.marked_text.clear();
            cx.notify();
        } else if !self.marked_text.is_empty() {
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
        if let Some(draft) = self.project_draft.as_mut() {
            draft.marked_text.clear();
            draft.name.push_str(text);
        } else {
            self.marked_text.clear();
            if !text.is_empty() {
                self.write(text.as_bytes(), cx);
            }
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
        if let Some(draft) = self.project_draft.as_mut() {
            draft.marked_text.clear();
            draft.marked_text.push_str(new_text);
        } else {
            self.marked_text.clear();
            self.marked_text.push_str(new_text);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<gpui::Pixels>> {
        if self.project_draft.is_some() {
            return Some(Bounds::new(
                point(
                    element_bounds.left() + px(16.0),
                    element_bounds.top() + px(16.0),
                ),
                size(px(CELL_WIDTH), px(CELL_HEIGHT)),
            ));
        }
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
        for session in &mut self.parked_sessions {
            if let Some(mut process) = session.process.take() {
                let _ = process.stop();
            }
            session.generation.fetch_add(1, Ordering::AcqRel);
        }
    }
}

impl Render for LureApp {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.route == Route::Terminal {
            self.sync_size(window);
        }
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
        let terminal_entity = cx.entity();
        let input_focus = self.focus.clone();
        let starting = self.phase == Phase::Starting;
        let route = self.route;
        let mut tab_data = self
            .parked_sessions
            .iter()
            .map(|session| {
                (
                    session.order,
                    session.id,
                    session.name.clone(),
                    session.phase,
                )
            })
            .collect::<Vec<_>>();
        if let Some(id) = self.current_id {
            tab_data.push((
                self.current_order,
                id,
                self.current_name.clone(),
                self.phase,
            ));
        }
        tab_data.sort_by_key(|tab| tab.0);
        let tabs = tab_data
            .into_iter()
            .map(|(_, id, name, phase)| {
                let active = self.current_id == Some(id);
                let element_id = usize::try_from(id).unwrap_or(usize::MAX);
                div()
                    .id(("tab-shell", element_id))
                    .h(px(36.0))
                    .max_w(px(180.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .bg(rgb(if active { BACKGROUND } else { TITLEBAR }))
                    .child(
                        div()
                            .id(("project-tab", element_id))
                            .role(Role::Tab)
                            .min_w_0()
                            .flex_1()
                            .px(px(10.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.activate_session(id, window, cx);
                            }))
                            .child(
                                div()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_color(rgb(if phase == Phase::Running {
                                        FOREGROUND
                                    } else {
                                        ERROR
                                    }))
                                    .child(name),
                            ),
                    )
                    .child(
                        div()
                            .id(("close-tab", element_id))
                            .role(Role::Button)
                            .px(px(8.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_session(id, window, cx);
                            }))
                            .child("×"),
                    )
            })
            .collect::<Vec<_>>();

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
                            .h_full()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
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
                            .when(route == Route::Home, |bar| {
                                bar.child(div().font_weight(FontWeight::SEMIBOLD).child("Lure"))
                            })
                            .when(route == Route::Terminal, |bar| {
                                bar.children(tabs).child(
                                    div()
                                        .id("new-tab")
                                        .role(Role::Button)
                                        .w(px(32.0))
                                        .h(px(32.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(8.0))
                                        .cursor_pointer()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .hover_style(|style| style.bg(rgb(BORDER)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.project_picker_open = !this.project_picker_open;
                                            cx.notify();
                                        }))
                                        .child("+"),
                                )
                            }),
                    )
                    .when(route == Route::Terminal, |titlebar| {
                        titlebar.child(
                            div()
                                .flex()
                                .gap(px(4.0))
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
                        )
                    }),
            )
            .when(route == Route::Home, |app| app.child(self.home_page(cx)))
            .when(route == Route::Terminal, |app| {
                app.child(
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
                        .on_scroll_wheel(cx.listener(
                            |this, event: &gpui::ScrollWheelEvent, _, cx| {
                                this.handle_scroll(event, cx);
                                cx.stop_propagation();
                            },
                        ))
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
                                        ElementInputHandler::new(bounds, terminal_entity),
                                        cx,
                                    );

                                    for (row, column, columns, background, _) in &painted_runs {
                                        window.paint_quad(
                                            PaintQuad::new(Bounds::new(
                                                point(
                                                    bounds.left()
                                                        + px(CELL_WIDTH * f32::from(*column)),
                                                    bounds.top()
                                                        + px(CELL_HEIGHT * f32::from(*row)),
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
                                                    bounds.left()
                                                        + px(CELL_WIDTH * f32::from(*column)),
                                                    bounds.top()
                                                        + px(CELL_HEIGHT * f32::from(*row)),
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
                                                bounds.left()
                                                    + px(CELL_WIDTH * f32::from(cursor.1)),
                                                bounds.top()
                                                    + px(CELL_HEIGHT * f32::from(cursor.0)),
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
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
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
            })
            .when(self.project_picker_open, |app| {
                app.child(self.project_picker(cx))
            })
            .when(
                route == Route::Terminal && self.project_draft.is_some(),
                |app| app.child(self.project_draft_panel(cx)),
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
    session: SessionId,
    generation: u64,
    events: &std::sync::mpsc::Receiver<PtyEvent>,
    messages: &Sender<AppMessage>,
) {
    while let Ok(event) = events.recv() {
        let message = match event {
            PtyEvent::Output(data) => AppMessage::Output {
                session,
                generation,
                data,
            },
            PtyEvent::Exited(status) => AppMessage::Exited {
                session,
                generation,
                code: status.exit_code,
            },
            PtyEvent::ReadFailed(message) => AppMessage::ReadFailed {
                session,
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
        cx.open_window(
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
        cx.activate(true);
    });
}
