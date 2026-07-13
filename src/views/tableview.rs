use std::cell::{Cell, RefCell};
use std::cmp::min;

use crate::text::{TextBlock, TextOptions};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Container, Element, View, WeakElement};
use crate::types::{Point, Rect, point, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::label::Label;
use crate::views::{Borders, Dimension, FieldsMain, Gravity, HAlign, Visibility};


const MIN_THUMB_SIZE: i32 = 16;
const DEFAULT_MIN_COL_WIDTH: i32 = 32;
/// Distance in dip between the grid's outer border and the inner content
/// (header/body/scrollbars). Two dip lets the 2-pixel `edit.body`
/// border sit cleanly outside the chrome instead of overlapping it.
const BORDER_INSET_DIP: i32 = 2;
// Padding in dip. Scaled at layout time. Kept small so the row stays tight
// against the cell text (which renders at scaled font size when we set
// `font_size` on auto-Labels — see `add_row_text`).
const DEFAULT_HEADER_PAD_V: i32 = 3;
const DEFAULT_ROW_PAD_V: i32 = 2;
const DEFAULT_CELL_PAD_H: i32 = 6;
const DIVIDER_GRAB_DIP: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    /// Fixed width in dip.
    Fixed(i32),
    /// Share of remaining space; columns with `Star(s)` divide leftover space proportionally.
    Star(f32),
}

