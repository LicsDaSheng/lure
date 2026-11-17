//! ANSI/VT 字节流的纯 Rust 终端模型。

use crate::theme::{BACKGROUND, FOREGROUND, indexed_color};

pub const TERMINAL_FONT_FAMILY: &str = "JetBrainsMono Nerd Font Mono";
pub const TERMINAL_FONT_FALLBACKS: [&str; 4] = [
    "Sarasa Mono SC",
    "PingFang SC",
    "Hiragino Sans GB",
    "Apple Color Emoji",
];
pub const CELL_WIDTH: f32 = 8.0;
pub const CELL_HEIGHT: f32 = 16.0;
pub const TITLEBAR_HEIGHT: f32 = 48.0;
pub const TERMINAL_PADDING: f32 = 16.0;

#[must_use]
pub fn terminal_font() -> gpui::Font {
    let mut font = gpui::font(TERMINAL_FONT_FAMILY);
    font.fallbacks = Some(gpui::FontFallbacks::from_fonts(
        TERMINAL_FONT_FALLBACKS
            .iter()
            .map(ToString::to_string)
            .collect(),
    ));
    font
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    pub text: String,
    pub foreground: u32,
    pub background: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRun {
    pub text: String,
    pub columns: u16,
    pub wide: bool,
    pub foreground: u32,
    pub background: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSelection {
    anchor: (u16, u16),
    head: (u16, u16),
}

impl TerminalSelection {
    #[must_use]
    pub fn new(anchor: (u16, u16), head: (u16, u16)) -> Self {
        Self { anchor, head }
    }

    #[must_use]
    pub fn anchor(self) -> (u16, u16) {
        self.anchor
    }

    #[must_use]
    pub fn with_head(self, head: (u16, u16)) -> Self {
        Self {
            anchor: self.anchor,
            head,
        }
    }

    #[must_use]
    pub fn ordered(self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    #[must_use]
    pub fn columns_for_row(self, row: u16, columns: u16) -> Option<std::ops::Range<u16>> {
        let ((start_row, start_column), (end_row, end_column)) = self.ordered();
        if row < start_row || row > end_row || columns == 0 {
            return None;
        }
        let start = if row == start_row {
            start_column.min(columns - 1)
        } else {
            0
        };
        let end = if row == end_row {
            end_column.saturating_add(1).min(columns)
        } else {
            columns
        };
        (start < end).then_some(start..end)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLine {
    cells: Vec<TerminalCell>,
}

impl TerminalLine {
    #[must_use]
    pub fn cells(&self) -> &[TerminalCell] {
        &self.cells
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.cells.iter().map(|cell| cell.text.as_str()).collect()
    }

    /// 普通相邻单元格按样式批量塑形；每个宽字符独立锚定到网格列，避免回退字体字宽累积漂移。
    #[must_use]
    pub fn runs(&self) -> Vec<TerminalRun> {
        let mut runs: Vec<TerminalRun> = Vec::new();
        for (column, cell) in self.cells.iter().enumerate() {
            let wide = cell.text.is_empty()
                || (column + 1 < self.cells.len() && self.cells[column + 1].text.is_empty());
            let same_style = runs.last().is_some_and(|run| {
                run.wide == wide
                    && (!wide || cell.text.is_empty())
                    && run.foreground == cell.foreground
                    && run.background == cell.background
                    && run.bold == cell.bold
                    && run.italic == cell.italic
                    && run.underline == cell.underline
            });
            if same_style {
                if let Some(run) = runs.last_mut() {
                    run.text.push_str(&cell.text);
                    run.columns = run.columns.saturating_add(1);
                }
            } else {
                runs.push(TerminalRun {
                    text: cell.text.clone(),
                    columns: 1,
                    wide,
                    foreground: cell.foreground,
                    background: cell.background,
                    bold: cell.bold,
                    italic: cell.italic,
                    underline: cell.underline,
                });
            }
        }
        runs
    }
}

/// ANSI/VT 网格由 Alacritty 终端内核维护，展示层只读取可见单元格。
pub struct TerminalGrid {
    term: alacritty_terminal::term::Term<TerminalEvents>,
    processor: alacritty_terminal::vte::ansi::Processor,
    events: TerminalEvents,
}

#[derive(Clone, Default)]
struct TerminalEvents(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

impl alacritty_terminal::event::EventListener for TerminalEvents {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        if let alacritty_terminal::event::Event::PtyWrite(text) = event {
            self.0.borrow_mut().push(text);
        }
    }
}

#[derive(Clone, Copy)]
struct GridSize {
    rows: usize,
    columns: usize,
}

impl alacritty_terminal::grid::Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

impl TerminalGrid {
    #[must_use]
    pub fn new(rows: u16, columns: u16) -> Self {
        use alacritty_terminal::term::{Config, Term};
        let events = TerminalEvents::default();
        let config = Config {
            kitty_keyboard: true,
            ..Config::default()
        };
        let size = GridSize {
            rows: usize::from(rows.max(1)),
            columns: usize::from(columns.max(2)),
        };
        Self {
            term: Term::new(config, &size, events.clone()),
            processor: alacritty_terminal::vte::ansi::Processor::default(),
            events,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    /// 回复终端控制序列查询的字节必须由 PTY 发送，不能渲染成文本。
    #[must_use]
    pub fn take_responses(&self) -> Vec<String> {
        std::mem::take(&mut *self.events.0.borrow_mut())
    }

    pub fn resize(&mut self, rows: u16, columns: u16) {
        self.term.resize(GridSize {
            rows: usize::from(rows.max(1)),
            columns: usize::from(columns.max(2)),
        });
    }

    pub fn scroll_by(&mut self, rows: i32) {
        self.term
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(rows));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term
            .scroll_display(alacritty_terminal::grid::Scroll::Bottom);
    }

    #[must_use]
    pub fn scrollback(&self) -> usize {
        self.term.grid().display_offset()
    }

    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.term
            .mode()
            .contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
    }

    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        use alacritty_terminal::term::TermMode;
        self.scrollback() == 0 && self.term.mode().contains(TermMode::SHOW_CURSOR)
    }

    #[must_use]
    pub fn keyboard_input(&self, input: &crate::input::KeyInput) -> Option<Vec<u8>> {
        let mode = *self.term.mode();
        if input.key.eq_ignore_ascii_case("enter")
            && input.modifiers.shift
            && mode.contains(alacritty_terminal::term::TermMode::DISAMBIGUATE_ESC_CODES)
        {
            return Some(b"\x1b[13;2u".to_vec());
        }
        if mode.contains(alacritty_terminal::term::TermMode::APP_CURSOR)
            && !input.modifiers.control
            && !input.modifiers.alt
            && !input.modifiers.platform
        {
            let sequence = match input.key.to_ascii_lowercase().as_str() {
                "up" => Some(b"\x1bOA".to_vec()),
                "down" => Some(b"\x1bOB".to_vec()),
                "right" => Some(b"\x1bOC".to_vec()),
                "left" => Some(b"\x1bOD".to_vec()),
                _ => None,
            };
            if sequence.is_some() {
                return sequence;
            }
        }
        crate::input::terminal_input(input)
    }

    fn mouse_sequence(&self, row: u16, column: u16, code: u16, release: bool) -> Option<Vec<u8>> {
        use alacritty_terminal::term::TermMode;
        let mode = *self.term.mode();
        if !mode.intersects(TermMode::MOUSE_MODE) {
            return None;
        }
        let col = column.saturating_add(1);
        let row = row.saturating_add(1);
        if mode.contains(TermMode::SGR_MOUSE) {
            return Some(
                format!(
                    "\x1b[<{code};{col};{row}{}",
                    if release { 'm' } else { 'M' }
                )
                .into_bytes(),
            );
        }
        let code = if release { 3 } else { code };
        let mut bytes = b"\x1b[M".to_vec();
        for value in [code, col, row] {
            if !mode.contains(TermMode::UTF8_MOUSE) && value > 223 {
                return None;
            }
            let character = char::from_u32(u32::from(value.saturating_add(32)))?;
            let mut encoded = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
        }
        Some(bytes)
    }

    #[must_use]
    pub fn mouse_button_sequence(&self, row: u16, column: u16, release: bool) -> Option<Vec<u8>> {
        self.mouse_sequence(row, column, 0, release)
    }

    #[must_use]
    #[allow(clippy::fn_params_excessive_bools)]
    pub fn mouse_wheel_sequence(
        &self,
        row: u16,
        column: u16,
        up: bool,
        shift: bool,
        alt: bool,
        control: bool,
    ) -> Option<Vec<u8>> {
        let code = if up { 64 } else { 65 }
            + u16::from(shift) * 4
            + u16::from(alt) * 8
            + u16::from(control) * 16;
        self.mouse_sequence(row, column, code, false)
    }

    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        use alacritty_terminal::grid::Dimensions;
        (
            self.term.screen_lines().try_into().unwrap_or(u16::MAX),
            self.term.columns().try_into().unwrap_or(u16::MAX),
        )
    }

    #[must_use]
    pub fn cursor(&self) -> (u16, u16) {
        let point = self.term.renderable_content().cursor.point;
        (
            point.line.0.try_into().unwrap_or(0),
            point.column.0.try_into().unwrap_or(0),
        )
    }

    #[must_use]
    pub fn selection_text(&self, selection: TerminalSelection) -> String {
        let (rows, columns) = self.size();
        if rows == 0 || columns == 0 {
            return String::new();
        }
        let lines = self.lines();
        let normalized = TerminalSelection::new(
            self.normalize_selection_point(selection.anchor, &lines),
            self.normalize_selection_point(selection.head, &lines),
        );
        let ((start_row, _), (end_row, _)) = normalized.ordered();
        (start_row..=end_row.min(rows - 1))
            .filter_map(|row| {
                let range = normalized.columns_for_row(row, columns)?;
                let text = range
                    .map(|column| {
                        lines[usize::from(row)].cells[usize::from(column)]
                            .text
                            .as_str()
                    })
                    .collect::<String>();
                Some(text.trim_end_matches(' ').to_string())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn normalize_selection_point(
        &self,
        (row, column): (u16, u16),
        lines: &[TerminalLine],
    ) -> (u16, u16) {
        let (rows, columns) = self.size();
        let row = row.min(rows.saturating_sub(1));
        let mut column = column.min(columns.saturating_sub(1));
        if lines[usize::from(row)].cells[usize::from(column)]
            .text
            .is_empty()
        {
            column = column.saturating_sub(1);
        }
        (row, column)
    }

    #[must_use]
    pub fn lines(&self) -> Vec<TerminalLine> {
        use alacritty_terminal::term::cell::Flags;
        let (rows, columns) = self.size();
        let mut lines = vec![
            TerminalLine {
                cells: vec![
                    TerminalCell {
                        text: " ".to_string(),
                        foreground: FOREGROUND,
                        background: BACKGROUND,
                        bold: false,
                        italic: false,
                        underline: false,
                    };
                    usize::from(columns)
                ]
            };
            usize::from(rows)
        ];
        let visible = self.term.renderable_content();
        for item in visible.display_iter {
            let Some(row) = i64::from(item.point.line.0)
                .checked_add(i64::try_from(visible.display_offset).unwrap_or(i64::MAX))
                .and_then(|row| usize::try_from(row).ok())
            else {
                continue;
            };
            if row >= usize::from(rows) || item.point.column.0 >= usize::from(columns) {
                continue;
            }
            let cell = item.cell;
            let mut foreground = color(cell.fg, FOREGROUND);
            let mut background = color(cell.bg, BACKGROUND);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            let mut text = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                String::new()
            } else {
                cell.c.to_string()
            };
            if !text.is_empty()
                && let Some(extra) = cell.zerowidth()
            {
                text.extend(extra.iter());
            }
            lines[row].cells[item.point.column.0] = TerminalCell {
                text,
                foreground,
                background,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
            };
        }
        lines
    }
}

fn color(color: alacritty_terminal::vte::ansi::Color, default: u32) -> u32 {
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    match color {
        Color::Spec(rgb) => (u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b),
        Color::Indexed(index) => indexed_color(index),
        Color::Named(
            NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::DimForeground,
        ) => FOREGROUND,
        Color::Named(NamedColor::Background) => BACKGROUND,
        Color::Named(name) if (name as usize) < 16 => {
            indexed_color(u8::try_from(name as usize).unwrap_or_default())
        }
        Color::Named(_) => default,
    }
}

#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn terminal_size_for_viewport(width: f32, height: f32) -> (u16, u16) {
    let columns = ((width - TERMINAL_PADDING).max(0.0) / CELL_WIDTH).floor() as u16;
    let rows =
        ((height - TITLEBAR_HEIGHT - TERMINAL_PADDING).max(0.0) / CELL_HEIGHT).floor() as u16;
    (columns.max(2), rows.max(1))
}
