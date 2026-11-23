//! ANSI/VT 字节流的纯 Rust 终端模型。

use crate::theme::Theme;

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
/// 窗口外框描边宽度，两侧合计占用可视宽度/高度。
pub const WINDOW_BORDER: f32 = 1.0;
/// 单张终端图片解码后的像素字节上限（RGBA，约 4096 × 1024）。
pub const MAX_GRAPHIC_BYTES: usize = 16 * 1024 * 1024;
/// 每个项目会话的图片纹理内存上限。
pub const GRAPHICS_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// 全应用所有项目会话合计的图片纹理内存上限。
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

/// 可见区域内的一段 OSC 8 超链接，由同一行内连续的单元格组成。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverLink {
    /// 可见网格行号，0 为当前视口顶部。
    pub row: u16,
    /// 相对终端网格左侧的起始列。
    pub column: u16,
    /// 覆盖的列数；宽字符的占位单元格同样计入。
    pub columns: u16,
    /// 目标原文，原样保留，由展示层负责校验与打开。
    pub uri: String,
    /// 目标类型，决定打开时是否需要基准目录。
    pub kind: crate::links::LinkKind,
}

impl HoverLink {
    #[must_use]
    pub fn contains(&self, cell: (u16, u16)) -> bool {
        cell.0 == self.row
            && cell.1 >= self.column
            && cell.1 < self.column.saturating_add(self.columns)
    }
}

/// 终端字符网格在窗口中的几何位置。
///
/// 终端内容区相对窗口左上角的偏移由标题栏、可选侧栏与终端区域内边距决定，
/// 与 `terminal_size_for_viewport` 使用同一口径，避免命中测试与网格换算错位。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalGeometry {
    origin_x: f32,
    origin_y: f32,
    rows: u16,
    columns: u16,
}

impl TerminalGeometry {
    #[must_use]
    pub fn new(sidebar_width: f32, rows: u16, columns: u16) -> Self {
        // 终端区域四周内边距各占 `TERMINAL_PADDING` 的一半。
        let inset = TERMINAL_PADDING / 2.0;
        Self {
            origin_x: sidebar_width + inset,
            origin_y: TITLEBAR_HEIGHT + inset,
            rows,
            columns,
        }
    }

    /// 窗口坐标 → 单元格；落在标题栏、侧栏、网格外时返回 `None`。
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn cell_at(&self, x: f32, y: f32) -> Option<(u16, u16)> {
        if self.rows == 0 || self.columns == 0 {
            return None;
        }
        let local_x = x - self.origin_x;
        let local_y = y - self.origin_y;
        if !local_x.is_finite() || !local_y.is_finite() || local_x < 0.0 || local_y < 0.0 {
            return None;
        }
        let column = (local_x / CELL_WIDTH).floor();
        let row = (local_y / CELL_HEIGHT).floor();
        if column >= f32::from(self.columns) || row >= f32::from(self.rows) {
            return None;
        }
        Some((row as u16, column as u16))
    }

    /// 窗口坐标 → 单元格；网格外按原行为夹取到最近的边界单元格。
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn cell_at_or_clamped(&self, x: f32, y: f32) -> (u16, u16) {
        let rows = f32::from(self.rows.max(1).saturating_sub(1));
        let columns = f32::from(self.columns.max(1).saturating_sub(1));
        let column = ((x - self.origin_x).max(0.0) / CELL_WIDTH)
            .floor()
            .clamp(0.0, columns);
        let row = ((y - self.origin_y).max(0.0) / CELL_HEIGHT)
            .floor()
            .clamp(0.0, rows);
        (row as u16, column as u16)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLine {
    cells: Vec<TerminalCell>,
}

/// 缓存上一帧的可见行，最多保存一个视口，不随历史记录增长。
pub struct VisibleRowCache<T> {
    top: usize,
    rows: Vec<Option<CachedRow<T>>>,
    next: Vec<Option<CachedRow<T>>>,
}

impl<T> Default for VisibleRowCache<T> {
    fn default() -> Self {
        Self {
            top: 0,
            rows: Vec::new(),
            next: Vec::new(),
        }
    }
}

struct CachedRow<T> {
    line: TerminalLine,
    rendered: std::rc::Rc<T>,
}

type GridRow = rio_vt::crosswords::grid::row::Row<rio_vt::crosswords::square::Square>;

struct CellView<'a> {
    base: Option<char>,
    extras: &'a [char],
    foreground: u32,
    background: u32,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl CellView<'_> {
    fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.base.into_iter().chain(self.extras.iter().copied())
    }