impl Default for ColumnWidth {
    fn default() -> Self { ColumnWidth::Star(1.0) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection { Asc, Desc }

#[derive(Clone, Debug)]
pub struct ColumnDef {
    pub title: String,
    pub width: ColumnWidth,
    /// Resolved width in physical pixels at the current scale; written by layout.
    pub current_width_px: i32,
    /// Set true once the user drags this column's divider; suppresses Star
    /// redistribution so a manual resize sticks.
    pub user_sized: bool,
    pub sortable: bool,
    pub resizable: bool,
    pub align: HAlign,
    /// Minimum width in dip.
    pub min_width_dip: i32,
}

impl Default for ColumnDef {
    fn default() -> Self {
        ColumnDef {
            title: String::new(),
            width: ColumnWidth::Star(1.0),
            current_width_px: 0,
            user_sized: false,
            sortable: true,
            resizable: true,
            align: HAlign::Left,
            min_width_dip: DEFAULT_MIN_COL_WIDTH,
        }
    }
}

pub(crate) fn parse_column_width(s: &str) -> Option<ColumnWidth> {
    let s = s.trim();
    if s == "*" { return Some(ColumnWidth::Star(1.0)); }
    if let Some(stripped) = s.strip_suffix('*') {
        if stripped.is_empty() { return Some(ColumnWidth::Star(1.0)); }
        if let Ok(v) = stripped.parse::<f32>() { return Some(ColumnWidth::Star(v.max(0.0))); }
        return None;
    }
    if let Ok(v) = s.parse::<i32>() {
        return Some(ColumnWidth::Fixed(v.max(0)));
    }
    None
}

fn parse_h_align(s: &str) -> HAlign {
    match s.trim() {
        "right" => HAlign::Right,
        "center" => HAlign::Center,
        _ => HAlign::Left,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind { None, ResizeColumn, DragVThumb, DragHThumb }

/// A scrollable, sortable data table with a sticky header and resizable
/// columns. Supports vertical and horizontal scrolling, single-row selection,
/// click-to-sort headers, and drag-to-resize column dividers. Rows share a
/// uniform height.
///
/// There are two ways to fill it:
/// - **Text mode** — [`set_data`](TableView::set_data) /
///   [`add_row_text`](TableView::add_row_text) take plain strings and the
///   table builds single-line `Label` cells for you.
/// - **View mode** — [`add_row`](TableView::add_row) /
///   [`set_cell`](TableView::set_cell) take arbitrary cell `Element`s.
///   Populate the sort-key cache with
///   [`set_cell_text`](TableView::set_cell_text) for columns that should sort.
///
/// XML tags: `<TableView>` with `<TableColumn>` column defs and (view mode)
/// `<TableRow>` rows. Columns are sized in dip or `*` star units — see
/// [`ColumnWidth`].
pub struct TableView {
    state: RefCell<FieldsMain>,

    columns: RefCell<Vec<ColumnDef>>,
    column_offsets: RefCell<Vec<i32>>,    // cumulative left edge in px, len = columns + 1

    rows: RefCell<Vec<Vec<Element>>>,
    row_text: RefCell<Vec<Vec<String>>>,  // sort-key cache; same shape as rows
    row_height_px: Cell<i32>,             // uniform v1
    row_count: Cell<usize>,
    row_offsets: RefCell<Vec<i32>>,       // cumulative top edges in px, len = rows + 1

    sort_state: Cell<Option<(usize, SortDirection)>>,
    display_order: RefCell<Vec<usize>>,

    selected_raw: Cell<Option<usize>>,    // raw row index (survives sort changes)

    scroll_x: Cell<i32>,                  // <= 0
    scroll_y: Cell<i32>,                  // <= 0
    h_scroll_visible: Cell<bool>,
    v_scroll_visible: Cell<bool>,

    header_blocks: RefCell<Vec<Option<TextBlock>>>,
    header_height_px: Cell<i32>,

    // Defaults applied to columns added later (and to any column attribute the
    // column doesn't override locally).
    default_sortable: Cell<bool>,
    default_resizable: Cell<bool>,
    explicit_header_height_dip: Cell<Option<i32>>,

    drag: Cell<DragKind>,
    drag_col: Cell<usize>,
    drag_anchor_x: Cell<i32>,
    drag_anchor_y: Cell<i32>,
    drag_anchor_width: Cell<i32>,
    drag_anchor_scroll: Cell<i32>,

    header_press: Cell<Option<usize>>,
    header_hover: Cell<Option<usize>>,

    needs_relayout: Cell<bool>,
}

impl HasMainFields for TableView {
    fn main_fields(&self) -> &RefCell<FieldsMain> { &self.state }
}
impl ViewBasics for TableView {}

#[allow(dead_code)]
impl TableView {
    /// Create an empty table with the given bounds and no columns or rows.
    pub fn new(rect_: Rect<i32>) -> TableView {
        let mut main = FieldsMain::with_rect(rect_, Dimension::Min, Dimension::Min);
        main.state.focusable = true;
        TableView {
            state: RefCell::new(main),
            columns: RefCell::new(Vec::new()),
            column_offsets: RefCell::new(vec![0]),
            rows: RefCell::new(Vec::new()),
            row_text: RefCell::new(Vec::new()),
            row_height_px: Cell::new(0),
            row_count: Cell::new(0),
            row_offsets: RefCell::new(vec![0]),
            sort_state: Cell::new(None),
            display_order: RefCell::new(Vec::new()),
            selected_raw: Cell::new(None),
            scroll_x: Cell::new(0),
            scroll_y: Cell::new(0),
            h_scroll_visible: Cell::new(false),
            v_scroll_visible: Cell::new(false),
            header_blocks: RefCell::new(Vec::new()),
            header_height_px: Cell::new(0),
            default_sortable: Cell::new(true),
            default_resizable: Cell::new(true),
            explicit_header_height_dip: Cell::new(None),
            drag: Cell::new(DragKind::None),
            drag_col: Cell::new(0),
            drag_anchor_x: Cell::new(0),
            drag_anchor_y: Cell::new(0),
            drag_anchor_width: Cell::new(0),
            drag_anchor_scroll: Cell::new(0),
            header_press: Cell::new(None),
            header_hover: Cell::new(None),
            needs_relayout: Cell::new(false),
        }
    }

    // --- Public API ---

    /// Replace all column definitions.
    pub fn set_columns(&self, defs: Vec<ColumnDef>) {
        *self.columns.borrow_mut() = defs;
        self.needs_relayout.set(true);
    }

    /// Append one column definition.
    pub fn add_column(&self, def: ColumnDef) {
        self.columns.borrow_mut().push(def);
        self.needs_relayout.set(true);
    }

    /// A clone of the current column definitions, including each column's
    /// resolved pixel width.
    pub fn columns(&self) -> Vec<ColumnDef> {
        self.columns.borrow().clone()
    }

    /// Override one column's width, clearing any user drag-resize on it.
    pub fn set_column_width(&self, col: usize, width: ColumnWidth) {
        if let Some(c) = self.columns.borrow_mut().get_mut(col) {
            c.width = width;
            c.user_sized = false;
        }
        self.needs_relayout.set(true);
    }

    /// Fill the table from headers plus string rows (text mode). Existing
    /// column defs are reused where present — preserving user-sized widths and
    /// explicit `<TableColumn>` declarations — and missing slots are
    /// synthesized; any existing rows are cleared first.
    pub fn set_data(&self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        // Reuse existing column defs where possible (preserves user-sized widths
        // and explicit Column declarations); only synthesize missing slots.
        let mut cols = self.columns.borrow().clone();
        for (i, title) in headers.iter().enumerate() {
            if i < cols.len() {
                cols[i].title = title.clone();
            } else {
                cols.push(ColumnDef {
                    title: title.clone(),
                    sortable: self.default_sortable.get(),
                    resizable: self.default_resizable.get(),
                    ..ColumnDef::default()
                });
            }
        }
        cols.truncate(headers.len().max(cols.len()));
        *self.columns.borrow_mut() = cols;

        self.clear_rows();
        for r in rows {
            self.add_row_text(r);
        }
        self.needs_relayout.set(true);
    }

    /// Append one text-mode row; each string becomes a single-line `Label`
    /// cell.
    pub fn add_row_text(&self, cells: Vec<String>) {
        let row_idx = self.row_text.borrow().len();
        self.row_text.borrow_mut().push(cells.clone());
        // Materialize Label children so cell painting goes through the standard
        // View::paint path — text mode and view mode share one rendering route.
        // Auto-cells are single-line: wrapping inside a uniform-height row would
        // visually overflow into adjacent rows, and standard table semantics
        // truncate rather than wrap.
        let mut row_views: Vec<Element> = Vec::with_capacity(cells.len());
        let font_size_str = format!("{}", crate::drawing::current_text_size("text") as i32);
        for text in &cells {
            let mut lbl = Label::new(rect((0, 0), (0, 0)), text, crate::drawing::current_text_size("text"));
            lbl.set_padding(0, DEFAULT_CELL_PAD_H, DEFAULT_CELL_PAD_H, 0);
            lbl.set_single_line(true);
            // Push the font size through Label's `set_any` so it routes through
            // FontManager. Label's `rebuild_text` only scales by `scale` when
            // `typeface.font_size` is Some — without this it falls back to
            // the raw `text_size` field and renders tiny text at HiDPI.
            lbl.set_any("font_size", &font_size_str);
            row_views.push(std::rc::Rc::new(RefCell::new(lbl)));
        }
        self.rows.borrow_mut().push(row_views);
        self.display_order.borrow_mut().push(row_idx);
        self.row_count.set(self.row_text.borrow().len());
        self.needs_relayout.set(true);
    }

    /// Append one view-mode row of arbitrary cell `Element`s. Sort keys start
    /// empty — call [`set_cell_text`](TableView::set_cell_text) per cell if
    /// those columns should sort.
    pub fn add_row(&self, cells: Vec<Element>) {
        let row_idx = self.rows.borrow().len();
        // Sort-key cache starts empty for view-mode rows; callers can populate
        // it per-cell with `set_cell_text` if they want sortable columns.
        let texts: Vec<String> = (0..cells.len()).map(|_| String::new()).collect();
        self.row_text.borrow_mut().push(texts);
        self.rows.borrow_mut().push(cells);
        self.display_order.borrow_mut().push(row_idx);
        self.row_count.set(self.rows.borrow().len());
        self.needs_relayout.set(true);
    }

    /// Replace the cell view at `(row, col)`, growing the row with blank cells
    /// if it is shorter than `col`.
    pub fn set_cell(&self, row: usize, col: usize, cell: Element) {
        if let Some(r) = self.rows.borrow_mut().get_mut(row) {
            while r.len() <= col { r.push(std::rc::Rc::new(RefCell::new(Label::new(rect((0,0),(0,0)), "", crate::drawing::current_text_size("text"))))); }
            r[col] = cell;
        }
        self.needs_relayout.set(true);
    }

    /// Set the sort-key text for the cell at `(row, col)`. This feeds sorting
    /// only; it does not change what a view-mode cell displays.
    pub fn set_cell_text(&self, row: usize, col: usize, text: &str) {
        if let Some(r) = self.row_text.borrow_mut().get_mut(row) {
            while r.len() <= col { r.push(String::new()); }
            r[col] = text.to_owned();
        }
        self.needs_relayout.set(true);
    }

    /// Remove all rows and clear the selection. Columns are kept.
    pub fn clear_rows(&self) {
        self.rows.borrow_mut().clear();
        self.row_text.borrow_mut().clear();
        self.display_order.borrow_mut().clear();
        self.row_count.set(0);
        self.selected_raw.set(None);
        self.needs_relayout.set(true);
    }

    /// Number of rows.
    pub fn row_count(&self) -> usize { self.row_count.get() }

    /// The selected row as a raw index (stable across sorting), or `None`.
    pub fn selected_row(&self) -> Option<usize> { self.selected_raw.get() }

    /// Select a row by raw index. Out-of-range indices are ignored.
    pub fn select_row(&self, raw_row: usize) {
        if raw_row < self.row_count.get() {
            self.selected_raw.set(Some(raw_row));
        }
    }

    /// Clear the row selection.
    pub fn clear_selection(&self) { self.selected_raw.set(None); }

    /// Sort by `col` in the given direction and rebuild the display order.
    /// Ignored if `col` is out of range.
    pub fn set_sort(&self, col: usize, dir: SortDirection) {
        if col < self.columns.borrow().len() {
            self.sort_state.set(Some((col, dir)));
            self.rebuild_display_order();
            self.needs_relayout.set(true);
        }
    }

    /// Remove sorting, restoring insertion order.
    pub fn clear_sort(&self) {
        self.sort_state.set(None);
        self.rebuild_display_order();
        self.needs_relayout.set(true);
    }

    /// The active sort as `(column, direction)`, or `None` when unsorted.
    pub fn sort_state(&self) -> Option<(usize, SortDirection)> { self.sort_state.get() }

    /// Scroll so the content offset is `(x, y)`. Offsets are non-positive
    /// (content moves up/left) and clamped to the scrollable range.
    pub fn scroll_to(&self, x: i32, y: i32) {
        self.scroll_x.set(x.min(0));
        self.scroll_y.set(y.min(0));
        self.clamp_scroll();
    }

    /// Scroll vertically so the given raw row is brought into the body view.
    pub fn scroll_to_row(&self, raw_row: usize) {
        let display_idx = self.raw_to_display(raw_row);
        if let Some(d) = display_idx {
            let row_h = self.row_height_px.get().max(1);
            let y = d as i32 * row_h;
            // Bring [y .. y + row_h] inside the body viewport.
            let body_h = self.body_height();
            let cur = self.scroll_y.get();
            let top_in_view = y + cur;
            let bot_in_view = top_in_view + row_h;
            if top_in_view < 0 {
                self.scroll_y.set(-y);
            } else if bot_in_view > body_h {
                self.scroll_y.set(body_h - (y + row_h));
            }
            self.clamp_scroll();
        }
    }

    // --- Internals ---

    fn rebuild_display_order(&self) {
        let n = self.row_count.get();
        let mut order: Vec<usize> = (0..n).collect();
        if let Some((col, dir)) = self.sort_state.get() {
            let texts = self.row_text.borrow();
            order.sort_by(|&a, &b| {
                let sa = texts.get(a).and_then(|r| r.get(col)).map(|s| s.as_str()).unwrap_or("");
                let sb = texts.get(b).and_then(|r| r.get(col)).map(|s| s.as_str()).unwrap_or("");
                let ord = sa.to_lowercase().cmp(&sb.to_lowercase());
                if dir == SortDirection::Desc { ord.reverse() } else { ord }
            });
        }
        *self.display_order.borrow_mut() = order;
    }

    fn raw_to_display(&self, raw: usize) -> Option<usize> {
        self.display_order.borrow().iter().position(|&r| r == raw)
    }

    fn relayout_columns(&self, available_px: i32, scale: f64) {
        let mut cols = self.columns.borrow_mut();
        let min_clamps: Vec<i32> = cols.iter()
            .map(|c| (c.min_width_dip as f64 * scale).round() as i32)
            .collect();

        // Pass 1: assign Fixed and user-sized.
        let mut sum_fixed = 0i32;
        let mut star_indices: Vec<usize> = Vec::new();
        let mut star_total: f32 = 0.0;
        for (i, c) in cols.iter_mut().enumerate() {
            if c.user_sized {
                c.current_width_px = c.current_width_px.max(min_clamps[i]);
                sum_fixed += c.current_width_px;
            } else {
                match c.width {
                    ColumnWidth::Fixed(d) => {
                        let w = ((d as f64 * scale).round() as i32).max(min_clamps[i]);
                        c.current_width_px = w;
                        sum_fixed += w;
                    }
                    ColumnWidth::Star(s) => {
                        let s = s.max(0.0001);
                        star_indices.push(i);
                        star_total += s;
                    }
                }
            }
        }

        // Pass 2: distribute remainder.
        let remainder = (available_px - sum_fixed).max(0);
        if !star_indices.is_empty() && star_total > 0.0 {
            let mut leftover = remainder;
            for (k, &i) in star_indices.iter().enumerate() {
                let s = match cols[i].width { ColumnWidth::Star(s) => s.max(0.0001), _ => 1.0 };
                let share = if k + 1 == star_indices.len() {
                    leftover
                } else {
                    ((s / star_total) * remainder as f32).round() as i32
                };
                let w = share.max(min_clamps[i]);
                cols[i].current_width_px = w;
                leftover -= w;
            }
        } else {
            // No star columns: any remainder is unused space (will leave a trailing
            // gap if columns don't fill, or trigger H scrollbar if they overflow).
        }

        // Rebuild column_offsets.
        let mut offs = self.column_offsets.borrow_mut();
        offs.clear();
        offs.push(0);
        let mut acc = 0i32;
        for c in cols.iter() {
            acc += c.current_width_px;
            offs.push(acc);
        }
    }

    fn relayout_rows(&self, scale: f64, font: &Typeface) {
        let n = self.row_count.get();
        let base_size = font.font_size.unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let line_h = (base_size * scale as f32).ceil() as i32;
        let row_h = line_h + (DEFAULT_ROW_PAD_V as f64 * scale).round() as i32 * 2;
        self.row_height_px.set(row_h);

        let mut offs = self.row_offsets.borrow_mut();
        offs.clear();
        offs.push(0);
        let mut acc = 0i32;
        for _ in 0..n {
            acc += row_h;
            offs.push(acc);
        }
    }

    /// Recompute everything that depends on the current rect — column widths,
    /// row heights, scrollbar visibility, header text, and per-cell layout.
    /// Does NOT touch the grid's own rect, so it's safe to call from `update`
    /// (where re-running `layout_content` with the cached rect would re-apply
    /// `Dimension::Percent` and shrink the grid each tick).
    fn inner_relayout(&self, scale: f64, font: &Typeface) {
        self.resolve_header_height(scale, font);

        // Two-pass scrollbar visibility: layout once at full width, check if
        // scrollbars are needed, then relayout against the reduced body width.
        let r = self.get_rect();
        self.relayout_columns(r.width().max(0), scale);
        self.relayout_rows(scale, font);
        self.resolve_scrollbar_visibility();

        let final_avail = self.body_width();
        self.relayout_columns(final_avail, scale);
        self.relayout_rows(scale, font);
        self.resolve_scrollbar_visibility();

        self.rebuild_header_blocks(scale, font);
        self.layout_cells(scale, font);

        if self.display_order.borrow().len() != self.row_count.get() {
            self.rebuild_display_order();
        }
        self.clamp_scroll();
    }

    fn rebuild_header_blocks(&self, scale: f64, font: &Typeface) {
        let cols = self.columns.borrow();
        let base_size = font.font_size.unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let size = base_size * scale as f32;
        let mut header_blocks = self.header_blocks.borrow_mut();
        header_blocks.clear();
        if let Some(font_family) = get_font_family(&font.font_name, font.font_style) {
            for c in cols.iter() {
                let block = font_family.layout_text(&c.title, size, TextOptions::new());
                header_blocks.push(Some(block));
            }
        } else {
            for _ in cols.iter() { header_blocks.push(None); }
        }
    }

    /// Lay out each cell view at (0,0)..(cell_w, row_h). Origin is then
    /// applied during paint to position the cell on screen — that lets us
    /// scroll without rerunning layout. Label cells re-layout automatically
    /// when their dimensions change (Label tracks last layout params).
    fn layout_cells(&self, scale: f64, font: &Typeface) {
        let cols = self.columns.borrow();
        let row_h = self.row_height_px.get();
        let rows = self.rows.borrow();
        for row in rows.iter() {
            for (c, cell) in row.iter().enumerate() {
                let cell_w = cols.get(c).map(|cc| cc.current_width_px).unwrap_or(0);
                cell.borrow_mut().layout_content(0, 0, cell_w, row_h, font, scale);
            }
        }
    }

    fn resolve_header_height(&self, scale: f64, font: &Typeface) {
        if let Some(dip) = self.explicit_header_height_dip.get() {
            self.header_height_px.set((dip as f64 * scale).round() as i32);
            return;
        }
        let base = font.font_size.unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let line = (base * scale as f32).ceil() as i32;
        let pad = (DEFAULT_HEADER_PAD_V as f64 * scale).round() as i32 * 2;
        self.header_height_px.set(line + pad);
    }

    fn border_inset(&self) -> i32 {
        let scale = self.state.borrow().scale;
        (BORDER_INSET_DIP as f64 * scale).round() as i32
    }

    fn body_width(&self) -> i32 {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let mut w = r.width() - 2 * inset;
        if self.v_scroll_visible.get() { w -= self.scrollbar_thickness(); }
        w.max(0)
    }

    fn body_height(&self) -> i32 {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let mut h = r.height() - 2 * inset - self.header_height_px.get();
        if self.h_scroll_visible.get() { h -= self.scrollbar_thickness(); }
        h.max(0)
    }

    fn content_width(&self) -> i32 {
        *self.column_offsets.borrow().last().unwrap_or(&0)
    }

    fn content_height(&self) -> i32 {
        *self.row_offsets.borrow().last().unwrap_or(&0)
    }

    fn scrollbar_thickness(&self) -> i32 {
        let scale = self.state.borrow().scale;
        (crate::drawing::current_dimension("scrollbar.thickness") as f64 * scale).round() as i32
    }

    fn resolve_scrollbar_visibility(&self) {
        // Cap at one iteration: if vertical is needed, body width shrinks and
        // can push horizontal into needed; recompute horizontal once.
        let r = self.state.borrow().rect;
        let cw = self.content_width();
        let ch = self.content_height();
        let thickness = self.scrollbar_thickness();
        let header_h = self.header_height_px.get();

        let mut v = ch > (r.height() - header_h).max(0);
        let body_w_with_v = if v { (r.width() - thickness).max(0) } else { r.width() };
        let h = cw > body_w_with_v;
        let body_h_with_h = if h { (r.height() - header_h - thickness).max(0) } else { (r.height() - header_h).max(0) };
        // Recheck v with the now-known h.
        v = ch > body_h_with_h;

        self.v_scroll_visible.set(v);
        self.h_scroll_visible.set(h);
    }

    fn clamp_scroll(&self) {
        let bw = self.body_width();
        let bh = self.body_height();
        let cw = self.content_width();
        let ch = self.content_height();
        let max_neg_x = -(cw - bw).max(0);
        let max_neg_y = -(ch - bh).max(0);
        let mut x = self.scroll_x.get();
        let mut y = self.scroll_y.get();
        if x < max_neg_x { x = max_neg_x; }
        if x > 0 { x = 0; }
        if y < max_neg_y { y = max_neg_y; }
        if y > 0 { y = 0; }
        self.scroll_x.set(x);
        self.scroll_y.set(y);
    }

    fn body_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let header_h = self.header_height_px.get();
        let bw = self.body_width();
        let bh = self.body_height();
        let x0 = r.min.x + origin.x;
        let y0 = r.min.y + origin.y + header_h;
        rect((x0, y0), (x0 + bw, y0 + bh))
    }

    fn header_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let header_h = self.header_height_px.get();
        let bw = self.body_width();
        let x0 = r.min.x + origin.x;
        let y0 = r.min.y + origin.y;
        rect((x0, y0), (x0 + bw, y0 + header_h))
    }

    /// Full V scrollbar rect (arrows + track). Spans the entire inner height
    /// (top of grid to body bottom, minus the H scrollbar's footprint when
    /// visible), inset from the outer border.
    fn v_scrollbar_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let thickness = self.scrollbar_thickness();
        let h_thick = if self.h_scroll_visible.get() { thickness } else { 0 };
        let x_max = r.min.x + origin.x + r.width() - inset;
        let x_min = x_max - thickness;
        let y_min = r.min.y + origin.y + inset;
        let y_max = r.min.y + origin.y + r.height() - inset - h_thick;
        rect((x_min, y_min), (x_max, y_max))
    }

    /// Full H scrollbar rect (arrows + track). Spans the entire inner width,
    /// inset from the outer border.
    fn h_scrollbar_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let thickness = self.scrollbar_thickness();
        let v_thick = if self.v_scroll_visible.get() { thickness } else { 0 };
        let x_min = r.min.x + origin.x + inset;
        let x_max = r.min.x + origin.x + r.width() - inset - v_thick;
        let y_max = r.min.y + origin.y + r.height() - inset;
        let y_min = y_max - thickness;
        rect((x_min, y_min), (x_max, y_max))
    }

