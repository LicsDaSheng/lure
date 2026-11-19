#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::{OsStr, OsString};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use async_channel::{Receiver, Sender};
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, Corners, ElementInputHandler,
    EntityInputHandler, FocusHandle, FontStyle, FontWeight, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathPromptOptions, Render, Role,
    SharedString, TextAlign, TextRun, TitlebarOptions, UTF16Selection, UnderlineStyle, Window,
    WindowBounds, WindowOptions, canvas, div, point, prelude::*, px, rgb, rgb_to_hsla, rgba, size,
    svg,
};
use lure_desktop::home::{ProjectFilter, matches_project};
use lure_desktop::input::{KeyInput, Modifiers};
use lure_desktop::projects::{
    ProjectEntry, ProjectList, backup_invalid_projects, load_projects, projects_file_from_home,
    save_projects,
};
use lure_desktop::terminal::{
    CELL_HEIGHT, CELL_WIDTH, TERMINAL_PADDING, TITLEBAR_HEIGHT, TerminalGraphic, TerminalGrid,
    TerminalLine, TerminalSelection, VisibleRowCache, terminal_font, terminal_pixel_size,
    terminal_size_for_viewport,
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

/// 终端网格与 PTY 共用同一套 8 × 16 字符单元尺寸，使 Pi 的内联图片换算与文字一致。
fn pty_size((columns, rows): (u16, u16)) -> TerminalSize {
    let (pixel_width, pixel_height) = terminal_pixel_size(columns, rows);
    TerminalSize::with_pixels(columns, rows, pixel_width, pixel_height)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Home,
    Terminal,
}

const PROJECT_SIDEBAR_WIDTH: f32 = 220.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarVisibility {
    Expanded,
    Collapsed,
}

fn terminal_viewport_width(width: f32, sidebar: SidebarVisibility) -> f32 {
    (width
        - if sidebar == SidebarVisibility::Expanded {
            PROJECT_SIDEBAR_WIDTH
        } else {
            0.0
        })
    .max(0.0)
}

impl SidebarVisibility {
    fn toggle(&mut self) {
        *self = match self {
            Self::Expanded => Self::Collapsed,
            Self::Collapsed => Self::Expanded,
        };
    }
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
    current_name: String,
    directory: PathBuf,
    phase: Phase,
    error: String,
    exit_code: Option<u32>,
    terminal: TerminalGrid,
    visible_rows: VisibleRowCache<Vec<PaintedRun>>,
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
    home_query: String,
    home_marked_text: String,
    home_search_active: bool,
    home_filter: ProjectFilter,
    sidebar_visibility: SidebarVisibility,
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
            current_name: String::new(),
            directory: PathBuf::new(),
            phase: Phase::Exited,
            error: String::new(),
            exit_code: None,
            terminal: TerminalGrid::new(dimensions.1, dimensions.0),
            visible_rows: VisibleRowCache::default(),
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
            home_query: String::new(),
            home_marked_text: String::new(),
            home_search_active: false,
            home_filter: ProjectFilter::All,
            sidebar_visibility: SidebarVisibility::Expanded,
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
        self.focus.focus(window, cx);
        self.start(cx);
    }

    fn close_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_home = self.route == Route::Home;
        self.stop_process();
        let mut closing = std::mem::replace(
            &mut self.terminal,
            TerminalGrid::new(self.dimensions.1, self.dimensions.0),
        );
        Self::release_images(&mut closing, window);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.current_id = None;
        let next_index = self.projects.projects.iter().find_map(|project| {
            self.parked_sessions
                .iter()
                .position(|session| session.directory == project.path)
        });
        if let Some(index) = next_index {
            let session = self.parked_sessions.swap_remove(index);
            self.restore_session(session);
            if was_home {
                self.route = Route::Home;
            }
            self.focus.focus(window, cx);
        } else {
            self.route = Route::Home;
            self.current_name.clear();
            self.directory.clear();
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
            Self::release_images(&mut session.terminal, window);
            cx.notify();
        }
    }

    /// 关闭会话时把图片纹理从 GPU atlas 中移除，避免跨会话泄漏。
    fn release_images(terminal: &mut TerminalGrid, window: &mut Window) {
        for image in terminal.release_all_images() {
            let _ = window.drop_image(image);
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
                if let Err(error) = process.resize(pty_size(state.dimensions)) {
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
                if let Err(error) = process.resize(pty_size(self.dimensions)) {
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
                        PtyProcess::spawn(command, pty_size(dimensions))
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
        if event.keystroke.modifiers.platform && event.keystroke.key.eq_ignore_ascii_case("b") {
            self.sidebar_visibility.toggle();
            cx.notify();
            return;
        }
        if self.route == Route::Home {
            let key = event.keystroke.key.as_str();
            if event.keystroke.modifiers.platform && key.eq_ignore_ascii_case("k") {
                self.home_search_active = true;
                cx.notify();
                return;
            }
            if self.home_search_active {
                if key == "escape" {
                    self.home_search_active = false;
                    self.home_marked_text.clear();
                } else if key == "backspace" {
                    self.home_query.pop();
                } else if event.keystroke.modifiers.platform && key.eq_ignore_ascii_case("a") {
                    self.home_query.clear();
                } else if event.keystroke.modifiers.platform && key.eq_ignore_ascii_case("v") {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.home_query.push_str(&text);
                    }
                } else if let Some(text) = event.keystroke.key_char.as_deref() {
                    self.home_query.push_str(text);
                }
                cx.notify();
            }
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
        let sidebar = if self.sidebar_visibility == SidebarVisibility::Expanded {
            PROJECT_SIDEBAR_WIDTH
        } else {
            0.0
        };
        let column = ((x - sidebar - inset).max(0.0) / CELL_WIDTH).floor() as u16;
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
        let next = terminal_size_for_viewport(
            terminal_viewport_width(viewport.width.into(), self.sidebar_visibility),
            viewport.height.into(),
        );
        if next != self.dimensions {
            self.dimensions = next;
            self.terminal.resize(next.1, next.0);
            if let Some(process) = self.process.as_mut() {
                let _ = process.resize(pty_size(next));
            }
        }
        for session in &mut self.parked_sessions {
            if session.dimensions == next {
                continue;
            }
            session.dimensions = next;
            session.terminal.resize(next.1, next.0);
            if let Some(process) = session.process.as_mut() {
                let _ = process.resize(pty_size(next));
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

    fn home_icon(data: &'static [u8], size: f32, color: u32) -> impl IntoElement {
        svg()
            .data(data)
            .w(px(size))
            .h(px(size))
            .flex_none()
            .text_color(rgb(color))
    }

    #[allow(clippy::too_many_lines)]
    fn project_card(
        index: usize,
        project: &ProjectEntry,
        running: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let name = project.display_name();
        let path = project.path.display().to_string();
        div()
            .id(("project-card", index))
            .role(Role::Button)
            .aria_label(format!("打开项目 {name}"))
            .w(px(310.0))
            .h(px(190.0))
            .p(px(16.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(0x24_2A30))
            .bg(rgb(0x10_1419))
            .cursor_pointer()
            .hover_style(|style| style.border_color(rgb(0x19_BFAF)))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(project) = this.projects.projects.get(index).cloned() {
                    this.open_project(project, window, cx);
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .w(px(34.0))
                            .h(px(34.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(rgb(0x2C_343D))
                            .bg(rgb(0x1B_2127))
                            .child(Self::home_icon(
                                include_bytes!("../../../assets/home/robot.svg"),
                                16.0,
                                0x00D4_DCE7,
                            )),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(14.0))
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_none()
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(12.0))
                            .border_1()
                            .border_color(rgb(if running { 0x1B_5145 } else { 0x34_3B44 }))
                            .text_color(rgb(if running { 0x2A_D7A5 } else { 0x9A_A7B8 }))
                            .text_size(px(11.0))
                            .child(if running {
                                "● 运行中"
                            } else {
                                "未打开"
                            }),
                    ),
            )
            .child(
                div()
                    .mt(px(18.0))
                    .px(px(10.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .min_w_0()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(0x1E_252B))
                    .bg(rgb(0x0B_0E12))
                    .text_size(px(11.0))
                    .child(Self::home_icon(
                        include_bytes!("../../../assets/home/folder.svg"),
                        14.0,
                        0x0074_899E,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(path),
                    ),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0xAC_B8C7))
                    .child(if running {
                        "› 返回正在运行的 Pi 终端"
                    } else {
                        "› 打开 Pi 终端"
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(0x22_2830))
                    .pt(px(10.0))
                    .text_size(px(11.0))
                    .text_color(rgb(0x70_839A))
                    .child("Pi · 本地终端"),
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

    #[allow(clippy::too_many_lines)]
    fn project_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.projects.projects.len();
        let running_for = |path: &Path| {
            (self.current_id.is_some() && self.directory == path && self.phase == Phase::Running)
                || self
                    .parked_sessions
                    .iter()
                    .any(|session| session.directory == path && session.phase == Phase::Running)
        };
        div()
            .id("project-sidebar")
            .w(px(PROJECT_SIDEBAR_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(rgb(0x20_252B))
            .bg(rgb(0x0E_1014))
            .child(
                div()
                    .px(px(18.0))
                    .pt(px(18.0))
                    .pb(px(9.0))
                    .text_size(px(11.0))
                    .text_color(rgb(0x69_8098))
                    .child("工作台导航"),
            )
            .child(
                div()
                    .id("sidebar-home")
                    .role(Role::Button)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.route = Route::Home;
                        cx.notify();
                    }))
                    .mx(px(12.0))
                    .px(px(12.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgb(0x30_363E))
                    .bg(rgb(0x1D_2026))
                    .child(Self::home_icon(
                        include_bytes!("../../../assets/home/grid.svg"),
                        16.0,
                        0x0017_CCD1,
                    ))
                    .child(div().flex_1().text_size(px(13.0)).child("项目"))
                    .child(
                        div()
                            .px(px(6.0))
                            .rounded(px(5.0))
                            .bg(rgb(0x30_343B))
                            .text_size(px(11.0))
                            .child(total.to_string()),
                    ),
            )
            .child(
                div()
                    .mt(px(22.0))
                    .px(px(18.0))
                    .text_size(px(11.0))
                    .text_color(rgb(0x69_8098))
                    .child("本地项目目录"),
            )
            .children(
                self.projects
                    .projects
                    .iter()
                    .enumerate()
                    .map(|(index, project)| {
                        let name = project.display_name();
                        let is_running = running_for(&project.path);
                        let opened_id =
                            if self.current_id.is_some() && self.directory == project.path {
                                self.current_id
                            } else {
                                self.parked_sessions
                                    .iter()
                                    .find(|session| session.directory == project.path)
                                    .map(|session| session.id)
                            };
                        let selected = self.route == Route::Terminal
                            && self.current_id == opened_id
                            && opened_id.is_some();
                        div()
                            .id(("sidebar-project", index))
                            .role(Role::Button)
                            .mx(px(12.0))
                            .mt(px(3.0))
                            .px(px(9.0))
                            .h(px(31.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(if selected { 0x1D_2026 } else { 0x0E_1014 }))
                            .cursor_pointer()
                            .hover_style(|style| style.bg(rgb(0x1D_2026)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(project) = this.projects.projects.get(index).cloned() {
                                    this.open_project(project, window, cx);
                                }
                            }))
                            .child(Self::home_icon(
                                include_bytes!("../../../assets/home/folder.svg"),
                                14.0,
                                0x0070_869B,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0xA8_B6C7))
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_color(rgb(if is_running { 0x2A_D7A5 } else { 0x52_6276 }))
                                    .child("●"),
                            )
                            .when_some(opened_id, |row, id| {
                                row.child(
                                    div()
                                        .id(("sidebar-close-session", index))
                                        .role(Role::Button)
                                        .aria_label("关闭项目终端")
                                        .px(px(5.0))
                                        .cursor_pointer()
                                        .text_color(rgb(0xA8_B6C7))
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.close_session(id, window, cx);
                                        }))
                                        .child("×"),
                                )
                            })
                    }),
            )
            .child(div().flex_1())
    }

    #[allow(clippy::too_many_lines)]
    fn home_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let running_for = |path: &Path| {
            (self.current_id.is_some() && self.directory == path && self.phase == Phase::Running)
                || self
                    .parked_sessions
                    .iter()
                    .any(|session| session.directory == path && session.phase == Phase::Running)
        };
        let total = self.projects.projects.len();
        let active = self
            .projects
            .projects
            .iter()
            .filter(|p| running_for(&p.path))
            .count();
        let cards = self
            .projects
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| {
                matches_project(
                    project,
                    &self.home_query,
                    running_for(&project.path),
                    self.home_filter,
                )
            })
            .map(|(index, project)| {
                Self::project_card(index, project, running_for(&project.path), cx)
            })
            .collect::<Vec<_>>();
        let empty = cards.is_empty();
        div().flex_1().min_h_0().flex().bg(rgb(0x09_0B0D)).child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(24.0))
                        .py(px(18.0))
                        .border_b_1()
                        .border_color(rgb(0x20_252B))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(
                            div()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(5.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(19.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child("项目工作区"),
                                        )
                                        .child(
                                            div()
                                                .px(px(8.0))
                                                .py(px(3.0))
                                                .rounded(px(12.0))
                                                .border_1()
                                                .border_color(rgb(0x1B_5145))
                                                .text_color(rgb(0x2A_D7A5))
                                                .text_size(px(11.0))
                                                .child(format!("● {active} 运行中")),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(0x86_95A8))
                                        .child("每个项目在自己的本地目录中运行 Pi 终端。"),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(self.home_filter_button("全部", ProjectFilter::All, cx))
                                .child(self.home_filter_button(
                                    "运行中",
                                    ProjectFilter::Running,
                                    cx,
                                ))
                                .child(self.home_filter_button("未打开", ProjectFilter::Idle, cx)),
                        ),
                )
                .when_some(self.projects_error.clone(), |page, error| {
                    page.child(
                        div()
                            .mx(px(24.0))
                            .mt(px(12.0))
                            .text_color(rgb(ERROR))
                            .child(error),
                    )
                })
                .when(empty, |page| {
                    page.child(
                        div()
                            .m(px(24.0))
                            .text_color(rgb(0x8F_9EB1))
                            .child(if total == 0 {
                                "还没有项目，点击右上角新建项目"
                            } else {
                                "没有匹配的项目"
                            }),
                    )
                })
                .child(
                    div()
                        .p(px(24.0))
                        .flex()
                        .flex_wrap()
                        .gap(px(16.0))
                        .children(cards),
                )
                .when(self.project_draft.is_some(), |page| {
                    page.child(self.project_draft_panel(cx))
                }),
        )
    }

    fn home_filter_button(
        &self,
        label: &'static str,
        filter: ProjectFilter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.home_filter == filter;
        div()
            .id(label)
            .role(Role::Button)
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .bg(rgb(if selected { 0x29_2D35 } else { 0x15_1920 }))
            .text_color(rgb(if selected { FOREGROUND } else { 0x86_95A8 }))
            .text_size(px(11.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.home_filter = filter;
                cx.notify();
            }))
            .child(label)
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
            .bg(gpui::rgba((TITLEBAR << 8) | 0x66))
            .child(
                div()
                    .w(px(420.0))
                    .p(px(20.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(TITLEBAR))
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
        if self.route == Route::Home && self.home_search_active {
            return (!self.home_marked_text.is_empty())
                .then(|| 0..self.home_marked_text.encode_utf16().count());
        }
        (!self.marked_text.is_empty()).then(|| 0..self.marked_text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self.project_draft.as_mut() {
            draft.marked_text.clear();
            cx.notify();
        } else if self.route == Route::Home && self.home_search_active {
            self.home_marked_text.clear();
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
        } else if self.route == Route::Home && self.home_search_active {
            self.home_marked_text.clear();
            self.home_query.push_str(text);
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
        } else if self.route == Route::Home && self.home_search_active {
            self.home_marked_text.clear();
            self.home_marked_text.push_str(new_text);
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
        if self.route == Route::Home && self.home_search_active {
            return Some(Bounds::new(
                point(
                    element_bounds.left() + px(32.0),
                    element_bounds.top() + px(12.0),
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

struct PaintedRun {
    column: u16,
    columns: u16,
    background: u32,
    shaped: Option<gpui::ShapedLine>,
}

fn shape_terminal_row(line: &TerminalLine, window: &Window) -> Vec<PaintedRun> {
    let mut column = 0;
    line.runs()
        .into_iter()
        .map(|run| {
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
                    underline: run.underline.then_some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(color),
                        wavy: false,
                    }),
                    strikethrough: None,
                    letter_spacing: None,
                };
                // 13px JetBrainsMono 的自然字进宽约 7.8px，
                // 长行若直接整段塑形，边框和竖线会逐列偏离 8px 网格。
                let mut text_run = text_run;
                if run.text.is_ascii() && run.columns > 1 {
                    let measured = window.text_system().shape_line(
                        run.text.clone().into(),
                        px(13.0),
                        &[text_run.clone()],
                    );
                    text_run.letter_spacing = lure_desktop::terminal::terminal_run_tracking(
                        f32::from(measured.width()),
                        run.text.len(),
                        run.columns,
                    )
                    .map(px);
                }
                Some(
                    window
                        .text_system()
                        .shape_line(run.text.into(), px(13.0), &[text_run]),
                )
            };
            let painted = PaintedRun {
                column,
                columns: run.columns,
                background: run.background,
                shaped,
            };
            column += run.columns;
            painted
        })
        .collect()
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
        let terminal_rows = if self.route == Route::Terminal {
            self.terminal
                .cached_visible_rows(&mut self.visible_rows, |line| {
                    shape_terminal_row(line, window)
                })
        } else {
            self.visible_rows = VisibleRowCache::default();
            Vec::new()
        };
        let marked_text = self.marked_text.clone();
        let graphics = self.terminal.graphics();
        let mut released_images = self.terminal.take_released_images();
        // 后台项目会话仍会收到图片替换/删除事件，不能等到切回前台才回收 atlas。
        for session in &mut self.parked_sessions {
            released_images.extend(session.terminal.take_released_images());
        }
        let selection = self.selection;
        let dimensions = self.dimensions;
        let terminal_entity = cx.entity();
        let input_focus = self.focus.clone();
        let starting = self.phase == Phase::Starting;
        let route = self.route;
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
                    .border_color(rgb(if route == Route::Home {
                        0x20_252B
                    } else {
                        BORDER
                    }))
                    .bg(rgb(if route == Route::Home {
                        0x0E_1014
                    } else {
                        TITLEBAR
                    }))
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
                            .child(
                                div()
                                    .id("toggle-sidebar-btn")
                                    .role(Role::Button)
                                    .aria_label(
                                        if self.sidebar_visibility == SidebarVisibility::Expanded {
                                            "隐藏项目侧栏"
                                        } else {
                                            "展开项目侧栏"
                                        },
                                    )
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(7.0))
                                    .cursor_pointer()
                                    .hover_style(|style| style.bg(rgb(0x1D_2026)))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_visibility.toggle();
                                        cx.notify();
                                    }))
                                    .child(Self::home_icon(
                                        include_bytes!("../../../assets/home/sidebar.svg"),
                                        16.0,
                                        0x00A8_B6C7,
                                    )),
                            )
                            .when(route == Route::Home, |bar| {
                                bar.child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap(px(7.0))
                                        .child(
                                            div()
                                                .w(px(20.0))
                                                .h(px(20.0))
                                                .rounded(px(6.0))
                                                .bg(rgb(0x16_C7B5))
                                                .text_color(rgb(0x09_211F))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(Self::home_icon(
                                                    include_bytes!(
                                                        "../../../assets/home/brand.svg"
                                                    ),
                                                    14.0,
                                                    0x0009_211F,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_size(px(14.0))
                                                .child("Lure"),
                                        )
                                        .child(
                                            div()
                                                .ml(px(4.0))
                                                .px(px(6.0))
                                                .py(px(3.0))
                                                .rounded(px(4.0))
                                                .border_1()
                                                .border_color(rgb(0x2B_333A))
                                                .text_color(rgb(0x8F_9EB1))
                                                .text_size(px(10.0))
                                                .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                                        ),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .id("home-search")
                                        .role(Role::Button)
                                        .relative()
                                        .w(px(310.0))
                                        .h(px(32.0))
                                        .px(px(10.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .rounded(px(7.0))
                                        .border_1()
                                        .border_color(rgb(0x2B_333A))
                                        .bg(rgb(0x15_1920))
                                        .cursor_pointer()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.home_search_active = true;
                                            this.focus.focus(window, cx);
                                            cx.notify();
                                        }))
                                        .child(Self::home_icon(
                                            include_bytes!("../../../assets/home/search.svg"),
                                            14.0,
                                            0x0074_899E,
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .text_color(rgb(if self.home_query.is_empty() {
                                                    0x74_899E
                                                } else {
                                                    FOREGROUND
                                                }))
                                                .child(
                                                    if self.home_query.is_empty()
                                                        && self.home_marked_text.is_empty()
                                                    {
                                                        "搜索项目名称或本地目录路径…".to_string()
                                                    } else {
                                                        format!(
                                                            "{}{}",
                                                            self.home_query, self.home_marked_text
                                                        )
                                                    },
                                                ),
                                        )
                                        .child(
                                            div()
                                                .px(px(4.0))
                                                .rounded(px(4.0))
                                                .border_1()
                                                .border_color(rgb(0x2B_333A))
                                                .text_color(rgb(0x74_899E))
                                                .child("⌘K"),
                                        )
                                        .child(
                                            canvas(|_, _, _| (), {
                                                let focus = self.focus.clone();
                                                let entity = cx.entity();
                                                move |bounds, (), window, cx| {
                                                    window.handle_input(
                                                        &focus,
                                                        ElementInputHandler::new(bounds, entity),
                                                        cx,
                                                    );
                                                }
                                            })
                                            .absolute()
                                            .inset_0(),
                                        ),
                                )
                                .child(div().flex_1())
                            })
                            .when(route == Route::Terminal, |bar| {
                                bar.child(
                                    div()
                                        .id("terminal-home")
                                        .role(Role::Button)
                                        .px(px(10.0))
                                        .py(px(8.0))
                                        .rounded(px(7.0))
                                        .cursor_pointer()
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.route = Route::Home;
                                            cx.notify();
                                        }))
                                        .child("项目"),
                                )
                                .child(div().text_color(rgb(MUTED)).child("/"))
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_color(rgb(FOREGROUND))
                                        .child(self.current_name.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id("new-project-home")
                            .role(Role::Button)
                            .px(px(13.0))
                            .py(px(8.0))
                            .rounded(px(7.0))
                            .bg(rgb(0xEE_F2F5))
                            .text_color(rgb(0x10_1419))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(12.0))
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                Self::choose_project_directory(this.route == Route::Terminal, cx);
                            }))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(Self::home_icon(
                                include_bytes!("../../../assets/home/plus.svg"),
                                14.0,
                                0x0010_1419,
                            ))
                            .child("新建项目"),
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
            .when(route == Route::Home, |app| {
                app.child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .when(
                            self.sidebar_visibility == SidebarVisibility::Expanded,
                            |body| body.child(self.project_sidebar(cx)),
                        )
                        .child(self.home_page(cx)),
                )
            })
            .when(route == Route::Terminal, |app| {
                app.child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .when(
                            self.sidebar_visibility == SidebarVisibility::Expanded,
                            |body| body.child(self.project_sidebar(cx)),
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
                                .on_mouse_move(cx.listener(
                                    |this, event: &MouseMoveEvent, _window, cx| {
                                        if event.dragging() {
                                            this.update_selection(event.position, cx);
                                        }
                                    },
                                ))
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
                                            (terminal_rows, marked)
                                        },
                                        move |bounds, (terminal_rows, marked), window, cx| {
                                            window.handle_input(
                                                &input_focus,
                                                ElementInputHandler::new(bounds, terminal_entity),
                                                cx,
                                            );

                                            let runs = || {
                                                terminal_rows.iter().enumerate().flat_map(
                                                    |(row, runs)| {
                                                        let row =
                                                            u16::try_from(row).unwrap_or(u16::MAX);
                                                        runs.iter().map(move |run| (row, run))
                                                    },
                                                )
                                            };
                                            for (
                                                row,
                                                PaintedRun {
                                                    column,
                                                    columns,
                                                    background,
                                                    ..
                                                },
                                            ) in runs()
                                            {
                                                window.paint_quad(
                                                    PaintQuad::new(Bounds::new(
                                                        point(
                                                            bounds.left()
                                                                + px(
                                                                    CELL_WIDTH * f32::from(*column)
                                                                ),
                                                            bounds.top()
                                                                + px(CELL_HEIGHT * f32::from(row)),
                                                        ),
                                                        size(
                                                            px(CELL_WIDTH * f32::from(*columns)),
                                                            px(CELL_HEIGHT),
                                                        ),
                                                    ))
                                                    .background(rgb(*background)),
                                                );
                                            }

                                            let paint_graphic =
                                                |window: &mut Window, graphic: &TerminalGraphic| {
                                                    let dest = Bounds::new(
                                                        point(
                                                            bounds.left() + px(graphic.x),
                                                            bounds.top() + px(graphic.y),
                                                        ),
                                                        size(px(graphic.width), px(graphic.height)),
                                                    );
                                                    let clip = dest.intersect(&bounds);
                                                    if clip.size.width <= px(0.0)
                                                        || clip.size.height <= px(0.0)
                                                    {
                                                        return;
                                                    }
                                                    let [u0, v0, u1, v1] = graphic.source_rect;
                                                    let crop_width = (u1 - u0).max(f32::EPSILON);
                                                    let crop_height = (v1 - v0).max(f32::EPSILON);
                                                    let full_width = graphic.width / crop_width;
                                                    let full_height = graphic.height / crop_height;
                                                    let image_bounds = Bounds::new(
                                                        point(
                                                            dest.origin.x - px(u0 * full_width),
                                                            dest.origin.y - px(v0 * full_height),
                                                        ),
                                                        size(px(full_width), px(full_height)),
                                                    );
                                                    // 纹理分配失败时仅缺失该图片，不影响终端文字与交互。
                                                    let _ = window.paint_image(
                                                        clip,
                                                        image_bounds,
                                                        Corners::all(px(0.0)),
                                                        0.0,
                                                        graphic.image.clone(),
                                                        0,
                                                        false,
                                                    );
                                                };

                                            for image in released_images.drain(..) {
                                                let _ = window.drop_image(image);
                                            }

                                            // 负 z-index 图片绘制在单元格背景之上、文字之下。
                                            for graphic in &graphics {
                                                if graphic.z_index < 0 {
                                                    paint_graphic(window, graphic);
                                                }
                                            }

                                            for (row, PaintedRun { column, shaped, .. }) in runs() {
                                                if let Some(shaped) = shaped {
                                                    let _ = shaped.paint(
                                                        point(
                                                            bounds.left()
                                                                + px(
                                                                    CELL_WIDTH * f32::from(*column)
                                                                ),
                                                            bounds.top()
                                                                + px(CELL_HEIGHT * f32::from(row)),
                                                        ),
                                                        px(CELL_HEIGHT),
                                                        TextAlign::Left,
                                                        None,
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            }

                                            // 正 z-index 图片覆盖在文字之上。
                                            for graphic in &graphics {
                                                if graphic.z_index >= 0 {
                                                    paint_graphic(window, graphic);
                                                }
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
                                                                            * f32::from(
                                                                                columns.start,
                                                                            )),
                                                                    bounds.top()
                                                                        + px(CELL_HEIGHT
                                                                            * f32::from(row)),
                                                                ),
                                                                size(
                                                                    px(CELL_WIDTH
                                                                        * f32::from(
                                                                            columns.end
                                                                                - columns.start,
                                                                        )),
                                                                    px(CELL_HEIGHT),
                                                                ),
                                                            ))
                                                            .background(rgba(0x83a5_9880)),
                                                        );
                                                    }
                                                }
                                            }

                                            if cursor_visible {
                                                window.paint_quad(
                                                    PaintQuad::new(Bounds::new(
                                                        point(
                                                            bounds.left()
                                                                + px(CELL_WIDTH
                                                                    * f32::from(cursor.1)),
                                                            bounds.top()
                                                                + px(CELL_HEIGHT
                                                                    * f32::from(cursor.0)),
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
                        ),
                )
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

#[cfg(test)]
mod sidebar_tests {
    use super::*;

    #[test]
    fn terminal_viewport_tracks_sidebar_visibility() {
        for (width, sidebar, expected) in [
            (800.0, SidebarVisibility::Expanded, 580.0),
            (800.0, SidebarVisibility::Collapsed, 800.0),
            (100.0, SidebarVisibility::Expanded, 0.0),
        ] {
            assert!((terminal_viewport_width(width, sidebar) - expected).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn sidebar_toggle_round_trip() {
        let mut sidebar = SidebarVisibility::Expanded;
        sidebar.toggle();
        assert_eq!(sidebar, SidebarVisibility::Collapsed);
        sidebar.toggle();
        assert_eq!(sidebar, SidebarVisibility::Expanded);
    }
}