    fn matches(&self, cell: &TerminalCell) -> bool {
        self.foreground == cell.foreground
            && self.background == cell.background
            && self.bold == cell.bold
            && self.italic == cell.italic
            && self.underline == cell.underline
            && self.chars().eq(cell.text.chars())
    }

    fn to_cell(&self) -> TerminalCell {
        TerminalCell {
            text: self.chars().collect(),
            foreground: self.foreground,
            background: self.background,
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
        }
    }
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
            // 回退字体与 Nerd 字形的自然字进宽不一定等于 8px；
            // 非 ASCII 字符单独锚定到网格列，避免长 run 累积误差。
            let same_style = runs.last().is_some_and(|run| {
                run.wide == wide
                    && (cell.text.is_empty() || (cell.text.is_ascii() && run.text.is_ascii()))
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
    theme: Theme,
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
    responses: Vec<TerminalResponse>,
    textures: TextureStore,
}

// 颜色查询直到取回复时才按当前主题求值，避免输出分块期间切换主题后回复旧色板。
enum TerminalResponse {
    Text(String),
    Color(
        usize,
        std::sync::Arc<dyn Fn(rio_vt::config::colors::ColorRgb) -> String + Send + Sync + 'static>,
    ),
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
    /// 已被替换、删除或回收的纹理，等待展示层从 GPU atlas 中移除。
    released: Vec<std::sync::Arc<gpui::RenderImage>>,
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
            self.released.push(old.image);
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
            self.released.push(old.image);
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
            self.released.push(old.image);
        }
        true
    }

    fn get(&self, image_id: u32) -> Option<&TextureEntry> {
        self.map.get(&image_id)
    }

    /// 取走所有待释放纹理，交给展示层从 GPU atlas 移除。
    fn take_released(&mut self) -> Vec<std::sync::Arc<gpui::RenderImage>> {
        std::mem::take(&mut self.released)
    }

    /// 清空当前项目会话的全部纹理（关闭会话时调用）。
    fn clear(&mut self) -> Vec<std::sync::Arc<gpui::RenderImage>> {
        use std::sync::atomic::Ordering;
        GLOBAL_TEXTURE_BYTES.fetch_sub(self.bytes, Ordering::AcqRel);
        self.bytes = 0;
        let mut released = std::mem::take(&mut self.released);
        released.extend(self.map.drain().map(|(_, entry)| entry.image));
        released
    }
}