    fn v_arrow_top_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        rect((sb.min.x, sb.min.y), (sb.max.x, sb.min.y + size))
    }

    fn v_arrow_bottom_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        rect((sb.min.x, sb.max.y - size), (sb.max.x, sb.max.y))
    }

    fn h_arrow_left_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.h_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        rect((sb.min.x, sb.min.y), (sb.min.x + size, sb.max.y))
    }

    fn h_arrow_right_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.h_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        rect((sb.max.x - size, sb.min.y), (sb.max.x, sb.max.y))
    }

    /// V scrollbar track: the area between the top and bottom arrow buttons
    /// where the thumb travels. Returns an empty rect when there's not enough
    /// space for both arrows.
    fn v_track_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        if sb.height() < 2 * size {
            return rect((sb.min.x, sb.min.y), (sb.max.x, sb.min.y));
        }
        rect((sb.min.x, sb.min.y + size), (sb.max.x, sb.max.y - size))
    }

    fn h_track_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.h_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        if sb.width() < 2 * size {
            return rect((sb.min.x, sb.min.y), (sb.min.x, sb.max.y));
        }
        rect((sb.min.x + size, sb.min.y), (sb.max.x - size, sb.max.y))
    }

    fn v_thumb_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let track = self.v_track_rect(origin);
        let bh = self.body_height().max(1);
        let ch = self.content_height().max(1);
        let track_len = track.height();
        if track_len <= 0 { return track; }
        let thumb_len = ((bh as f64 / ch as f64) * track_len as f64).round() as i32;
        let thumb_len = thumb_len.max(MIN_THUMB_SIZE).min(track_len.max(MIN_THUMB_SIZE));
        let scroll_range = (ch - bh).max(0);
        let thumb_range = (track_len - thumb_len).max(0);
        let pos = if scroll_range > 0 {
            (-self.scroll_y.get() as f64 / scroll_range as f64 * thumb_range as f64).round() as i32
        } else { 0 };
        rect((track.min.x, track.min.y + pos), (track.max.x, track.min.y + pos + thumb_len))
    }

    fn h_thumb_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let track = self.h_track_rect(origin);
        let bw = self.body_width().max(1);
        let cw = self.content_width().max(1);
        let track_len = track.width();
        if track_len <= 0 { return track; }
        let thumb_len = ((bw as f64 / cw as f64) * track_len as f64).round() as i32;
        let thumb_len = thumb_len.max(MIN_THUMB_SIZE).min(track_len.max(MIN_THUMB_SIZE));
        let scroll_range = (cw - bw).max(0);
        let thumb_range = (track_len - thumb_len).max(0);
        let pos = if scroll_range > 0 {
            (-self.scroll_x.get() as f64 / scroll_range as f64 * thumb_range as f64).round() as i32
        } else { 0 };
        rect((track.min.x + pos, track.min.y), (track.min.x + pos + thumb_len, track.max.y))
    }

    fn v_track_length(&self) -> i32 {
        let bh = self.body_height();
        let t = self.scrollbar_thickness();
        if bh < 2 * t { 0 } else { bh - 2 * t }
    }

    fn h_track_length(&self) -> i32 {
        let bw = self.body_width();
        let t = self.scrollbar_thickness();
        if bw < 2 * t { 0 } else { bw - 2 * t }
    }

    fn fire_click(&self, ui: &mut UI) {
        self.base_fire_event(ui, EventType::Click, &EventData::None);
    }

    fn ensure_visible_display(&self, display_idx: usize) {
        let row_h = self.row_height_px.get().max(1);
        let bh = self.body_height();
        let top = display_idx as i32 * row_h;
        let bot = top + row_h;
        let cur = self.scroll_y.get();
        if top + cur < 0 {
            self.scroll_y.set(-top);
        } else if bot + cur > bh {
            self.scroll_y.set(bh - bot);
        }
        self.clamp_scroll();
    }

    fn col_at_local_x(&self, local_x: i32) -> Option<usize> {
        let offs = self.column_offsets.borrow();
        if offs.len() < 2 { return None; }
        let inset = self.border_inset();
        let x = local_x - inset - self.scroll_x.get();
        for c in 0..offs.len() - 1 {
            if x >= offs[c] && x < offs[c + 1] { return Some(c); }
        }
        None
    }

    fn divider_at_local_x(&self, local_x: i32) -> Option<usize> {
        let offs = self.column_offsets.borrow();
        if offs.len() < 2 { return None; }
        let scale = self.state.borrow().scale;
        let grab = (DIVIDER_GRAB_DIP as f64 * scale).ceil() as i32;
        let inset = self.border_inset();
        let x = local_x - inset - self.scroll_x.get();
        for c in 0..offs.len() - 1 {
            let edge = offs[c + 1];
            if (x - edge).abs() <= grab { return Some(c); }
        }
        None
    }

    fn display_at_local_y(&self, local_y_in_body: i32) -> Option<usize> {
        let row_h = self.row_height_px.get().max(1);
        let y = local_y_in_body - self.scroll_y.get();
        if y < 0 { return None; }
        let n = self.display_order.borrow().len();
        let idx = (y / row_h) as usize;
        if idx < n { Some(idx) } else { None }
    }

    fn paint_sort_indicator(&self, theme: &mut dyn Theme, cell_rect: Rect<i32>, dir: SortDirection) {
        let scale = self.state.borrow().scale;
        let size = (8.0 * scale).round() as i32;
        let cx = cell_rect.max.x - (10.0 * scale).round() as i32;
        let cy = cell_rect.min.y + cell_rect.height() / 2;
        // Draw a triangle as 4 horizontal rect slivers.
        for i in 0..(size / 2) {
            let half_w = match dir {
                SortDirection::Asc => size / 2 - i,
                SortDirection::Desc => i + 1,
            };
            let y = match dir {
                SortDirection::Asc => cy - size / 2 + i,
                SortDirection::Desc => cy - size / 2 + i,
            };
            let x0 = cx - half_w;
            let x1 = cx + half_w;
            theme.draw_rect(rect((x0, y), (x1, y + 1)), theme.color("text"));
        }
    }
}

