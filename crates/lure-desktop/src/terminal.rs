//! ANSI/VT 字节流的纯 Rust 终端模型。

use crate::theme::{BACKGROUND, FOREGROUND, indexed_color};

pub const CELL_WIDTH: f32 = 8.0;
pub const CELL_HEIGHT: f32 = 16.0;
pub const TITLEBAR_HEIGHT: f32 = 48.0;
pub const TERMINAL_PADDING: f32 = 16.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    pub text: String,
    pub foreground: u32,
    pub background: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
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
}

pub struct TerminalGrid {
    parser: vt100::Parser,
}

impl TerminalGrid {
    #[must_use]
    pub fn new(rows: u16, columns: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, columns, 10_000),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn resize(&mut self, rows: u16, columns: u16) {
        self.parser.screen_mut().set_size(rows, columns);
    }

    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        self.parser.screen().size()
    }

    #[must_use]
    pub fn cursor(&self) -> (u16, u16) {
        self.parser.screen().cursor_position()
    }

    #[must_use]
    pub fn lines(&self) -> Vec<TerminalLine> {
        let (rows, columns) = self.size();
        (0..rows)
            .map(|row| TerminalLine {
                cells: (0..columns)
                    .map(|column| self.render_cell(row, column))
                    .collect(),
            })
            .collect()
    }

    fn render_cell(&self, row: u16, column: u16) -> TerminalCell {
        let cell = self.parser.screen().cell(row, column);
        let mut foreground = cell.map_or(FOREGROUND, |cell| color(cell.fgcolor(), FOREGROUND));
        let mut background = cell.map_or(BACKGROUND, |cell| color(cell.bgcolor(), BACKGROUND));
        if cell.is_some_and(vt100::Cell::inverse) {
            std::mem::swap(&mut foreground, &mut background);
        }
        TerminalCell {
            text: cell.map_or_else(
                || " ".to_string(),
                |cell| {
                    if cell.is_wide_continuation() || cell.contents().is_empty() {
                        " ".to_string()
                    } else {
                        cell.contents().to_string()
                    }
                },
            ),
            foreground,
            background,
            bold: cell.is_some_and(vt100::Cell::bold),
            italic: cell.is_some_and(vt100::Cell::italic),
            underline: cell.is_some_and(vt100::Cell::underline),
        }
    }
}

fn color(color: vt100::Color, default: u32) -> u32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(index) => indexed_color(index),
        vt100::Color::Rgb(red, green, blue) => {
            (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
        }
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
