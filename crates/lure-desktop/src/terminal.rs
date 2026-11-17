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
const CELL_WIDTH_PX: u32 = 8;
const CELL_HEIGHT_PX: u32 = 16;
pub const TITLEBAR_HEIGHT: f32 = 48.0;
pub const TERMINAL_PADDING: f32 = 16.0;
/// 单张终端图片解码后的像素字节上限（RGBA，约 4096 × 1024）。
pub const MAX_GRAPHIC_BYTES: usize = 16 * 1024 * 1024;
/// 每个终端 Tab 的图片纹理内存上限。
pub const GRAPHICS_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// 全应用所有 Tab 合计的图片纹理内存上限。
pub const GLOBAL_GRAPHICS_MEMORY_LIMIT: usize = 256 * 1024 * 1024;

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

/// 一张已解析的 Kitty Graphics 图片 placement，坐标相对终端内容原点（像素）。
#[derive(Clone)]
pub struct TerminalGraphic {
    pub image_id: u32,
    pub placement_id: u32,
    /// placement 目标矩形（相对终端内容原点）。
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// 归一化源裁剪矩形 `[u0, v0, u1, v1]`。
    pub source_rect: [f32; 4],
    pub z_index: i32,
    pub pixel_width: usize,
    pub pixel_height: usize,
    pub image: std::sync::Arc<gpui::RenderImage>,
}

impl std::fmt::Debug for TerminalGraphic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalGraphic")
            .field("image_id", &self.image_id)
            .field("placement_id", &self.placement_id)
            .field("x", &self.x)
            .field("y", &self.y)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("source_rect", &self.source_rect)
            .field("z_index", &self.z_index)
            .field("pixel_width", &self.pixel_width)
            .field("pixel_height", &self.pixel_height)
            .finish_non_exhaustive()
    }
}

/// ANSI/VT 网格由 rio-vt 终端内核维护，展示层只读取可见单元格与图片 placement。
pub struct TerminalGrid {
    term: rio_vt::crosswords::Crosswords<TerminalEvents>,
    processor: rio_vt::performer::handler::Processor,
    events: TerminalEvents,
    graphics_memory: usize,
    graphics_limit: usize,
}

#[derive(Clone, Default)]
struct TerminalEvents(std::rc::Rc<std::cell::RefCell<TerminalQueues>>);

#[derive(Default)]
struct TerminalQueues {
    responses: Vec<String>,
    textures: TextureStore,
}

struct TextureEntry {
    image: std::sync::Arc<gpui::RenderImage>,
    bytes: usize,
    sequence: u64,
}

#[derive(Default)]
struct TextureStore {
    map: std::collections::HashMap<u32, TextureEntry>,
    bytes: usize,
}

static TEXTURE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GLOBAL_TEXTURE_BYTES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

impl TextureStore {
    fn insert(&mut self, image_id: u32, graphic: &rio_graphics::GraphicData) {
        use std::sync::atomic::Ordering;
        let Some(image) = render_image(graphic) else {
            return;
        };
        let bytes = graphic
            .width
            .saturating_mul(graphic.height)
            .saturating_mul(4);
        if bytes == 0 || bytes > MAX_GRAPHIC_BYTES {
            return;
        }
        if let Some(old) = self.map.remove(&image_id) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
            GLOBAL_TEXTURE_BYTES.fetch_sub(old.bytes, Ordering::AcqRel);
        }
        let sequence = TEXTURE_SEQUENCE.fetch_add(1, Ordering::AcqRel);
        self.map.insert(
            image_id,
            TextureEntry {
                image,
                bytes,
                sequence,
            },
        );
        self.bytes = self.bytes.saturating_add(bytes);
        GLOBAL_TEXTURE_BYTES.fetch_add(bytes, Ordering::AcqRel);
    }

    fn remove(&mut self, image_id: u32) {
        use std::sync::atomic::Ordering;
        if let Some(old) = self.map.remove(&image_id) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
            GLOBAL_TEXTURE_BYTES.fetch_sub(old.bytes, Ordering::AcqRel);
        }
    }

    fn evict_oldest(&mut self) -> bool {
        use std::sync::atomic::Ordering;
        let Some((&id, _)) = self.map.iter().min_by_key(|(_, entry)| entry.sequence) else {
            return false;
        };
        if let Some(old) = self.map.remove(&id) {
            self.bytes = self.bytes.saturating_sub(old.bytes);
            GLOBAL_TEXTURE_BYTES.fetch_sub(old.bytes, Ordering::AcqRel);
        }
        true
    }

    fn get(&self, image_id: u32) -> Option<&TextureEntry> {
        self.map.get(&image_id)
    }
}