impl View for TableView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) { return; }
        match name {
            "headers" => {
                let titles: Vec<String> = value.split(',').map(|s| s.trim().to_owned()).collect();
                let mut cols = self.columns.borrow_mut();
                for (i, t) in titles.iter().enumerate() {
                    if i < cols.len() {
                        cols[i].title = t.clone();
                    } else {
                        cols.push(ColumnDef {
                            title: t.clone(),
                            sortable: self.default_sortable.get(),
                            resizable: self.default_resizable.get(),
                            ..ColumnDef::default()
                        });
                    }
                }
            }
            "widths" => {
                let parts: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
                let mut cols = self.columns.borrow_mut();
                for (i, p) in parts.iter().enumerate() {
                    if i >= cols.len() {
                        cols.push(ColumnDef::default());
                    }
                    if let Some(w) = parse_column_width(p) {
                        cols[i].width = w;
                        cols[i].user_sized = false;
                    }
                }
            }
            "sortable" => {
                let v = value.parse().unwrap_or(true);
                self.default_sortable.set(v);
                for c in self.columns.borrow_mut().iter_mut() { c.sortable = v; }
            }
            "resizable" => {
                let v = value.parse().unwrap_or(true);
                self.default_resizable.set(v);
                for c in self.columns.borrow_mut().iter_mut() { c.resizable = v; }
            }
            "header_height" => {
                if let Ok(h) = value.parse::<i32>() {
                    self.explicit_header_height_dip.set(Some(h));
                }
            }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.state.borrow_mut().font_manager.set_font_size(size);
                }
            }
            "font" => { self.state.borrow_mut().font_manager.set_font(value); }
            "font_style" => { self.state.borrow_mut().font_manager.set_font_style(value); }
            _ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) { self.base_set_parent(parent); }
    fn get_parent(&self) -> Option<Element> { self.base_get_parent() }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        // Resolve typeface (inheriting from parent) and persist it.
        let effective = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(effective.clone()));
        self.base_set_scale(scale);

        let (mut new_w, mut new_h) = self.calculate_size(width, height, scale);
        // For Min, fall back to a sensible default size.
        {
            let state = self.state.borrow();
            if matches!(state.width, Dimension::Min) { new_w = width.max(200); }
            if matches!(state.height, Dimension::Min) { new_h = height.max(150); }
        }
        let r = rect((x, y), (x + new_w, y + new_h));
        self.set_rect(r);

        self.inner_relayout(scale, &effective);
        self.needs_relayout.set(false);
        r
    }

    fn fits_in_rect(&self, w: i32, h: i32, _scale: f64) -> bool {
        let r = self.get_rect();
        r.width() <= w && r.height() <= h
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let mut r = self.state.borrow().rect;
        r.move_by(origin);

        theme.push_clip();
        theme.clip_rect(r);

        // Outer background. A 9-patch background replaces the back and the
        // body components; headers and scrollbars stay drawable-based.
        let main_state = self.state.borrow().state;
        let ninepatch = self.base_draw_ninepatch(theme, r);
        if !ninepatch {
            theme.draw_component("edit.back", r, main_state);
        }

        let header_h = self.header_height_px.get();
        let bw = self.body_width();
        let bh = self.body_height();
        let inset = self.border_inset();

        let body_origin = Point { x: r.min.x + inset, y: r.min.y + inset + header_h };
        let body_clip = rect((body_origin.x, body_origin.y), (body_origin.x + bw, body_origin.y + bh));

        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let row_h = self.row_height_px.get();

        // ---- Body cells ----
        theme.push_clip();
        theme.clip_rect(body_clip);

        let order = self.display_order.borrow();
        let cols = self.columns.borrow();
        let col_offs = self.column_offsets.borrow();
        let rows = self.rows.borrow();

        // Compute first/last visible display rows.
        let n = order.len();
        let first_visible = if row_h > 0 { ((-scroll_y) / row_h.max(1)).max(0) as usize } else { 0 };
        let last_visible = if row_h > 0 {
            ((bh - scroll_y).max(0) / row_h.max(1) + 1) as usize
        } else { 0 };
        let last_visible = min(last_visible, n);

        for di in first_visible..last_visible {
            let raw = order[di];
            let row_top = body_origin.y + di as i32 * row_h + scroll_y;
            let row_rect = rect((body_origin.x, row_top), (body_origin.x + bw, row_top + row_h));

            // Selection highlight (light underlay so cell text remains readable)
            if self.selected_raw.get() == Some(raw) {
                theme.draw_rect(row_rect, theme.color("table_selection"));
            }

            // Cells — each cell's own paint() draws at (origin + cell.rect.min).
            // Cells are laid out at (0,0)..(cell_w, row_h), so we shift origin
            // to position them on screen.
            if let Some(row_cells) = rows.get(raw) {
                for c in 0..cols.len() {
                    let cell_x = body_origin.x + col_offs[c] + scroll_x;
                    let cell_w = cols[c].current_width_px;
                    if cell_x + cell_w < body_clip.min.x || cell_x > body_clip.max.x { continue; }
                    if let Some(cell) = row_cells.get(c) {
                        let cell_origin = Point { x: cell_x, y: row_top };
                        // Clip each cell to its own box so long cell content
                        // (e.g. a single-line Label) can't bleed into the
                        // neighbouring column or over the row separator.
                        let cell_clip = rect((cell_x, row_top), (cell_x + cell_w, row_top + row_h));
                        theme.push_clip();
                        theme.clip_rect(cell_clip);
                        cell.borrow().paint(cell_origin, theme);
                        theme.pop_clip();
                    }
                }
            }

            // Row separator
            let sep_y = row_top + row_h - 1;
            theme.draw_rect(rect((body_origin.x, sep_y), (body_origin.x + bw, sep_y + 1)), theme.color("table_separator"));
        }
        drop(order); drop(cols); drop(col_offs); drop(rows);

        theme.pop_clip();

        // ---- Header ----
        let header_origin = Point { x: r.min.x + inset, y: r.min.y + inset };
        let header_clip = rect((header_origin.x, header_origin.y), (header_origin.x + bw, header_origin.y + header_h));
        theme.push_clip();
        theme.clip_rect(header_clip);

        let cols = self.columns.borrow();
        let col_offs = self.column_offsets.borrow();
        let header_blocks = self.header_blocks.borrow();
        let sort = self.sort_state.get();
        let h_press = self.header_press.get();
        let h_hover = self.header_hover.get();
        let pad_h = (DEFAULT_CELL_PAD_H as f64 * self.state.borrow().scale).round() as i32;

        for c in 0..cols.len() {
            let cx0 = header_origin.x + col_offs[c] + scroll_x;
            let cw = cols[c].current_width_px;
            let cell_rect = rect((cx0, header_origin.y), (cx0 + cw, header_origin.y + header_h));
            if cell_rect.max.x < header_clip.min.x || cell_rect.min.x > header_clip.max.x { continue; }

            let mut cell_state = main_state;
            cell_state.hovered = h_hover == Some(c);
            cell_state.pressed = h_press == Some(c);

            theme.draw_component("button.back", cell_rect, cell_state);
            if let Some(Some(block)) = header_blocks.get(c) {
                let tw = block.width().ceil() as i32;
                let th = block.height().ceil() as i32;
                let tx = match cols[c].align {
                    HAlign::Left => cell_rect.min.x + pad_h,
                    HAlign::Right => cell_rect.max.x - pad_h - tw,
                    HAlign::Center => cell_rect.min.x + (cw - tw) / 2,
                };
                let ty = cell_rect.min.y + (header_h - th) / 2;
                // Crop the title to its cell interior so a long header label
                // can't spill into the neighbouring column.
                let crop = rect((cell_rect.min.x + pad_h, cell_rect.min.y), (cell_rect.max.x - pad_h, cell_rect.max.y));
                theme.draw_text_cropped(tx as f32, ty as f32, crop, theme.color("text"), block);
            }
            if let Some((sc, dir)) = sort
                && sc == c
            {
                self.paint_sort_indicator(theme, cell_rect, dir);
            }
            // Header cells inherit the table's `focused` flag via `main_state`,
            // which makes `button.body` paint a dashed focus rectangle. Keep the
            // focus *lighten* on `button.back` above, but drop the focus lines
            // here (same as the scrollbar thumbs).
            let mut body_state = cell_state;
            body_state.focused = false;
            theme.draw_component("button.body", cell_rect, body_state);
        }
        drop(cols); drop(col_offs); drop(header_blocks);

        theme.pop_clip();

        // ---- Scrollbars ----
        let unfocused = ViewState::no_focus();
        if self.v_scroll_visible.get() {
            let track = self.v_track_rect(origin);
            let thumb = self.v_thumb_rect(origin);
            let arrow_top = self.v_arrow_top_rect(origin);
            let arrow_bot = self.v_arrow_bottom_rect(origin);
            for (arrow_rect, role) in [(arrow_top, "scrollbar.arrow.up"), (arrow_bot, "scrollbar.arrow.down")] {
                theme.draw_component("button.back", arrow_rect, unfocused);
                theme.draw_component("button.body", arrow_rect, unfocused);
                theme.draw_component(role, arrow_rect, unfocused);
            }
            theme.draw_component("scrollbar.track", track, unfocused);
            let mut s = main_state;
            s.pressed = matches!(self.drag.get(), DragKind::DragVThumb);
            s.focused = false; // no focus ring on scrollbar thumbs
            theme.draw_component("button.back", thumb, s);
            theme.draw_component("button.body", thumb, s);
        }
        if self.h_scroll_visible.get() {
            let track = self.h_track_rect(origin);
            let thumb = self.h_thumb_rect(origin);
            let arrow_l = self.h_arrow_left_rect(origin);
            let arrow_r = self.h_arrow_right_rect(origin);
            for (arrow_rect, role) in [(arrow_l, "scrollbar.arrow.left"), (arrow_r, "scrollbar.arrow.right")] {
                theme.draw_component("button.back", arrow_rect, unfocused);
                theme.draw_component("button.body", arrow_rect, unfocused);
                theme.draw_component(role, arrow_rect, unfocused);
            }
            theme.draw_component("scrollbar.track", track, unfocused);
            let mut s = main_state;
            s.pressed = matches!(self.drag.get(), DragKind::DragHThumb);
            s.focused = false; // no focus ring on scrollbar thumbs
            theme.draw_component("button.back", thumb, s);
            theme.draw_component("button.body", thumb, s);
        }
        // Dead corner where V and H scrollbars meet — fill flat so it blends
        // with the scrollbar tracks instead of leaving a confusing gap.
        if self.v_scroll_visible.get() && self.h_scroll_visible.get() {
            let thickness = self.scrollbar_thickness();
            let v_sb = self.v_scrollbar_rect(origin);
            let h_sb = self.h_scrollbar_rect(origin);
            let corner = rect((v_sb.min.x, h_sb.min.y), (v_sb.min.x + thickness, h_sb.min.y + thickness));
            theme.draw_component("scrollbar.track", corner, unfocused);
        }

        // Outer border
        if !ninepatch {
            theme.draw_component("edit.body", r, main_state);
        }

        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> { Some(self.state.borrow().state) }
    fn get_rect(&self) -> Rect<i32> { self.base_get_rect() }
    fn set_rect(&mut self, r: Rect<i32>) { self.base_set_rect(r); }
    fn get_padding(&self, scale: f64) -> Borders { self.base_get_padding(scale) }
    fn set_padding(&self, t: i32, l: i32, r: i32, b: i32) { self.base_set_padding(t, l, r, b); }
    fn get_margin(&self, scale: f64) -> Borders { self.base_get_margin(scale) }
    fn set_margin(&self, t: i32, l: i32, r: i32, b: i32) { self.base_set_margin(t, l, r, b); }
    fn get_gravity(&self) -> Gravity { self.base_get_gravity() }
    fn get_layout_params(&self) -> super::LayoutParams { self.base_get_layout_params() }
    fn set_layout_params(&self, params: super::LayoutParams) { self.base_set_layout_params(params); }
    fn set_gravity(&self, g: Gravity) { self.base_set_gravity(g); }
    fn get_bounds(&self) -> (Dimension, Dimension) { self.base_get_bounds() }
    fn get_content_size(&self) -> (i32, i32) {
        // Reasonable minimum so Dimension::Min lays out something visible.
        let w = self.content_width().max(200);
        let h = (self.header_height_px.get() + self.content_height()).max(120);
        (w, h)
    }
    fn is_focused(&self) -> bool { self.base_is_focused() }
    fn is_break(&self) -> bool { self.base_is_break() }
    fn set_focused(&self, focused: bool) { self.base_set_focused(focused); }
    fn set_focusable(&self, focusable: bool) { self.base_set_focusable(focusable); }
    fn set_width(&mut self, width: Dimension) { self.base_set_width(width); }
    fn set_height(&mut self, height: Dimension) { self.base_set_height(height); }
    fn set_scale(&mut self, scale: f64) { self.base_set_scale(scale); }
    fn set_id(&mut self, id: &str) { self.base_set_id(id); }
    fn get_id(&self) -> String { self.base_get_id() }
    fn get_tooltip(&self) -> Option<String> { self.base_get_tooltip() }
    fn set_tooltip(&mut self, t: Option<String>) { self.base_set_tooltip(t); }
    fn get_content_description(&self) -> Option<String> { self.base_get_content_description() }
    fn set_content_description(&mut self, d: Option<String>) { self.base_set_content_description(d); }
    fn get_labelled_by(&self) -> Option<String> { self.base_get_labelled_by() }
    fn set_labelled_by(&mut self, v: Option<String>) { self.base_set_labelled_by(v); }
    fn get_background(&self) -> Option<u32> { self.base_get_background() }
    fn set_background(&mut self, c: Option<u32>) { self.base_set_background(c); }
    fn get_border_color(&self) -> Option<u32> { self.base_get_border_color() }
    fn set_border_color(&mut self, c: Option<u32>) { self.base_set_border_color(c); }
    fn is_enabled(&self) -> bool { self.base_is_enabled() }
    fn set_enabled(&mut self, e: bool) { self.base_set_enabled(e); }
    fn get_visibility(&self) -> Visibility { self.base_get_visibility() }
    fn set_visibility(&mut self, v: Visibility) { self.base_set_visibility(v); }

    fn as_container(&self) -> Option<&dyn Container> { Some(self) }
    fn as_container_mut(&mut self) -> Option<&mut dyn Container> { Some(self) }

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
        let mut node = accesskit::Node::new(accesskit::Role::Table);
        node.set_row_count(self.row_count());
        node.set_column_count(self.columns.borrow().len());
        node
    }

    /// Synthetic table structure: a header `Row` of `ColumnHeader`s (with the
    /// live sort direction), then one `Row` per data row in display order,
    /// each claiming its real cell elements as children. Item-id space:
    /// 0 = header row, 1..=columns = headers, columns+1+raw = data row `raw`
    /// (keyed by raw index so ids survive re-sorting).
    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        let id = self.get_id();
        let inset = self.border_inset();
        let header_h = self.header_height_px.get();
        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let row_h = self.row_height_px.get();
        let width = self.get_rect_width();
        let cols = self.columns.borrow();
        let col_offs = self.column_offsets.borrow();
        let order = self.display_order.borrow();
        let rows = self.rows.borrow();
        let sort = self.sort_state.get();

        let mut result = Vec::new();

        // Header row first, so it precedes the data rows in reading order.
        let mut header_children = Vec::new();
        let mut header_nodes = Vec::new();
        for (c, col) in cols.iter().enumerate() {
            let header_id = crate::accessibility::item_node_id(&id, 1 + c);
            let mut node = accesskit::Node::new(accesskit::Role::ColumnHeader);
            node.set_label(col.title.clone());
            node.set_column_index(c);
            if let Some((sort_col, dir)) = sort
                && sort_col == c
            {
                node.set_sort_direction(match dir {
                    SortDirection::Asc => accesskit::SortDirection::Ascending,
                    SortDirection::Desc => accesskit::SortDirection::Descending,
                });
            }
            if col.sortable {
                node.add_action(accesskit::Action::Click);
            }
            let x = inset + col_offs[c] + scroll_x;
            node.set_bounds(accesskit::Rect {
                x0: x as f64,
                y0: inset as f64,
                x1: (x + col.current_width_px) as f64,
                y1: (inset + header_h) as f64,
            });
            header_children.push(header_id);
            header_nodes.push((header_id, node));
        }
        let mut header_row = accesskit::Node::new(accesskit::Role::Row);
        header_row.set_children(header_children);
        header_row.set_bounds(accesskit::Rect {
            x0: inset as f64,
            y0: inset as f64,
            x1: (width - inset) as f64,
            y1: (inset + header_h) as f64,
        });
        result.push((crate::accessibility::item_node_id(&id, 0), header_row));
        result.extend(header_nodes);

        // Data rows, in display (sorted) order.
        let base = 1 + cols.len();
        for (di, &raw) in order.iter().enumerate() {
            let row_id = crate::accessibility::item_node_id(&id, base + raw);
            let mut node = accesskit::Node::new(accesskit::Role::Row);
            node.set_row_index(di);
            node.set_selected(self.selected_raw.get() == Some(raw));
            if let Some(cells) = rows.get(raw) {
                let kids: Vec<accesskit::NodeId> = cells.iter()
                    .map(|cell| crate::accessibility::node_id_for(&cell.borrow().get_id()))
                    .collect();
                node.set_children(kids);
            }
            let y = inset + header_h + di as i32 * row_h + scroll_y;
            node.set_bounds(accesskit::Rect {
                x0: inset as f64,
                y0: y as f64,
                x1: (width - inset) as f64,
                y1: (y + row_h) as f64,
            });
            result.push((row_id, node));
        }
        result
    }

    /// Cells are laid out at (0,0) and positioned at paint time, so the table
    /// exposes them manually with their on-screen offsets (header + scroll).
    fn accessibility_child_elements(&self) -> Vec<(Element, Point<i32>)> {
        let inset = self.border_inset();
        let header_h = self.header_height_px.get();
        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let row_h = self.row_height_px.get();
        let col_offs = self.column_offsets.borrow();
        let order = self.display_order.borrow();
        let rows = self.rows.borrow();

        let mut result = Vec::new();
        for (di, &raw) in order.iter().enumerate() {
            let Some(cells) = rows.get(raw) else { continue };
            let y = inset + header_h + di as i32 * row_h + scroll_y;
            for (c, cell) in cells.iter().enumerate() {
                let x = inset + col_offs.get(c).copied().unwrap_or(0) + scroll_x;
                result.push((std::rc::Rc::clone(cell), Point::new(x, y)));
            }
        }
        result
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.fire_click(ui);
        true
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        if self.needs_relayout.replace(false) {
            // Self-heal: refresh inner layout against the cached rect.
            // Do NOT call `layout_content` — that would re-apply
            // `Dimension::Percent` to the already-resolved height and shrink
            // the grid every tick.
            let scale = self.state.borrow().scale;
            let typeface = self.state.borrow().font_manager.get().unwrap_or_default();
            self.inner_relayout(scale, &typeface);
            return true;
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Point<i32>) -> bool {
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) {
            if self.header_hover.get().is_some() {
                self.header_hover.set(None);
                return true;
            }
            return false;
        }
        let local_x = position.x - r.min.x;
        let local_y = position.y - r.min.y;
        let header_h = self.header_height_px.get();

        // Drag updates first.
        match self.drag.get() {
            DragKind::ResizeColumn => {
                let scale = self.state.borrow().scale;
                let min_w = (DEFAULT_MIN_COL_WIDTH as f64 * scale).round() as i32;
                let new_w = (self.drag_anchor_width.get() + (local_x - self.drag_anchor_x.get())).max(min_w);
                let col = self.drag_col.get();
                if let Some(c) = self.columns.borrow_mut().get_mut(col) {
                    c.current_width_px = new_w;
                    c.user_sized = true;
                }
                self.needs_relayout.set(true);
                return true;
            }
            DragKind::DragVThumb => {
                let bh = self.body_height().max(1);
                let ch = self.content_height().max(1);
                let track_len = self.v_track_length().max(1);
                let thumb_len = ((bh as f64 / ch as f64) * track_len as f64).round() as i32;
                let thumb_len = thumb_len.max(MIN_THUMB_SIZE).min(track_len.max(MIN_THUMB_SIZE));
                let scroll_range = (ch - bh).max(1) as f64;
                let thumb_range = (track_len - thumb_len).max(1) as f64;
                let dy = (local_y - self.drag_anchor_y.get()) as f64;
                let new_scroll = self.drag_anchor_scroll.get() as f64 - dy * (scroll_range / thumb_range);
                self.scroll_y.set(new_scroll.round() as i32);
                self.clamp_scroll();
                return true;
            }
            DragKind::DragHThumb => {
                let bw = self.body_width().max(1);
                let cw = self.content_width().max(1);
                let track_len = self.h_track_length().max(1);
                let thumb_len = ((bw as f64 / cw as f64) * track_len as f64).round() as i32;
                let thumb_len = thumb_len.max(MIN_THUMB_SIZE).min(track_len.max(MIN_THUMB_SIZE));
                let scroll_range = (cw - bw).max(1) as f64;
                let thumb_range = (track_len - thumb_len).max(1) as f64;
                let dx = (local_x - self.drag_anchor_x.get()) as f64;
                let new_scroll = self.drag_anchor_scroll.get() as f64 - dx * (scroll_range / thumb_range);
                self.scroll_x.set(new_scroll.round() as i32);
                self.clamp_scroll();
                return true;
            }
            DragKind::None => {}
        }

        // Hover updates on header.
        let inset = self.border_inset();
        let in_header = local_y >= inset && local_y < inset + header_h;
        let new_hover = if in_header { self.col_at_local_x(local_x) } else { None };
        if new_hover != self.header_hover.get() {
            self.header_hover.set(new_hover);
            return true;
        }
        false
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) { return false; }
        if !matches!(button, MouseButton::Left) { return false; }

        self.state.borrow_mut().state.focused = true;

        let local_x = position.x - r.min.x;
        let local_y = position.y - r.min.y;
        let header_h = self.header_height_px.get();

        // Local screen coords for scrollbar hit test (track rects are absolute,
        // so build them relative to origin=0 to match `position`).
        let v_thumb = self.v_thumb_rect(point(0, 0));
        let h_thumb = self.h_thumb_rect(point(0, 0));

        if self.v_scroll_visible.get() && v_thumb.hit((position.x, position.y)) {
            self.drag.set(DragKind::DragVThumb);
            self.drag_anchor_y.set(local_y);
            self.drag_anchor_scroll.set(self.scroll_y.get());
            return true;
        }
        if self.h_scroll_visible.get() && h_thumb.hit((position.x, position.y)) {
            self.drag.set(DragKind::DragHThumb);
            self.drag_anchor_x.set(local_x);
            self.drag_anchor_scroll.set(self.scroll_x.get());
            return true;
        }
        // Arrow buttons: scroll by one row/step on click.
        if self.v_scroll_visible.get() {
            let arrow_top = self.v_arrow_top_rect(point(0, 0));
            let arrow_bot = self.v_arrow_bottom_rect(point(0, 0));
            let row_h = self.row_height_px.get().max(20);
            if arrow_top.hit((position.x, position.y)) {
                self.scroll_y.set(self.scroll_y.get() + row_h);
                self.clamp_scroll();
                return true;
            }
            if arrow_bot.hit((position.x, position.y)) {
                self.scroll_y.set(self.scroll_y.get() - row_h);
                self.clamp_scroll();
                return true;
            }
        }
        if self.h_scroll_visible.get() {
            let arrow_l = self.h_arrow_left_rect(point(0, 0));
            let arrow_r = self.h_arrow_right_rect(point(0, 0));
            let step = self.row_height_px.get().max(20);
            if arrow_l.hit((position.x, position.y)) {
                self.scroll_x.set(self.scroll_x.get() + step);
                self.clamp_scroll();
                return true;
            }
            if arrow_r.hit((position.x, position.y)) {
                self.scroll_x.set(self.scroll_x.get() - step);
                self.clamp_scroll();
                return true;
            }
        }

        let inset = self.border_inset();
        if local_y >= inset && local_y < inset + header_h {
            // Try divider grab first.
            if let Some(c) = self.divider_at_local_x(local_x) {
                let resizable = self.columns.borrow().get(c).map(|cc| cc.resizable).unwrap_or(false);
                if resizable {
                    self.drag.set(DragKind::ResizeColumn);
                    self.drag_col.set(c);
                    self.drag_anchor_x.set(local_x);
                    self.drag_anchor_width.set(self.columns.borrow()[c].current_width_px);
                    return true;
                }
            }
            if let Some(c) = self.col_at_local_x(local_x) {
                self.header_press.set(Some(c));
                return true;
            }
            return false;
        }

        // Body click — selection.
        let body_local_y = local_y - inset - header_h;
        if let Some(d) = self.display_at_local_y(body_local_y) {
            let raw = self.display_order.borrow()[d];
            self.selected_raw.set(Some(raw));
            self.ensure_visible_display(d);
            // Defer the listener call to mouse-up to match Button semantics.
            return true;
        }
        true
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) { return false; }
        let r = self.state.borrow().rect;
        let local_x = position.x - r.min.x;
        let local_y = position.y - r.min.y;
        let header_h = self.header_height_px.get();

        let was_dragging = self.drag.get();
        if was_dragging != DragKind::None {
            self.drag.set(DragKind::None);
            return true;
        }

        let inset = self.border_inset();

        // Header click → toggle sort if release is over the same header cell.
        if let Some(pressed) = self.header_press.get() {
            self.header_press.set(None);
            if local_y >= inset && local_y < inset + header_h {
                if let Some(release_col) = self.col_at_local_x(local_x) {
                    if release_col == pressed {
                        let sortable = self.columns.borrow().get(pressed).map(|c| c.sortable).unwrap_or(false);
                        if sortable {
                            let new_state = match self.sort_state.get() {
                                Some((c, SortDirection::Asc)) if c == pressed => Some((pressed, SortDirection::Desc)),
                                Some((c, SortDirection::Desc)) if c == pressed => None,
                                _ => Some((pressed, SortDirection::Asc)),
                            };
                            match new_state {
                                Some((c, d)) => self.set_sort(c, d),
                                None => self.clear_sort(),
                            }
                            return true;
                        }
                    }
                }
            }
            return true;
        }

        // Body click release → fire selection click.
        if r.hit((position.x, position.y)) && local_y >= inset + header_h {
            self.fire_click(ui);
            return true;
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) { return false; }
        let row_h = self.row_height_px.get().max(20);
        let bh = self.body_height();
        let bw = self.body_width();

        let (dx_lines, dy_lines, dx_pix, dy_pix, dx_pages, dy_pages) = match distance {
            MouseScrollDistance::Lines { x, y, .. } => (x as i32, y as i32, 0, 0, 0, 0),
            MouseScrollDistance::Pixels { x, y, .. } => (0, 0, x as i32, y as i32, 0, 0),
            MouseScrollDistance::Pages { x, y, .. } => (0, 0, 0, 0, x as i32, y as i32),
        };

        let mut sy = self.scroll_y.get();
        sy += dy_lines * row_h + dy_pix + dy_pages * bh;
        self.scroll_y.set(sy);

        let mut sx = self.scroll_x.get();
        sx += dx_lines * row_h + dx_pix + dx_pages * bw;
        self.scroll_x.set(sx);

        self.clamp_scroll();
        true
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_focused() { return false; }
        let Some(code) = virtual_key_code else { return false; };
        let n = self.display_order.borrow().len();
        if n == 0 { return false; }
        let row_h = self.row_height_px.get().max(1);
        let bh = self.body_height();
        let visible_rows = (bh / row_h).max(1) as usize;

        let cur_display = self.selected_raw.get()
            .and_then(|raw| self.raw_to_display(raw));
        let new_display = match code {
            VirtualKeyCode::Up => Some(cur_display.map(|d| d.saturating_sub(1)).unwrap_or(0)),
            VirtualKeyCode::Down => Some(cur_display.map(|d| (d + 1).min(n - 1)).unwrap_or(0)),
            VirtualKeyCode::Home => Some(0),
            VirtualKeyCode::End => Some(n - 1),
            VirtualKeyCode::PageUp => Some(cur_display.map(|d| d.saturating_sub(visible_rows)).unwrap_or(0)),
            VirtualKeyCode::PageDown => Some(cur_display.map(|d| (d + visible_rows).min(n - 1)).unwrap_or(0)),
            _ => None,
        };
        if let Some(d) = new_display {
            let raw = self.display_order.borrow()[d];
            self.selected_raw.set(Some(raw));
            self.ensure_visible_display(d);
            self.fire_click(ui);
            return true;
        }
        false
    }
}

