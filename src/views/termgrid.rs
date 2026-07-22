use std::cell::{Cell, RefCell};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::text::TextOptions;
use crate::themes::{FontStyle, Renderer, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};

/// Default grid geometry (a classic terminal) and text size (dips).
const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;
const DEFAULT_FONT_SIZE: f32 = 14.0;
const DEFAULT_FONT: &str = "Noto Sans Mono";
const DEFAULT_FG: u32 = 0xFFE0E0E0;

/// Style bits of a [`TermCell`] (bitflags in `flags`).
pub const TERM_BOLD: u8 = 1;
pub const TERM_UNDERLINE: u8 = 2;
pub const TERM_INVERSE: u8 = 4;

/// One character cell: a scalar + ARGB colors + style flags. `bg == 0` (fully transparent) means
/// "the widget's background shows through" — the common case, so a mostly-empty screen costs no
/// per-cell rects.
#[derive(Clone, Copy)]
pub struct TermCell {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub flags: u8,
}

impl Default for TermCell {
    fn default() -> Self {
        TermCell { ch: ' ', fg: DEFAULT_FG, bg: 0, flags: 0 }
    }
}

/// A monospace character-cell grid — the display half of a terminal emulator. Deliberately
/// **dumb**: no escape-sequence parsing, no scrollback, no line editing — the embedding
/// application owns all terminal semantics and pushes cell updates ([`TermGrid::apply_cells`],
/// a packed binary format, see below) plus the cursor ([`TermGrid::set_cursor`]). Keyboard input
/// is likewise the embedder's business: the widget is focusable but consumes no keys itself, so
/// an embedder can route the raw key stream of a focused TermGrid wholesale (Tab and all).
///
/// The packed `apply_cells` format (all little-endian u32s): a 12-byte header
/// `[cols, first_row, row_count]` followed by `cols × row_count` cells of 16 bytes each:
/// `[ch, fg, bg, flags]` (`ch` a unicode scalar, colors ARGB, `flags` the TERM_* bits). A header
/// whose `cols`/`first_row + row_count` don't match the current grid resizes it — so a full-grid
/// push with `first_row = 0` is also how the grid is (re)sized.
pub struct TermGrid {
    state: RefCell<FieldsMain>,
    cols: Cell<usize>,
    rows: Cell<usize>,
    cells: RefCell<Vec<TermCell>>,
    /// (col, row, visible) — drawn as an inverse-video block.
    cursor: Cell<(usize, usize, bool)>,
    /// Cell metrics in physical px, measured at the last layout (one shaped glyph).
    cell_w: Cell<f32>,
    cell_h: Cell<f32>,
}

impl HasMainFields for TermGrid {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for TermGrid {}

impl TermGrid {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> TermGrid {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.font_manager.set_font(DEFAULT_FONT);
        TermGrid {
            state: RefCell::new(main),
            cols: Cell::new(DEFAULT_COLS),
            rows: Cell::new(DEFAULT_ROWS),
            cells: RefCell::new(vec![TermCell::default(); DEFAULT_COLS * DEFAULT_ROWS]),
            cursor: Cell::new((0, 0, true)),
            cell_w: Cell::new(0.0),
            cell_h: Cell::new(0.0),
        }
    }

    pub fn get_cols(&self) -> usize {
        self.cols.get()
    }

    pub fn get_rows(&self) -> usize {
        self.rows.get()
    }

    /// Cell metrics in physical px, valid after the first layout (0.0 before). An embedder reads
    /// them to compute how many cols×rows fit a window, then pushes a matching grid.
    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_w.get(), self.cell_h.get())
    }

    /// (Re)size the grid, preserving nothing (the embedder repaints in full after a resize).
    pub fn resize(&self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.cols.set(cols);
        self.rows.set(rows);
        *self.cells.borrow_mut() = vec![TermCell::default(); cols * rows];
    }

    /// Move the cursor (cells; clamped at draw time) and set its visibility.
    pub fn set_cursor(&self, col: usize, row: usize, visible: bool) {
        self.cursor.set((col, row, visible));
    }

    /// Apply a packed cell update (format in the type doc). Returns false if the payload is
    /// malformed (bad header/length); a well-formed payload always applies.
    pub fn apply_cells(&self, data: &[u8]) -> bool {
        let u32_at = |off: usize| -> u32 {
            u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
        };
        if data.len() < 12 {
            return false;
        }
        let cols = u32_at(0) as usize;
        let first_row = u32_at(4) as usize;
        let row_count = u32_at(8) as usize;
        let need = cols.checked_mul(row_count).and_then(|c| c.checked_mul(16));
        if cols == 0 || need.is_none_or(|n| data.len() != 12 + n) {
            return false;
        }
        // A mismatched shape resizes the grid (a full push with first_row = 0 sets the size).
        if cols != self.cols.get() || first_row + row_count > self.rows.get() {
            self.resize(cols, first_row + row_count);
        }
        let mut cells = self.cells.borrow_mut();
        for i in 0..cols * row_count {
            let off = 12 + i * 16;
            let ch = char::from_u32(u32_at(off)).unwrap_or(' ');
            cells[first_row * cols + i] = TermCell {
                ch,
                fg: u32_at(off + 4),
                bg: u32_at(off + 8),
                flags: u32_at(off + 12) as u8,
            };
        }
        true
    }

    /// Measure one glyph of the grid's typeface → the cell advance/height (mono assumed).
    fn measure(&self, parent_typeface: &Typeface, scale: f64) {
        let typeface = self.state.borrow().get_typeface(parent_typeface);
        let size = typeface.font_size.unwrap_or(DEFAULT_FONT_SIZE) * scale as f32;
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            let block = font.layout_text("W", size, TextOptions::new());
            self.cell_w.set(block.width());
            self.cell_h.set(block.height());
        }
    }
}