impl Drop for TextureStore {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        GLOBAL_TEXTURE_BYTES.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// 将 Rio 解码后的 RGB/RGBA 像素转换为 GPUI 需要的 BGRA 纹理。
fn render_image(graphic: &rio_graphics::GraphicData) -> Option<std::sync::Arc<gpui::RenderImage>> {
    let width = u32::try_from(graphic.width).ok()?;
    let height = u32::try_from(graphic.height).ok()?;
    let pixels = match graphic.color_type {
        rio_graphics::ColorType::Rgba => {
            if graphic.pixels.len() != graphic.width * graphic.height * 4 {
                return None;
            }
            let mut pixels = graphic.pixels.clone();
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            pixels
        }
        rio_graphics::ColorType::Rgb => {
            if graphic.pixels.len() != graphic.width * graphic.height * 3 {
                return None;
            }
            let mut converted = Vec::with_capacity(graphic.width * graphic.height * 4);
            for pixel in graphic.pixels.chunks_exact(3) {
                converted.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
            converted
        }
    };
    let buffer = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(width, height, pixels)?;
    Some(std::sync::Arc::new(gpui::RenderImage::new(
        smallvec::smallvec![image::Frame::new(buffer)],
    )))
}

impl rio_vt::event::EventListener for TerminalEvents {
    fn send_event(&self, event: rio_vt::event::RioEvent, _window: rio_vt::event::WindowId) {
        match event {
            rio_vt::event::RioEvent::PtyWrite(_, text) => {
                self.0.borrow_mut().responses.push(text);
            }
            rio_vt::event::RioEvent::UpdateGraphics { mut queues, .. } => {
                let mut queues_ref = self.0.borrow_mut();
                for (image_id, graphic) in queues.pending_images.drain(..) {
                    queues_ref.textures.insert(image_id, &graphic);
                }
                for key in queues.remove_queue.drain(..) {
                    // Kitty 图片纹理以协议 image_id（u32）为键；atlas 图形使用更高命名空间。
                    if key < (1_u64 << 32) {
                        queues_ref
                            .textures
                            .remove(u32::try_from(key).unwrap_or(u32::MAX));
                    }
                }
            }
            _ => {}
        }
    }
}

impl TerminalGrid {
    #[must_use]
    pub fn new(rows: u16, columns: u16) -> Self {
        use rio_vt::ansi::CursorShape;
        use rio_vt::crosswords::{Crosswords, CrosswordsSize};
        let events = TerminalEvents::default();
        let rows = usize::from(rows.max(1));
        let columns = usize::from(columns.max(2));
        let size = CrosswordsSize::new_with_dimensions(
            columns,
            rows,
            u32::try_from(columns)
                .unwrap_or(u32::MAX)
                .saturating_mul(CELL_WIDTH_PX),
            u32::try_from(rows)
                .unwrap_or(u32::MAX)
                .saturating_mul(CELL_HEIGHT_PX),
            CELL_WIDTH_PX,
            CELL_HEIGHT_PX,
        );
        let mut term = Crosswords::new(
            size,
            CursorShape::Block,
            events.clone(),
            rio_vt::event::WindowId::from(0),
            0,
            10_000,
        );
        term.graphics.total_limit = GRAPHICS_MEMORY_LIMIT;
        Self {
            term,
            processor: rio_vt::performer::handler::Processor::default(),
            events,
            graphics_memory: 0,
            graphics_limit: GRAPHICS_MEMORY_LIMIT,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
        self.enforce_graphics_limits();
    }

    fn enforce_graphics_limits(&mut self) {
        use std::sync::atomic::Ordering;
        loop {
            let (local, global) = {
                let queues = self.events.0.borrow();
                (
                    queues.textures.bytes,
                    GLOBAL_TEXTURE_BYTES.load(Ordering::Acquire),
                )
            };
            let over_local = local > self.graphics_limit;
            let over_global = global > GLOBAL_GRAPHICS_MEMORY_LIMIT;
            if !over_local && !over_global {
                break;
            }
            if !self.events.0.borrow_mut().textures.evict_oldest() {
                break;
            }
        }
        self.graphics_memory = self.events.0.borrow().textures.bytes;
    }

    /// 当前终端可见范围内的 Kitty Graphics placement，按 z-index 升序。
    #[must_use]
    pub fn graphics(&self) -> Vec<TerminalGraphic> {
        use rio_vt::ansi::graphics::{OverlayViewport, kitty_overlay_geometry};
        let queues = self.events.0.borrow();
        let viewport = OverlayViewport {
            cell_width: CELL_WIDTH,
            cell_height: CELL_HEIGHT,
            origin_x: 0.0,
            origin_y: 0.0,
            history_size: i64::try_from(self.term.history_size()).unwrap_or(i64::MAX),
            display_offset: i64::try_from(self.term.display_offset()).unwrap_or(i64::MAX),
            screen_lines: i64::try_from(self.term.screen_lines()).unwrap_or(i64::MAX),
        };
        let mut graphics = Vec::new();
        for (&(image_id, placement_id), placement) in &self.term.graphics.kitty_placements {
            let Some(texture) = queues.textures.get(image_id) else {
                continue;
            };
            let size = texture.image.size(0);
            let (width, height) = (usize::try_from(size.width.0).unwrap_or(0), {
                let h: i32 = size.height.0;
                usize::try_from(h).unwrap_or(0)
            });
            if width == 0 || height == 0 {
                continue;
            }
            let Some(geometry) = kitty_overlay_geometry(placement, width, height, &viewport) else {
                continue;
            };
            graphics.push(TerminalGraphic {
                image_id,
                placement_id,
                x: geometry.x,
                y: geometry.y,
                width: geometry.width,
                height: geometry.height,
                source_rect: geometry.source_rect,
                z_index: placement.z_index,
                pixel_width: width,
                pixel_height: height,
                image: texture.image.clone(),
            });
        }
        graphics.sort_by_key(|graphic| graphic.z_index);
        graphics
    }

    /// 当前 Tab 持有的图片纹理字节数。
    #[must_use]
    pub fn graphics_memory_bytes(&self) -> usize {
        self.events.0.borrow().textures.bytes
    }

    /// 调整当前 Tab 的图片纹理内存上限；立即回收最久未使用的纹理直到满足上限。
    pub fn set_graphics_memory_limit(&mut self, limit: usize) {
        self.graphics_limit = limit;
        self.term.graphics.total_limit = limit;
        while self.events.0.borrow().textures.bytes > self.graphics_limit {
            if !self.events.0.borrow_mut().textures.evict_oldest() {
                break;
            }
        }
        self.graphics_memory = self.events.0.borrow().textures.bytes;
    }

    /// 回复终端控制序列查询的字节必须由 PTY 发送，不能渲染成文本。
    #[must_use]
    pub fn take_responses(&self) -> Vec<String> {
        std::mem::take(&mut self.events.0.borrow_mut().responses)
    }

    pub fn resize(&mut self, rows: u16, columns: u16) {
        use rio_vt::crosswords::CrosswordsSize;
        let rows = usize::from(rows.max(1));
        let columns = usize::from(columns.max(2));
        self.term.resize(CrosswordsSize::new_with_dimensions(
            columns,
            rows,
            u32::try_from(columns)
                .unwrap_or(u32::MAX)
                .saturating_mul(CELL_WIDTH_PX),
            u32::try_from(rows)
                .unwrap_or(u32::MAX)
                .saturating_mul(CELL_HEIGHT_PX),
            CELL_WIDTH_PX,
            CELL_HEIGHT_PX,
        ));
    }

    pub fn scroll_by(&mut self, rows: i32) {
        self.term
            .scroll_display(rio_vt::crosswords::grid::Scroll::Delta(rows));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term
            .scroll_display(rio_vt::crosswords::grid::Scroll::Bottom);
    }

    #[must_use]
    pub fn scrollback(&self) -> usize {
        self.term.display_offset()
    }

    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.term
            .mode()
            .contains(rio_vt::crosswords::Mode::ALT_SCREEN)
    }

    #[must_use]
    pub fn cursor_visible(&self) -> bool {
        self.scrollback() == 0
            && self
                .term
                .mode()
                .contains(rio_vt::crosswords::Mode::SHOW_CURSOR)
    }

    #[must_use]
    pub fn keyboard_input(&self, input: &crate::input::KeyInput) -> Option<Vec<u8>> {
        use rio_vt::ansi::KeyboardModes;
        use rio_vt::crosswords::Mode;
        let mode = self.term.mode();
        if input.key.eq_ignore_ascii_case("enter")
            && input.modifiers.shift
            && self
                .term
                .keyboard_mode()
                .contains(KeyboardModes::DISAMBIGUATE_ESC_CODES)
        {
            return Some(b"\x1b[13;2u".to_vec());
        }
        if mode.contains(Mode::APP_CURSOR)
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
        use rio_vt::crosswords::Mode;
        let mode = self.term.mode();
        if !mode.intersects(Mode::MOUSE_MODE) {
            return None;
        }
        let col = column.saturating_add(1);
        let row = row.saturating_add(1);
        if mode.contains(Mode::SGR_MOUSE) {
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
            if !mode.contains(Mode::UTF8_MOUSE) && value > 223 {
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
        (
            self.term.screen_lines().try_into().unwrap_or(u16::MAX),
            self.term.columns().try_into().unwrap_or(u16::MAX),
        )
    }

    #[must_use]
    pub fn cursor(&self) -> (u16, u16) {
        let cursor = self.term.cursor();
        (
            cursor.pos.row.0.try_into().unwrap_or(0),
            cursor.pos.col.0.try_into().unwrap_or(0),
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
        use rio_vt::crosswords::pos::Column;
        use rio_vt::crosswords::square::Wide;
        use rio_vt::crosswords::style::StyleFlags;
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
        let visible = self.term.visible_rows();
        for (row_index, row) in visible.iter().take(usize::from(rows)).enumerate() {
            for column in 0..usize::from(columns) {
                if column >= row.len() {
                    break;
                }
                let square = row[Column(column)];
                let style = self.term.grid.style_of(&square);
                let mut foreground = color(style.fg, FOREGROUND);
                let mut background = color(style.bg, BACKGROUND);
                if style.flags.contains(StyleFlags::INVERSE) {
                    std::mem::swap(&mut foreground, &mut background);
                }
                let mut text = if matches!(square.wide(), Wide::Spacer | Wide::LeadingSpacer) {
                    String::new()
                } else {
                    match square.c() {
                        '\0' => " ".to_string(),
                        c => c.to_string(),
                    }
                };
                if !text.is_empty()
                    && let Some(extras_id) = square.extras_id_checked()
                    && let Some(extras) = self.term.grid.extras_table.get(extras_id)
                {
                    text.extend(extras.zerowidth.iter());
                }
                lines[row_index].cells[column] = TerminalCell {
                    text,
                    foreground,
                    background,
                    bold: style.flags.contains(StyleFlags::BOLD),
                    italic: style.flags.contains(StyleFlags::ITALIC),
                    underline: style.flags.intersects(StyleFlags::ALL_UNDERLINES),
                };
            }
        }
        lines
    }
}

fn color(color: rio_vt::config::colors::AnsiColor, default: u32) -> u32 {
    use rio_vt::config::colors::{AnsiColor, NamedColor};
    match color {
        AnsiColor::Spec(rgb) => {
            (u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b)
        }
        AnsiColor::Indexed(index) => indexed_color(index),
        AnsiColor::Named(
            NamedColor::Foreground | NamedColor::LightForeground | NamedColor::DimForeground,
        ) => FOREGROUND,
        AnsiColor::Named(NamedColor::Background) => BACKGROUND,
        AnsiColor::Named(name) if (name as usize) < 16 => {
            indexed_color(u8::try_from(name as usize).unwrap_or_default())
        }
        AnsiColor::Named(_) => default,
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