impl Container for TableView {
    fn add_view(&mut self, view: Element) {
        // The XML parser hands us children unconditionally. Recognize our
        // helpers; ignore anything else (a stray cell without a `<Row>`).
        let consumed = {
            let mut borrowed = view.borrow_mut();
            if let Some(col) = borrowed.as_any_mut().downcast_mut::<TableColumn>() {
                let def = col.def.borrow().clone();
                self.columns.borrow_mut().push(def);
                true
            } else if let Some(row) = borrowed.as_any_mut().downcast_mut::<TableRow>() {
                let cells = std::mem::take(&mut *row.cells.borrow_mut());
                drop(borrowed);
                self.add_row(cells);
                true
            } else {
                false
            }
        };
        if consumed {
            self.needs_relayout.set(true);
        }
    }

    fn get_view(&self, id: &str) -> Option<Element> {
        for row in self.rows.borrow().iter() {
            for cell in row {
                if cell.borrow().get_id() == id { return Some(cell.clone()); }
                if let Some(c) = cell.borrow().as_container() {
                    if let Some(found) = c.get_view(id) { return Some(found); }
                }
            }
        }
        None
    }

    fn get_view_count(&self) -> usize {
        self.rows.borrow().iter().map(|r| r.len()).sum()
    }

    fn get_views(&self) -> Vec<Element> {
        let mut v = Vec::new();
        for row in self.rows.borrow().iter() {
            for cell in row { v.push(cell.clone()); }
        }
        v
    }
}