impl View for TermGrid {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "cols" => {
                if let Ok(c) = value.parse::<usize>() {
                    self.resize(c, self.rows.get());
                }
            }
            "rows" => {
                if let Ok(r) = value.parse::<usize>() {
                    self.resize(self.cols.get(), r);
                }
            }
            "font" => self.state.borrow_mut().font_manager.set_font(value),
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.state.borrow_mut().font_manager.set_font_size(size);
                }
            }
            _ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.base_set_scale(scale);
        self.measure(typeface, scale);
        let (new_width, new_height) = self.calculate_size(width, height, scale);
        let r = rect((x, y), (x + new_width, y + new_height));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let r = self.state.borrow().rect;
        r.width() <= width && r.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Renderer) {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);
        let scale = state.scale;
        let padding = state.padding.scaled(scale);
        let typeface = state.get_typeface(&theme.typeface("default"));
        drop(state);

        theme.push_clip();
        let clip = theme.clip_rect(r);

        // The widget background: an explicit `background` attr, a 9-patch, or the theme's field
        // background — a terminal is a "dark field" like an edit box.
        if !self.base_draw_ninepatch(theme, r) {
            let back = self.base_get_background().unwrap_or_else(|| theme.color("edit.back"));
            theme.draw_rect(r, back);
        }

        let (cw, ch) = (self.cell_w.get(), self.cell_h.get());
        if cw <= 0.0 || ch <= 0.0 {
            theme.pop_clip();
            return;
        }
        let (x0, y0) = (r.min.x + padding.left, r.min.y + padding.top);
        let (cols, rows) = (self.cols.get(), self.rows.get());
        let (cur_col, cur_row, cur_visible) = self.cursor.get();
        let cells = self.cells.borrow();
        let size = typeface.font_size.unwrap_or(DEFAULT_FONT_SIZE) * scale as f32;
        let regular = get_font_family(&typeface.font_name, typeface.font_style);
        let bold = get_font_family(&typeface.font_name, FontStyle::Bold).or(regular.clone());

        // The effective colors of a cell, after inverse-video and the cursor block.
        let effective = |col: usize, row: usize, cell: &TermCell| -> (u32, u32) {
            let mut fg = cell.fg;
            let mut bg = cell.bg;
            let mut inverse = cell.flags & TERM_INVERSE != 0;
            if cur_visible && col == cur_col && row == cur_row {
                inverse = !inverse;
            }
            if inverse {
                // A transparent bg inverts against the widget background stand-in (fg on fg would
                // vanish) — use the fg as the block and the back color for the glyph.
                std::mem::swap(&mut fg, &mut bg);
                if fg == 0 {
                    fg = 0xFF000000 | !bg & 0x00FFFFFF; // contrast fallback
                }
                bg |= 0xFF000000; // an inverse block is always opaque
            }
            (fg, bg)
        };

        for row in 0..rows {
            let ry = y0 + (row as f32 * ch).round() as i32;
            if ry > clip.max.y || ry + (ch as i32) < clip.min.y {
                continue;
            }
            // Pass 1: background runs (adjacent equal bg cells collapse into one rect).
            let mut run_start = 0usize;
            let mut run_bg = 0u32;
            let flush = |start: usize, end: usize, bg: u32, theme: &mut dyn Renderer| {
                if bg & 0xFF000000 != 0 && end > start {
                    let bx0 = x0 + (start as f32 * cw).round() as i32;
                    let bx1 = x0 + (end as f32 * cw).round() as i32;
                    theme.draw_rect(rect((bx0, ry), (bx1, ry + ch.ceil() as i32)), bg);
                }
            };
            for col in 0..cols {
                let (_, bg) = effective(col, row, &cells[row * cols + col]);
                if col == 0 {
                    run_bg = bg;
                } else if bg != run_bg {
                    flush(run_start, col, run_bg, theme);
                    run_start = col;
                    run_bg = bg;
                }
            }
            flush(run_start, cols, run_bg, theme);

            // Pass 2: text runs (adjacent cells with equal fg + style shape as one string —
            // mono metrics keep columns aligned regardless of shaping).
            let mut col = 0usize;
            while col < cols {
                let cell = &cells[row * cols + col];
                let (fg, _) = effective(col, row, cell);
                let flags = cell.flags & (TERM_BOLD | TERM_UNDERLINE);
                let mut text = String::new();
                let start = col;
                while col < cols {
                    let c2 = &cells[row * cols + col];
                    let (fg2, _) = effective(col, row, c2);
                    if fg2 != fg || c2.flags & (TERM_BOLD | TERM_UNDERLINE) != flags {
                        break;
                    }
                    text.push(c2.ch);
                    col += 1;
                }
                let rx = x0 + (start as f32 * cw).round() as i32;
                if !text.trim_end().is_empty() {
                    let font =
                        if flags & TERM_BOLD != 0 { bold.as_ref() } else { regular.as_ref() };
                    if let Some(font) = font {
                        let block = font.layout_text(&text, size, TextOptions::new());
                        theme.draw_text(rx as f32, ry as f32, fg, &block);
                    }
                }
                if flags & TERM_UNDERLINE != 0 {
                    let uy = ry + ch as i32 - (scale.max(1.0)) as i32;
                    let ux1 = x0 + (col as f32 * cw).round() as i32;
                    theme.draw_rect(rect((rx, uy), (ux1, uy + scale.max(1.0) as i32)), fg);
                }
            }
        }
        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().state)
    }

    fn get_rect(&self) -> Rect<i32> {
        self.base_get_rect()
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.base_set_rect(rect);
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.base_get_padding(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_padding(top, left, right, bottom);
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.base_get_margin(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_margin(top, left, right, bottom);
    }

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        // The natural size of the grid at the measured cell metrics (scaled px — the metrics are
        // measured at the current scale; base sizing treats this as the content floor).
        let w = (self.cols.get() as f32 * self.cell_w.get()).ceil() as i32;
        let h = (self.rows.get() as f32 * self.cell_h.get()).ceil() as i32;
        (w, h)
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn is_focused(&self) -> bool {
        self.base_is_focused()
    }

    fn set_focused(&self, focused: bool) {
        self.base_set_focused(focused);
    }

    fn set_focusable(&self, focusable: bool) {
        self.base_set_focusable(focusable);
    }

    fn set_width(&mut self, width: Dimension) {
        self.base_set_width(width);
    }

    fn set_height(&mut self, height: Dimension) {
        self.base_set_height(height);
    }

    fn set_scale(&mut self, scale: f64) {
        self.base_set_scale(scale);
    }

    fn set_id(&mut self, id: &str) {
        self.base_set_id(id);
    }

    fn get_id(&self) -> String {
        self.base_get_id()
    }

    fn get_tooltip(&self) -> Option<String> {
        self.base_get_tooltip()
    }

    fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.base_set_tooltip(tooltip);
    }

    fn get_content_description(&self) -> Option<String> {
        self.base_get_content_description()
    }

    fn set_content_description(&mut self, description: Option<String>) {
        self.base_set_content_description(description);
    }

    fn get_labelled_by(&self) -> Option<String> {
        self.base_get_labelled_by()
    }

    fn set_labelled_by(&mut self, view_id: Option<String>) {
        self.base_set_labelled_by(view_id);
    }

    fn get_background(&self) -> Option<u32> {
        self.base_get_background()
    }

    fn set_background(&mut self, color: Option<u32>) {
        self.base_set_background(color);
    }

    fn get_border_color(&self) -> Option<u32> {
        self.base_get_border_color()
    }

    fn set_border_color(&mut self, color: Option<u32>) {
        self.base_set_border_color(color);
    }

    fn is_enabled(&self) -> bool {
        self.base_is_enabled()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.base_set_enabled(enabled);
    }

    fn get_visibility(&self) -> Visibility {
        self.base_get_visibility()
    }

    fn set_visibility(&mut self, visibility: Visibility) {
        self.base_set_visibility(visibility);
    }

    fn on_event(&mut self, event: EventType, func: EventCallback) {
        self.base_on_event(event, func);
    }

    fn has_listener(&self, event: EventType) -> bool {
        self.base_has_listener(event)
    }

    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool {
        self.base_fire_event(ui, event, data)
    }

    fn accessibility_node(&self) -> accesskit::Node {
        accesskit::Node::new(accesskit::Role::Terminal)
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() {
            return false;
        }
        self.base_fire_event(ui, EventType::Click, &EventData::None)
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: crate::input::MouseButton) -> bool {
        // A click focuses the grid (like an edit field, which sets its own focused flag and lets
        // `sync_focus` reconcile the owner), so the keyboard flows to the terminal.
        let _ = (ui, button);
        if !self.base_is_enabled() || !self.state.borrow().rect.hit((position.x, position.y)) {
            return false;
        }
        self.state.borrow_mut().state.focused = true;
        true
    }
}

impl Default for TermGrid {
    fn default() -> Self {
        let r = rect((0, 0), (200, 100));
        TermGrid::new(r, Dimension::Max, Dimension::Max)
    }
}
