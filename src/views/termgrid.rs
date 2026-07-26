use std::cell::{Cell, RefCell};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::input::{MouseButton, MouseScrollDistance};
use crate::text::{TextBlock, TextOptions};
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
// The default font is resolved per platform: a family the OS does not ship
// leaves the grid with no cell metrics, and it then draws nothing at all.
const DEFAULT_FG: u32 = 0xFFE0E0E0;
/// Default widget background. Dark, to match [`DEFAULT_FG`]; an embedder that wants
/// something else sets the `background` attribute.
const DEFAULT_BACK: u32 = 0xFF000000;
/// Characters measured to size a cell: a capital, an accent and three descenders,
/// so the row is tall enough for anything a terminal is likely to print.
const PROBE: &str = "WÄgjy";
/// Floor for the row pitch, as a multiple of the em size.
const MIN_LINE_RATIO: f32 = 1.2;

/// Style bits of a [`TermCell`] (bitflags in `flags`).
pub const TERM_BOLD: u8 = 1;
pub const TERM_UNDERLINE: u8 = 2;
pub const TERM_INVERSE: u8 = 4;

/// One row's text, already shaped. Shaping dominates the cost of a paint, and a
/// terminal repaints far more often than its rows actually change, so the result
/// is kept until that row's cells, the cursor or the font metrics move.
struct ShapedRun {
    /// Left edge in physical px, relative to the grid's content origin.
    x: i32,
    /// Column just past the run, for the underline's right edge.
    end_col: usize,
    fg: u32,
    flags: u8,
    /// `None` for a run that is only blanks: nothing to draw, but it may still
    /// carry an underline.
    block: Option<TextBlock>,
}

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
    /// The typeface those metrics were measured with. Painting MUST reuse it: the
    /// view inherits its font size from whatever typeface it is resolved against,
    /// and layout resolves against the parent in the view tree while painting
    /// would otherwise resolve against the theme's default — a different size, so
    /// the drawn text would advance by more than a cell and creep out of the grid.
    metrics_face: RefCell<Option<Typeface>>,
    /// Offset of the shaped line inside the cell, and the baseline below the cell
    /// top. Cells are at least [`MIN_LINE_RATIO`] ems tall, so a line usually has
    /// leading to spare; splitting it evenly keeps glyphs centred in the block a
    /// cell background paints, instead of hugging its top edge.
    text_dy: Cell<f32>,
    baseline: Cell<f32>,
    /// Shaped text per row; `None` means "shape it again".
    shaped: RefCell<Vec<Option<Vec<ShapedRun>>>>,
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
        main.font_manager.set_font(crate::themes::default_mono_font_name());
        TermGrid {
            state: RefCell::new(main),
            cols: Cell::new(DEFAULT_COLS),
            rows: Cell::new(DEFAULT_ROWS),
            cells: RefCell::new(vec![TermCell::default(); DEFAULT_COLS * DEFAULT_ROWS]),
            cursor: Cell::new((0, 0, true)),
            cell_w: Cell::new(0.0),
            cell_h: Cell::new(0.0),
            metrics_face: RefCell::new(None),
            text_dy: Cell::new(0.0),
            baseline: Cell::new(0.0),
            shaped: RefCell::new(Vec::new()),
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

    /// The grid cell under a point given in ABSOLUTE window coordinates — the space the
    /// `MouseDown`/`MouseMove`/`MouseUp`/`MouseWheel` payloads use, so a listener can turn an
    /// event straight into (col, row). `None` when the point is outside the grid, before the
    /// first layout, or past the last row/column.
    pub fn cell_at(&self, point: Point<i32>) -> Option<(usize, usize)> {
        let (cw, ch) = (self.cell_w.get(), self.cell_h.get());
        if cw <= 0.0 || ch <= 0.0 {
            return None;
        }
        let origin = self.get_absolute_position();
        let padding = self.state.borrow().padding.scaled(self.state.borrow().scale);
        let dx = point.x - origin.x - padding.left;
        let dy = point.y - origin.y - padding.top;
        if dx < 0 || dy < 0 {
            return None;
        }
        let col = (dx as f32 / cw) as usize;
        let row = (dy as f32 / ch) as usize;
        if col >= self.cols.get() || row >= self.rows.get() {
            return None;
        }
        Some((col, row))
    }

    /// (Re)size the grid, preserving nothing (the embedder repaints in full after a resize).
    pub fn resize(&self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.cols.set(cols);
        self.rows.set(rows);
        *self.cells.borrow_mut() = vec![TermCell::default(); cols * rows];
        self.shaped.borrow_mut().clear();
    }

    /// Move the cursor (cells; clamped at draw time) and set its visibility.
    pub fn set_cursor(&self, col: usize, row: usize, visible: bool) {
        let (_, old_row, _) = self.cursor.get();
        self.cursor.set((col, row, visible));
        // The cursor inverts one cell, so the rows it left and joined must shape
        // again.
        self.invalidate_row(old_row);
        self.invalidate_row(row);
    }

    fn invalidate_row(&self, row: usize) {
        if let Some(slot) = self.shaped.borrow_mut().get_mut(row) {
            *slot = None;
        }
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
        {
            let mut shaped = self.shaped.borrow_mut();
            for row in first_row..first_row + row_count {
                if let Some(slot) = shaped.get_mut(row) {
                    *slot = None;
                }
            }
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
        // Metrics feed straight into shaping, so anything already shaped is stale.
        if self.metrics_face.borrow().as_ref() != Some(&typeface) {
            self.shaped.borrow_mut().clear();
        }
        *self.metrics_face.borrow_mut() = Some(typeface.clone());
        let size = typeface.font_size.unwrap_or(DEFAULT_FONT_SIZE) * scale as f32;
        match get_font_family(&typeface.font_name, typeface.font_style) {
            Some(font) => {
                // Cell width is the step between two shaped glyphs — the very positions
                // the renderer will draw at. Neither `TextBlock::width()` (the ink
                // extent of one glyph) nor the reported `advance_width` can be trusted
                // for this: both disagree with the layout's own step, and a cell that
                // disagrees makes drawn text creep out of its grid by a fraction of a
                // cell per column, leaving the cursor further behind the longer the line.
                let block = font.layout_text("WW", size, TextOptions::new());
                let mut glyphs = block.iter_lines().next().into_iter().flat_map(|l| l.iter_glyphs());
                let step = match (glyphs.next(), glyphs.next()) {
                    (Some(first), Some(second)) => second.position_x() - first.position_x(),
                    (Some(only), None) => only.advance_width(),
                    _ => 0.0,
                };
                self.cell_w
                    .set(if step > 0.0 { step } else { block.width() / 2.0 });

                // Height must be a LINE, not an em. `TextBlock::height()` is normalised
                // to the em size and rows spaced by it clip the tails of g, j and p into
                // the row below. Measure a probe with both extremes, because the software
                // backend reports the extents of the glyphs actually laid out rather than
                // the font's own metrics: "W" alone has no descender and comes back short.
                let probe = font.layout_text(PROBE, size, TextOptions::new());
                let line = probe.iter_lines().next();
                // `descent` is negative, matching speedy2d's convention.
                let ascent = line.map(|l| l.ascent()).unwrap_or(0.0);
                let descent = line.map(|l| l.descent()).unwrap_or(0.0);
                let extents = ascent - descent;
                // Never tighter than a conventional line: glyph extents still miss
                // accents, and a terminal needs the leading to stay readable.
                let cell_h = extents.max(size * MIN_LINE_RATIO).ceil();
                self.cell_h.set(cell_h);
                // Put the baseline low enough that descenders just reach the cell
                // floor, and give the leftover leading to the space above. Splitting
                // it evenly looks top-heavy instead: the descent below the baseline
                // is already empty for capitals and digits, which is most of what a
                // terminal prints, so adding more space below only deepens the gap.
                let baseline = cell_h + descent;
                self.baseline.set(baseline);
                self.text_dy.set((baseline - ascent).max(0.0));
            }
            // Without metrics the grid cannot place a single cell, so it would
            // just stay blank; say why rather than leaving a mystery.
            None => log::warn!(
                "TermGrid: monospace font '{}' is not available, nothing will be drawn",
                typeface.font_name
            ),
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
        let typeface = self
            .metrics_face
            .borrow()
            .clone()
            .unwrap_or_else(|| state.get_typeface(&theme.typeface("default")));
        drop(state);

        theme.push_clip();
        let clip = theme.clip_rect(r);

        // The widget background: an explicit `background` attr, a 9-patch, or [`DEFAULT_BACK`].
        // The default is dark rather than the theme's field colour so that it agrees with
        // [`DEFAULT_FG`]: a light field under light default text shows nothing at all. It also
        // covers the strip left over when the widget is not an exact multiple of the cell size.
        if !self.base_draw_ninepatch(theme, r) {
            let back = self.base_get_background().unwrap_or(DEFAULT_BACK);
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

        let mut shaped = self.shaped.borrow_mut();
        if shaped.len() != rows {
            shaped.clear();
            shaped.resize_with(rows, || None);
        }

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
            // mono metrics keep columns aligned regardless of shaping). Shaping is by far
            // the most expensive part of a paint, so a row's runs are kept and reshaped
            // only once that row, the cursor or the metrics change.
            if shaped[row].is_none() {
                let mut runs = Vec::new();
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
                    let font =
                        if flags & TERM_BOLD != 0 { bold.as_ref() } else { regular.as_ref() };
                    let block = match (text.trim_end().is_empty(), font) {
                        (false, Some(font)) => {
                            // NO trim_each_line: a run may start with real terminal cells that
                            // are spaces (indented output) — trimming would shift the visible
                            // text left of its grid columns.
                            let opts = TextOptions::new().with_trim_each_line(false);
                            Some(font.layout_text(&text, size, opts))
                        }
                        _ => None,
                    };
                    runs.push(ShapedRun {
                        x: (start as f32 * cw).round() as i32,
                        end_col: col,
                        fg,
                        flags,
                        block,
                    });
                }
                shaped[row] = Some(runs);
            }

            for run in shaped[row].as_ref().expect("just shaped") {
                let rx = x0 + run.x;
                if let Some(block) = &run.block {
                    theme.draw_text(rx as f32, ry as f32 + self.text_dy.get(), run.fg, block);
                }
                if run.flags & TERM_UNDERLINE != 0 {
                    let uy = ry + (self.baseline.get() + scale.max(1.0) as f32) as i32;
                    let ux1 = x0 + (run.end_col as f32 * cw).round() as i32;
                    theme.draw_rect(rect((rx, uy), (ux1, uy + scale.max(1.0) as i32)), run.fg);
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

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        // A click focuses the grid (like an edit field, which sets its own focused flag and lets
        // `sync_focus` reconcile the owner), so the keyboard flows to the terminal.
        if !self.base_is_enabled() || !self.state.borrow().rect.hit((position.x, position.y)) {
            return false;
        }
        self.state.borrow_mut().state.focused = true;
        let pos = ui.get_mouse_pos();
        self.base_fire_event(ui, EventType::MouseDown, &EventData::Mouse { x: pos.x, y: pos.y, button });
        true
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        if !self.base_is_enabled() {
            return false;
        }
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        let old_state = self.state.borrow().state;
        self.state.borrow_mut().state.hovered = hit;
        let changed = self.state.borrow().state != old_state;
        // Fired even when the pointer is outside the grid: a selection drag that started inside
        // must keep receiving moves after the pointer leaves. `cell_at` returns None out there,
        // so a listener can clamp however it likes.
        let pos = ui.get_mouse_pos();
        let fired = self.base_fire_event(ui, EventType::MouseMove, &EventData::Position { x: pos.x, y: pos.y });
        changed || fired
    }

    fn on_mouse_button_up(&self, ui: &mut UI, _position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() {
            return false;
        }
        // Like `on_mouse_move`, not gated on a hit test — the release that ends a drag often
        // lands outside the grid.
        let pos = ui.get_mouse_pos();
        self.base_fire_event(ui, EventType::MouseUp, &EventData::Mouse { x: pos.x, y: pos.y, button })
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        if !self.base_is_enabled() || !self.state.borrow().rect.hit((position.x, position.y)) {
            return false;
        }
        let pos = ui.get_mouse_pos();
        self.base_fire_event(ui, EventType::MouseWheel, &EventData::Wheel { x: pos.x, y: pos.y, distance })
    }
}

impl Default for TermGrid {
    fn default() -> Self {
        let r = rect((0, 0), (200, 100));
        TermGrid::new(r, Dimension::Max, Dimension::Max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{ModifiersState, VirtualKeyCode};
    use crate::themes::Typeface;
    use crate::traits::Element;
    use std::rc::Rc;

    const XML: &str = r#"
    <Frame id="root" width="max" height="max" direction="vertical">
        <TermGrid id="term" width="max" height="max" cols="20" rows="10"/>
    </Frame>"#;

    /// A 20x10 grid laid out at (0,0) with cell metrics pinned to 10x20 px, so the
    /// coordinate maths is exact whether or not the environment has fonts to
    /// measure a real glyph with.
    fn grid_ui() -> (UI, Element) {
        let ui = UI::from_xml(XML, 400, 200, Typeface::default(), 1.0).unwrap();
        let el = ui.get_view("term").unwrap();
        el.borrow_mut().layout_content(0, 0, 400, 200, &Typeface::default(), 1.0);
        let view = el.borrow();
        let grid = view.as_any().downcast_ref::<TermGrid>().unwrap();
        grid.cell_w.set(10.0);
        grid.cell_h.set(20.0);
        drop(view);
        (ui, el)
    }

    fn with_grid<R>(el: &Element, f: impl FnOnce(&TermGrid) -> R) -> R {
        let view = el.borrow();
        f(view.as_any().downcast_ref::<TermGrid>().unwrap())
    }

    /// Whatever the text backend, a cell must be exactly the step the shaper puts
    /// between two glyphs. When it was not, drawn text crept out of its own grid by
    /// a fraction of a cell per column and the cursor block fell behind the text.
    #[test]
    fn a_run_of_glyphs_spans_exactly_that_many_cells() {
        let grid = TermGrid::default();
        let typeface = Typeface::default();
        grid.measure(&typeface, 1.0);
        let (cell_w, cell_h) = grid.cell_size();
        assert!(cell_w > 0.0 && cell_h > 0.0, "no metrics: is a monospace font installed?");

        let face = grid.state.borrow().get_typeface(&typeface);
        let size = face.font_size.unwrap_or(DEFAULT_FONT_SIZE);
        let font = get_font_family(&face.font_name, face.font_style).unwrap();
        let run = font.layout_text(&"W".repeat(20), size, TextOptions::new());
        let last = run
            .iter_lines()
            .next()
            .and_then(|line| line.iter_glyphs().last())
            .expect("shaped glyphs");
        // The 20th glyph starts 19 cells in; anything else and the grid drifts.
        let expected = cell_w * 19.0;
        assert!(
            (last.position_x() - expected).abs() < 0.5,
            "glyph 20 sits at {} but 19 cells span {expected}",
            last.position_x()
        );
    }

    #[test]
    fn cell_at_maps_window_pixels_to_cells() {
        let (_ui, el) = grid_ui();
        with_grid(&el, |g| {
            assert_eq!(g.cell_at(Point::new(0, 0)), Some((0, 0)));
            assert_eq!(g.cell_at(Point::new(9, 19)), Some((0, 0)));
            assert_eq!(g.cell_at(Point::new(10, 20)), Some((1, 1)));
            assert_eq!(g.cell_at(Point::new(195, 199)), Some((19, 9)));
            // Past the last column/row, and left/above the origin.
            assert_eq!(g.cell_at(Point::new(200, 0)), None);
            assert_eq!(g.cell_at(Point::new(0, 200)), None);
            assert_eq!(g.cell_at(Point::new(-1, 0)), None);
        });
    }

    #[test]
    fn cell_at_is_none_before_the_first_layout() {
        let grid = TermGrid::default();
        assert_eq!(grid.cell_at(Point::new(0, 0)), None);
    }

    #[test]
    fn mouse_down_reports_button_and_absolute_position() {
        let (mut ui, el) = grid_ui();
        let seen: Rc<Cell<Option<(i32, i32, MouseButton)>>> = Rc::new(Cell::new(None));
        let sink = Rc::clone(&seen);
        el.borrow_mut().on_event(
            EventType::MouseDown,
            Box::new(move |_ui, _view, data| {
                if let EventData::Mouse { x, y, button } = data {
                    sink.set(Some((*x, *y, *button)));
                }
                true
            }),
        );

        // `on_mouse_move` is what records the pointer the payload reports.
        ui.on_mouse_move(Point::new(35, 45));
        assert!(el.borrow().on_mouse_button_down(&mut ui, Point::new(35, 45), MouseButton::Right));

        assert_eq!(seen.get(), Some((35, 45, MouseButton::Right)));
        // The reported point is what an embedder feeds straight back to `cell_at`.
        with_grid(&el, |g| assert_eq!(g.cell_at(Point::new(35, 45)), Some((3, 2))));
    }

    #[test]
    fn focused_grid_receives_layout_characters() {
        let (mut ui, el) = grid_ui();
        let typed: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let sink = Rc::clone(&typed);
        el.borrow_mut().on_event(
            EventType::KeyChar,
            Box::new(move |_ui, _view, data| {
                if let EventData::Char { ch, .. } = data {
                    sink.borrow_mut().push(*ch);
                }
                true
            }),
        );
        assert!(ui.set_focus_to(&el));

        // A virtual key code could never carry these: they are what the layout produced.
        for ch in "щцD".chars() {
            assert!(ui.on_key_char(ch, ModifiersState::default()));
        }
        assert_eq!(typed.borrow().as_str(), "щцD");
    }

    #[test]
    fn a_held_key_repeats_to_the_listener() {
        let (mut ui, el) = grid_ui();
        let count = Rc::new(Cell::new(0u32));
        let sink = Rc::clone(&count);
        el.borrow_mut().on_event(
            EventType::KeyDown,
            Box::new(move |_ui, _view, _data| {
                sink.set(sink.get() + 1);
                true
            }),
        );
        assert!(ui.set_focus_to(&el));

        ui.on_key_down(Some(VirtualKeyCode::Backspace), 0, ModifiersState::default());
        for _ in 0..4 {
            ui.on_key_repeat(Some(VirtualKeyCode::Backspace), ModifiersState::default());
        }
        assert_eq!(
            count.get(),
            5,
            "a held Backspace must keep reaching the listener, or a terminal cannot erase a line"
        );
    }

    /// The other half of the bargain: repeats must not drive built-in widget
    /// behaviour, or a held Enter would re-click a button and a held Tab would
    /// race through the focus order.
    #[test]
    fn a_held_key_does_not_move_focus() {
        const XML: &str = r#"
        <Frame id="root" width="max" height="max" direction="vertical">
            <Button id="one" text="One"/>
            <Button id="two" text="Two"/>
        </Frame>"#;
        let mut ui = UI::from_xml(XML, 200, 100, Typeface::default(), 1.0).unwrap();
        let one = ui.get_view("one").unwrap();
        let two = ui.get_view("two").unwrap();
        assert!(ui.set_focus_to(&one));

        ui.on_key_repeat(Some(VirtualKeyCode::Tab), ModifiersState::default());
        assert!(one.borrow().is_focused(), "a repeat must not traverse focus");

        ui.on_key_down(Some(VirtualKeyCode::Tab), 0, ModifiersState::default());
        assert!(two.borrow().is_focused(), "a real press still traverses focus");
    }

    #[test]
    fn wheel_outside_the_grid_is_ignored() {
        let (mut ui, el) = grid_ui();
        let fired = Rc::new(Cell::new(0usize));
        let sink = Rc::clone(&fired);
        el.borrow_mut().on_event(
            EventType::MouseWheel,
            Box::new(move |_ui, _view, _data| {
                sink.set(sink.get() + 1);
                true
            }),
        );

        let lines = MouseScrollDistance::Lines { x: 0.0, y: -1.0, z: 0.0 };
        assert!(el.borrow().on_mouse_wheel_scroll(&mut ui, Point::new(50, 50), lines));
        assert_eq!(fired.get(), 1);
        // Below the widget's rect: not ours to handle.
        assert!(!el.borrow().on_mouse_wheel_scroll(&mut ui, Point::new(50, 500), lines));
        assert_eq!(fired.get(), 1);
    }
}