impl Default for TableView {
    fn default() -> Self {
        TableView::new(rect((0, 0), (300, 200)))
    }
}

// =========================================================================
// TableColumn — XML-only marker view.
// =========================================================================

pub struct TableColumn {
    state: RefCell<FieldsMain>,
    def: RefCell<ColumnDef>,
}

impl HasMainFields for TableColumn {
    fn main_fields(&self) -> &RefCell<FieldsMain> { &self.state }
}
impl ViewBasics for TableColumn {}

impl Default for TableColumn {
    fn default() -> Self {
        TableColumn {
            state: RefCell::new(FieldsMain::with_rect(rect((0, 0), (0, 0)), Dimension::Min, Dimension::Min)),
            def: RefCell::new(ColumnDef::default()),
        }
    }
}

impl View for TableColumn {
    fn set_any(&mut self, name: &str, value: &str) {
        // Intercept column-specific attribute names BEFORE delegating to
        // base_set_any — `width` here means column width (Fixed/Star), not a
        // layout Dimension, so we must not let the base parser consume it.
        {
            let mut def = self.def.borrow_mut();
            match name {
                "title" => { def.title = value.to_owned(); return; }
                "width" => {
                    if let Some(w) = parse_column_width(value) { def.width = w; }
                    return;
                }
                "align" => { def.align = parse_h_align(value); return; }
                "sortable" => { def.sortable = value.parse().unwrap_or(true); return; }
                "resizable" => { def.resizable = value.parse().unwrap_or(true); return; }
                "min_width" => {
                    if let Ok(v) = value.parse::<i32>() { def.min_width_dip = v; }
                    return;
                }
                _ => {}
            }
        }
        let _ = self.base_set_any(name, value);
    }
    fn set_parent(&self, parent: Option<WeakElement>) { self.base_set_parent(parent); }
    fn get_parent(&self) -> Option<Element> { self.base_get_parent() }
    fn layout_content(&mut self, x: i32, y: i32, _w: i32, _h: i32, _typeface: &Typeface, _scale: f64) -> Rect<i32> {
        let r = rect((x, y), (x, y));
        self.set_rect(r);
        r
    }
    fn fits_in_rect(&self, _w: i32, _h: i32, _scale: f64) -> bool { true }
    fn paint(&self, _origin: Point<i32>, _theme: &mut dyn Theme) {}
    fn get_state(&self) -> Option<ViewState> { Some(self.state.borrow().state) }
    fn get_rect(&self) -> Rect<i32> { self.base_get_rect() }
    fn set_rect(&mut self, r: Rect<i32>) { self.base_set_rect(r); }
    fn set_padding(&self, t: i32, l: i32, rr: i32, b: i32) { self.base_set_padding(t, l, rr, b); }
    fn set_margin(&self, t: i32, l: i32, rr: i32, b: i32) { self.base_set_margin(t, l, rr, b); }
    fn get_bounds(&self) -> (Dimension, Dimension) { self.base_get_bounds() }
    fn get_content_size(&self) -> (i32, i32) { (0, 0) }
    fn set_focusable(&self, f: bool) { self.base_set_focusable(f); }
    fn set_width(&mut self, w: Dimension) { self.base_set_width(w); }
    fn set_height(&mut self, h: Dimension) { self.base_set_height(h); }
    fn set_scale(&mut self, s: f64) { self.base_set_scale(s); }
    fn set_id(&mut self, id: &str) { self.base_set_id(id); }
    fn get_id(&self) -> String { self.base_get_id() }
    fn on_event(&mut self, event: EventType, func: EventCallback) { self.base_on_event(event, func); }
    fn has_listener(&self, event: EventType) -> bool { self.base_has_listener(event) }
    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool { self.base_fire_event(ui, event, data) }
    fn click(&self, _ui: &mut UI) -> bool { false }
}