fn push_link(links: &mut Vec<HoverLink>, uri: Option<String>, row: u16, column: u16, columns: u16) {
    if columns > 0
        && let Some(uri) = uri
    {
        links.push(HoverLink {
            row,
            column,
            columns,
            uri,
            kind: crate::links::LinkKind::Uri,
        });
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
                self.0
                    .borrow_mut()
                    .responses
                    .push(TerminalResponse::Text(text));
            }
            rio_vt::event::RioEvent::ColorRequest(_, index, formatter) => {
                self.0
                    .borrow_mut()
                    .responses
                    .push(TerminalResponse::Color(index, formatter));
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
    /// 仅更换绘制色板，不修改终端内容或子进程。
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

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
            // 保持独立终端模型的默认色板；应用绘制前显式应用所选主题。
            theme: Theme::Dark,
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
            // Rio 的 Kitty dest_row 是绝对行号（含已逐出的历史行），
            // 可见文字从当前历史顶部绘制；两者必须使用同一个原点。
            history_size: i64::try_from(self.term.lines_evicted())
                .unwrap_or(i64::MAX)
                .saturating_add(i64::try_from(self.term.history_size()).unwrap_or(i64::MAX)),
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

    /// 取走待释放的纹理，展示层应调用 `Window::drop_image` 释放 GPU 资源。
    #[must_use]
    pub fn take_released_images(&mut self) -> Vec<std::sync::Arc<gpui::RenderImage>> {
        self.events.0.borrow_mut().textures.take_released()
    }

    /// 释放当前项目会话 的全部图片纹理。
    #[must_use]
    pub fn release_all_images(&mut self) -> Vec<std::sync::Arc<gpui::RenderImage>> {
        self.graphics_memory = 0;
        self.events.0.borrow_mut().textures.clear()
    }

    /// 当前项目会话 持有的图片纹理字节数。
    #[must_use]
    pub fn graphics_memory_bytes(&self) -> usize {
        self.events.0.borrow().textures.bytes
    }

    /// 调整当前项目会话 的图片纹理内存上限；立即回收最久未使用的纹理直到满足上限。
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
        use rio_vt::config::colors::{ColorRgb, NamedColor};

        std::mem::take(&mut self.events.0.borrow_mut().responses)
            .into_iter()
            .filter_map(|response| match response {
                TerminalResponse::Text(text) => Some(text),
                TerminalResponse::Color(index, formatter) => {
                    let palette = self.theme.palette();
                    let rgb = if index < 16 {
                        self.theme.indexed_color(u8::try_from(index).ok()?)
                    } else if index == NamedColor::Foreground as usize {
                        palette.foreground
                    } else if index == NamedColor::Background as usize {
                        palette.background
                    } else if index == NamedColor::Cursor as usize {
                        palette.foreground
                    } else {
                        return None;
                    };
                    Some(formatter(ColorRgb {
                        r: u8::try_from((rgb >> 16) & 0xff).unwrap_or_default(),
                        g: u8::try_from((rgb >> 8) & 0xff).unwrap_or_default(),
                        b: u8::try_from(rgb & 0xff).unwrap_or_default(),
                    }))
                }
            })
            .collect()
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

    /// 按终端模式包装粘贴，不追加回车；过滤可提前结束 bracketed paste 的 ESC。
    #[must_use]
    pub fn paste_input(&self, text: &str) -> Vec<u8> {
        if self
            .term
            .mode()
            .contains(rio_vt::crosswords::Mode::BRACKETED_PASTE)
        {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
        } else {
            text.as_bytes().to_vec()
        }
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

    /// 左键按住时的位置报告；仅在子进程请求拖动或全部移动报告时编码。
    #[must_use]
    pub fn mouse_drag_sequence(&self, row: u16, column: u16) -> Option<Vec<u8>> {
        use rio_vt::crosswords::Mode;
        if !self
            .term
            .mode()
            .intersects(Mode::MOUSE_DRAG | Mode::MOUSE_MOTION)
        {
            return None;
        }
        // 鼠标协议中 32 表示移动，左键按钮编号为 0。
        self.mouse_sequence(row, column, 32, false)
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

    /// 终端字符网格在窗口中的几何；侧栏展开状态影响水平偏移。
    #[must_use]
    pub fn geometry(&self, sidebar_width: f32) -> TerminalGeometry {
        let (rows, columns) = self.size();
        TerminalGeometry::new(sidebar_width, rows, columns)
    }

    /// 扫描可见网格，返回按行、列排序的 OSC 8 链接区间。
    ///
    /// 只读取 rio-vt 单元格附带的超链接元数据，不根据输出文字推断路径或 URL。
    #[must_use]
    pub fn visible_links(&self) -> Vec<HoverLink> {
        let mut links = self.osc8_links();
        let chars = self.visible_grid_chars();
        if chars.is_empty() {
            return links;
        }
        // 文本识别只补充 OSC 8 未覆盖的区域：显式超链接始终优先。
        for detected in crate::links::detect_links(&chars) {
            for (row, column, columns) in crate::links::map_to_cells(&chars, &detected) {
                let overlaps = links.iter().any(|link| {
                    link.row == row
                        && link.column < column.saturating_add(columns)
                        && column < link.column.saturating_add(link.columns)
                });
                if overlaps {
                    continue;
                }
                links.push(HoverLink {
                    row,
                    column,
                    columns,
                    uri: detected.target.clone(),
                    kind: detected.kind,
                });
            }
        }
        links
    }

    /// 读取 rio-vt 单元格上的显式 OSC 8 超链接。
    fn osc8_links(&self) -> Vec<HoverLink> {
        use rio_vt::crosswords::pos::Column;
        let columns = self.term.columns();
        let mut links: Vec<HoverLink> = Vec::new();
        if columns == 0 {
            return links;
        }
        for (row, grid_row) in self.visible_grid_rows().enumerate() {
            // 没有 extras 的行不可能携带超链接，跳过整行扫描。
            if !grid_row.has_extras {
                continue;
            }
            let row = u16::try_from(row).unwrap_or(u16::MAX);
            let mut span: Option<(u16, u16, u16)> = None;
            for column in 0..columns {
                // 越出行宽的列视为无链接；`Row::len()` 可能小于网格列数。
                let id = (column < grid_row.len())
                    .then(|| grid_row[Column(column)])
                    .and_then(|cell| cell.has_hyperlink().then(|| cell.extras_id()).flatten());
                let same = matches!((span, id), (Some((current, ..)), Some(id)) if current == id);
                if same {
                    if let Some((_, _, columns)) = span.as_mut() {
                        *columns = columns.saturating_add(1);
                    }
                    continue;
                }
                if let Some((id, start, count)) = span.take() {
                    push_link(&mut links, self.hyperlink_uri(id), row, start, count);
                }
                if let Some(id) = id {
                    span = Some((id, u16::try_from(column).unwrap_or(u16::MAX), 1));
                }
            }
            if let Some((id, start, count)) = span {
                push_link(&mut links, self.hyperlink_uri(id), row, start, count);
            }
        }
        links
    }

    /// 可见网格的字符流，供文本识别使用。
    ///
    /// 每个字符携带自己的可见行与列：宽字符占两列，占位单元格被跳过，
    /// 软换行的行尾不插入分隔符，使跨行路径能拼成一段。
    fn visible_grid_chars(&self) -> Vec<crate::links::GridChar> {
        use rio_vt::crosswords::pos::Column;
        use rio_vt::crosswords::square::Wide;
        let columns = self.term.columns();
        let mut chars = Vec::new();
        for (index, grid_row) in self.visible_grid_rows().enumerate() {
            let row = u16::try_from(index).unwrap_or(u16::MAX);
            for column in 0..columns.min(grid_row.len()) {
                let square = grid_row[Column(column)];
                if matches!(square.wide(), Wide::Spacer | Wide::LeadingSpacer) {
                    continue;
                }
                let ch = match square.c() {
                    '\0' => ' ',
                    ch => ch,
                };
                let width = if matches!(square.wide(), Wide::Wide) {
                    2
                } else {
                    1
                };
                chars.push(crate::links::GridChar {
                    row,
                    column: u16::try_from(column).unwrap_or(u16::MAX),
                    width,
                    ch,
                });
            }
            // 非软换行的行尾补一个分隔符，避免相邻两行被拼成一个 token。
            if !grid_row.last().is_some_and(|cell| cell.wrapline()) {
                chars.push(crate::links::GridChar {
                    row,
                    column: u16::try_from(columns).unwrap_or(u16::MAX),
                    width: 1,
                    ch: ' ',
                });
            }
        }
        chars
    }

    /// 命中测试：返回覆盖 `cell` 的链接区间。
    #[must_use]
    pub fn link_at(&self, cell: (u16, u16)) -> Option<HoverLink> {
        self.visible_links()
            .into_iter()
            .find(|link| link.contains(cell))
    }

    /// 读取 extras 槽位的 OSC 8 目标地址；槽位回收后的链接视为不存在。
    fn hyperlink_uri(&self, extras_id: u16) -> Option<String> {
        self.term
            .grid
            .extras_table
            .get(extras_id)
            .and_then(|extras| extras.hyperlink.as_ref())
            .map(|hyperlink| hyperlink.uri().to_string())
    }

    #[must_use]
    pub fn lines(&self) -> Vec<TerminalLine> {
        self.visible_grid_rows()
            .map(|row| self.terminal_line(row))
            .collect()
    }

    /// 只为变化的可见行调用 `build`。缓存应由同一绘制上下文和构建逻辑复用。
    /// 在滚动或追加历史后尝试复用移位的行，始终按实际文本和样式校验。
    pub fn cached_visible_rows<T>(
        &self,
        cache: &mut VisibleRowCache<T>,
        mut build: impl FnMut(&TerminalLine) -> T,
    ) -> Vec<std::rc::Rc<T>> {
        let top = usize::try_from(self.term.lines_evicted())
            .unwrap_or(usize::MAX)
            .saturating_add(
                self.term
                    .history_size()
                    .saturating_sub(self.term.display_offset()),
            );
        let columns = self.term.columns();
        let mut rendered = Vec::with_capacity(self.term.screen_lines());
        for (index, row) in self.visible_grid_rows().enumerate() {
            let previous = if top >= cache.top {
                index.checked_add(top - cache.top)
            } else {
                index.checked_sub(cache.top - top)
            };
            let entry = previous
                .and_then(|index| cache.rows.get_mut(index))
                .and_then(Option::take)
                .filter(|entry| {
                    entry.line.cells.len() == columns
                        && (0..columns)
                            .all(|col| self.cell_view(row, col).matches(&entry.line.cells[col]))
                })
                .unwrap_or_else(|| {
                    let line = self.terminal_line(row);
                    let rendered = std::rc::Rc::new(build(&line));
                    CachedRow { line, rendered }
                });
            rendered.push(std::rc::Rc::clone(&entry.rendered));
            cache.next.push(Some(entry));
        }
        std::mem::swap(&mut cache.rows, &mut cache.next);
        cache.next.clear();
        cache.top = top;
        rendered
    }

    fn visible_grid_rows(&self) -> impl Iterator<Item = &GridRow> {
        use rio_vt::crosswords::pos::Line;
        let start = -i32::try_from(self.term.display_offset()).unwrap_or(i32::MAX);
        let height = i32::from(self.size().0);
        (start..start + height).map(|line| &self.term.grid[Line(line)])
    }

    fn terminal_line(&self, row: &GridRow) -> TerminalLine {
        TerminalLine {
            cells: (0..self.term.columns())
                .map(|column| self.cell_view(row, column).to_cell())
                .collect(),
        }
    }

    fn cell_view<'a>(&'a self, row: &'a GridRow, column: usize) -> CellView<'a> {
        use rio_vt::crosswords::pos::Column;
        use rio_vt::crosswords::square::Wide;
        use rio_vt::crosswords::style::StyleFlags;
        if column >= row.len() {
            return CellView {
                base: Some(' '),
                extras: &[],
                foreground: self.theme.palette().foreground,
                background: self.theme.palette().background,
                bold: false,
                italic: false,
                underline: false,
            };
        }
        let square = row[Column(column)];
        let style = self.term.grid.style_of(&square);
        let mut foreground = color(style.fg, self.theme.palette().foreground, self.theme);
        let mut background = color(style.bg, self.theme.palette().background, self.theme);
        if style.flags.contains(StyleFlags::INVERSE) {
            std::mem::swap(&mut foreground, &mut background);
        }
        let base = match (square.wide(), square.c()) {
            (Wide::Spacer | Wide::LeadingSpacer, _) => None,
            (_, '\0') => Some(' '),
            (_, c) => Some(c),
        };
        let extras = base
            .and_then(|_| square.extras_id_checked())
            .and_then(|id| self.term.grid.extras_table.get(id))
            .map_or(&[][..], |extras| extras.zerowidth.as_slice());
        CellView {
            base,
            extras,
            foreground,
            background,
            bold: style.flags.contains(StyleFlags::BOLD),
            italic: style.flags.contains(StyleFlags::ITALIC),
            underline: style.flags.intersects(StyleFlags::ALL_UNDERLINES),
        }
    }
}

fn color(color: rio_vt::config::colors::AnsiColor, default: u32, theme: Theme) -> u32 {
    use rio_vt::config::colors::{AnsiColor, NamedColor};
    match color {
        AnsiColor::Spec(rgb) => {
            (u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b)
        }
        AnsiColor::Indexed(index) => theme.indexed_color(index),
        AnsiColor::Named(
            NamedColor::Foreground | NamedColor::LightForeground | NamedColor::DimForeground,
        ) => theme.palette().foreground,
        AnsiColor::Named(NamedColor::Background) => theme.palette().background,
        AnsiColor::Named(name) if (name as usize) < 16 => {
            theme.indexed_color(u8::try_from(name as usize).unwrap_or_default())
        }
        AnsiColor::Named(_) => default,
    }
}

/// 字符网格对应的像素尺寸，用于同步 PTY 的 `ws_xpixel` / `ws_ypixel`。
#[must_use]
pub fn terminal_pixel_size(columns: u16, rows: u16) -> (u16, u16) {
    (
        columns.saturating_mul(u16::try_from(CELL_WIDTH_PX).unwrap_or(u16::MAX)),
        rows.saturating_mul(u16::try_from(CELL_HEIGHT_PX).unwrap_or(u16::MAX)),
    )
}

/// 对普通 ASCII run 按实际塑形宽度调整字距，令每个字符落在固定 8px 网格。
/// 单字符和异常宽度不做调整；非 ASCII 字形由独立网格锚点定位。
#[must_use]
pub fn terminal_run_tracking(shaped_width: f32, glyphs: usize, columns: u16) -> Option<f32> {
    if glyphs < 2
        || usize::from(columns) != glyphs
        || !shaped_width.is_finite()
        || shaped_width <= 0.0
    {
        return None;
    }
    let target = f32::from(columns) * CELL_WIDTH;
    Some((target - shaped_width) / f32::from(columns - 1))
}

#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn terminal_size_for_viewport(width: f32, height: f32) -> (u16, u16) {
    let border = WINDOW_BORDER * 2.0;
    let columns = ((width - TERMINAL_PADDING - border).max(0.0) / CELL_WIDTH).floor() as u16;
    let rows = ((height - TITLEBAR_HEIGHT - TERMINAL_PADDING - border).max(0.0) / CELL_HEIGHT)
        .floor() as u16;
    (columns.max(2), rows.max(1))
}