// =========================================================================
// TableRow — XML-only container collecting cell views.
// =========================================================================

pub struct TableRow {
    state: RefCell<FieldsMain>,
    cells: RefCell<Vec<Element>>,
}

impl HasMainFields for TableRow {
    fn main_fields(&self) -> &RefCell<FieldsMain> { &self.state }
}
impl ViewBasics for TableRow {}

impl Default for TableRow {
    fn default() -> Self {
        TableRow {
            state: RefCell::new(FieldsMain::with_rect(rect((0, 0), (0, 0)), Dimension::Min, Dimension::Min)),
            cells: RefCell::new(Vec::new()),
        }
    }
}

impl View for TableRow {
    fn set_any(&mut self, name: &str, value: &str) {
        let _ = self.base_set_any(name, value);
    }
    fn set_parent(&self, parent: Option<WeakElement>) { self.base_set_parent(parent); }
    fn get_parent(&self) -> Option<Element> { self.base_get_parent() }
    fn layout_content(&mut self, x: i32, y: i32, _w: i32, _h: i32, _typeface: &Typeface, _scale: f64) -> Rect<i32> {
        let r = rect((x, y), (x, y));
        self.set_rect(r);
        r
    }
    fn fits_in_rect(&self, _w: i32, _h: i32, _scale: f64) -> bool { true }
    fn paint(&self, _origin: Point<i32>, _theme: &mut dyn Theme) {}
    fn get_state(&self) -> Option<ViewState> { Some(self.state.borrow().state) }
    fn get_rect(&self) -> Rect<i32> { self.base_get_rect() }
    fn set_rect(&mut self, r: Rect<i32>) { self.base_set_rect(r); }
    fn set_padding(&self, t: i32, l: i32, rr: i32, b: i32) { self.base_set_padding(t, l, rr, b); }
    fn set_margin(&self, t: i32, l: i32, rr: i32, b: i32) { self.base_set_margin(t, l, rr, b); }
    fn get_bounds(&self) -> (Dimension, Dimension) { self.base_get_bounds() }
    fn get_content_size(&self) -> (i32, i32) { (0, 0) }
    fn set_focusable(&self, f: bool) { self.base_set_focusable(f); }
    fn set_width(&mut self, w: Dimension) { self.base_set_width(w); }
    fn set_height(&mut self, h: Dimension) { self.base_set_height(h); }
    fn set_scale(&mut self, s: f64) { self.base_set_scale(s); }
    fn set_id(&mut self, id: &str) { self.base_set_id(id); }
    fn get_id(&self) -> String { self.base_get_id() }
    fn as_container(&self) -> Option<&dyn Container> { Some(self) }
    fn as_container_mut(&mut self) -> Option<&mut dyn Container> { Some(self) }
    fn on_event(&mut self, event: EventType, func: EventCallback) { self.base_on_event(event, func); }
    fn has_listener(&self, event: EventType) -> bool { self.base_has_listener(event) }
    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool { self.base_fire_event(ui, event, data) }
    fn click(&self, _ui: &mut UI) -> bool { false }
}

impl Container for TableRow {
    fn add_view(&mut self, view: Element) {
        self.cells.borrow_mut().push(view);
    }
    fn get_view(&self, id: &str) -> Option<Element> {
        for c in self.cells.borrow().iter() {
            if c.borrow().get_id() == id { return Some(c.clone()); }
        }
        None
    }
    fn get_view_count(&self) -> usize { self.cells.borrow().len() }
    fn get_views(&self) -> Vec<Element> { self.cells.borrow().clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_table() -> TableView { TableView::new(rect((0, 0), (400, 200))) }

    #[test]
    fn relayout_columns_distributes_star() {
        let g = make_table();
        g.set_columns(vec![
            ColumnDef { title: "A".into(), width: ColumnWidth::Fixed(100), ..ColumnDef::default() },
            ColumnDef { title: "B".into(), width: ColumnWidth::Star(1.0), ..ColumnDef::default() },
            ColumnDef { title: "C".into(), width: ColumnWidth::Star(2.0), ..ColumnDef::default() },
        ]);
        g.relayout_columns(400, 1.0);
        let cols = g.columns.borrow();
        assert_eq!(cols[0].current_width_px, 100);
        // Remaining 300, split 1:2 → 100, 200
        assert_eq!(cols[1].current_width_px, 100);
        assert_eq!(cols[2].current_width_px, 200);
    }

    #[test]
    fn relayout_columns_clamps_to_min_width() {
        let g = make_table();
        g.set_columns(vec![
            ColumnDef { title: "A".into(), width: ColumnWidth::Star(1.0), min_width_dip: 80, ..ColumnDef::default() },
            ColumnDef { title: "B".into(), width: ColumnWidth::Star(1.0), min_width_dip: 80, ..ColumnDef::default() },
        ]);
        // Available much smaller than total min — both get min, accept overflow.
        g.relayout_columns(50, 1.0);
        let cols = g.columns.borrow();
        assert_eq!(cols[0].current_width_px, 80);
        assert_eq!(cols[1].current_width_px, 80);
    }

    #[test]
    fn user_sized_suppresses_star_redistribution() {
        let g = make_table();
        g.set_columns(vec![
            ColumnDef { title: "A".into(), width: ColumnWidth::Star(1.0), current_width_px: 250, user_sized: true, ..ColumnDef::default() },
            ColumnDef { title: "B".into(), width: ColumnWidth::Star(1.0), ..ColumnDef::default() },
        ]);
        g.relayout_columns(400, 1.0);
        let cols = g.columns.borrow();
        assert_eq!(cols[0].current_width_px, 250); // preserved
        assert_eq!(cols[1].current_width_px, 150); // remaining
    }
}
