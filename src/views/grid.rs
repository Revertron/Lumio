//! Simple non-scrolling grid layout. Children are placed into cells in
//! row-major order; the column count is determined by the `widths`
//! attribute. Designed to be wrapped in a `ScrollView` when the content
//! is taller than the available space — this widget itself does not
//! scroll, sort, or resize.
//!
//! For a full-featured table (sticky header, sortable headers, drag-resize
//! columns, V/H scrollbars), use `TableView`.

use std::cell::{Cell, RefCell};

use crate::input::MouseButton;

use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Container, Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::tableview::parse_column_width;
use crate::views::{Borders, ColumnWidth, Dimension, FieldsMain, Gravity, Visibility};

const DEFAULT_MIN_COL_WIDTH: i32 = 32;

/// A lightweight non-scrolling 2D grid. Children are added in row-major order
/// and flow into cells; the number of columns equals the number of entries in
/// `widths`. Column widths are fixed dip or `*` star-proportional (the same
/// `ColumnWidth` syntax `TableView` uses); each row's height is its tallest
/// cell. Wrap the grid in a `ScrollView` if its content can overflow.
///
/// XML: `<Grid widths="100,*,50"> ...children... </Grid>`.
pub struct Grid {
    state: RefCell<FieldsMain>,
    /// Column widths, one per column. Number of columns = `widths.len()`.
    widths: RefCell<Vec<ColumnWidth>>,
    /// Resolved per-column widths in physical pixels at the current scale.
    column_offsets: RefCell<Vec<i32>>,    // cumulative left edges, len = n_cols + 1
    /// Children, stored in row-major order. Each `n_cols` consecutive
    /// children form one row.
    cells: RefCell<Vec<Element>>,
    /// Per-row height in physical pixels (max of cell heights in that row).
    row_heights: RefCell<Vec<i32>>,
    /// Cumulative row-top offsets in physical pixels, len = rows + 1.
    row_offsets: RefCell<Vec<i32>>,
    needs_relayout: Cell<bool>,
}

impl HasMainFields for Grid {
    fn main_fields(&self) -> &RefCell<FieldsMain> { &self.state }
}
impl ViewBasics for Grid {}

#[allow(dead_code)]
impl Grid {
    /// Create an empty grid with the given bounds and no columns.
    pub fn new(rect_: Rect<i32>) -> Grid {
        Grid {
            state: RefCell::new(FieldsMain::with_rect(rect_, Dimension::Min, Dimension::Min)),
            widths: RefCell::new(Vec::new()),
            column_offsets: RefCell::new(vec![0]),
            cells: RefCell::new(Vec::new()),
            row_heights: RefCell::new(Vec::new()),
            row_offsets: RefCell::new(vec![0]),
            needs_relayout: Cell::new(false),
        }
    }

    /// Replace the column definitions. The grid then has `widths.len()`
    /// columns and existing cells re-flow into them row-major.
    pub fn set_widths(&self, widths: Vec<ColumnWidth>) {
        *self.widths.borrow_mut() = widths;
        self.needs_relayout.set(true);
    }

    /// The current column-width definitions.
    pub fn widths(&self) -> Vec<ColumnWidth> { self.widths.borrow().clone() }

    /// Number of columns (the number of width entries).
    pub fn column_count(&self) -> usize { self.widths.borrow().len() }

    /// Number of rows, counting a trailing partially-filled row.
    pub fn row_count(&self) -> usize {
        let n_cols = self.column_count().max(1);
        let n_cells = self.cells.borrow().len();
        n_cells.div_ceil(n_cols)
    }

    fn relayout_columns(&self, available_px: i32, scale: f64) {
        let widths = self.widths.borrow();
        let n = widths.len();
        let min_clamp = (DEFAULT_MIN_COL_WIDTH as f64 * scale).round() as i32;

        let mut resolved = vec![0i32; n];
        let mut sum_fixed = 0i32;
        let mut star_indices: Vec<usize> = Vec::new();
        let mut star_total: f32 = 0.0;
        for (i, w) in widths.iter().enumerate() {
            match *w {
                ColumnWidth::Fixed(d) => {
                    let px = ((d as f64 * scale).round() as i32).max(min_clamp);
                    resolved[i] = px;
                    sum_fixed += px;
                }
                ColumnWidth::Star(s) => {
                    star_indices.push(i);
                    star_total += s.max(0.0001);
                }
            }
        }

        let remainder = (available_px - sum_fixed).max(0);
        if !star_indices.is_empty() && star_total > 0.0 {
            let mut leftover = remainder;
            for (k, &i) in star_indices.iter().enumerate() {
                let s = match widths[i] { ColumnWidth::Star(s) => s.max(0.0001), _ => 1.0 };
                let share = if k + 1 == star_indices.len() {
                    leftover
                } else {
                    ((s / star_total) * remainder as f32).round() as i32
                };
                let px = share.max(min_clamp);
                resolved[i] = px;
                leftover -= px;
            }
        }

        let mut offs = self.column_offsets.borrow_mut();
        offs.clear();
        offs.push(0);
        let mut acc = 0i32;
        for w in &resolved { acc += w; offs.push(acc); }
    }

    fn layout_cells(&self, scale: f64, font: &Typeface) {
        let n_cols = self.column_count();
        if n_cols == 0 {
            self.row_heights.borrow_mut().clear();
            self.row_offsets.borrow_mut().clear();
            self.row_offsets.borrow_mut().push(0);
            return;
        }
        let padding = self.get_padding(scale);
        let cells = self.cells.borrow();
        let col_offs = self.column_offsets.borrow();

        // Pass 1: lay each cell out at (0, 0) to measure its content size.
        // We capture the per-cell height — used both for the row-max (row
        // height) AND for setting each cell's rect in Pass 2. Setting the
        // rect to the row max would pollute Label's cached rect (`Label::
        // layout_content` early-returns it when (width, height, scale)
        // params haven't changed), so on a subsequent relayout where only
        // some columns changed, cells in unchanged columns would return
        // the stale row-max height and rows would never shrink.
        let mut measured_heights: Vec<i32> = Vec::with_capacity(cells.len());
        let mut row_heights: Vec<i32> = Vec::new();
        let mut row_h = 0i32;
        for (idx, cell) in cells.iter().enumerate() {
            let col = idx % n_cols;
            let cell_w = col_offs[col + 1] - col_offs[col];
            let cell_rect = cell.borrow_mut().layout_content(0, 0, cell_w, i32::MAX / 4, font, scale);
            let h = cell_rect.height();
            measured_heights.push(h);
            row_h = row_h.max(h);
            if col == n_cols - 1 {
                row_heights.push(row_h);
                row_h = 0;
            }
        }
        if cells.len() % n_cols != 0 {
            row_heights.push(row_h);
        }

        // Build cumulative row offsets.
        let mut row_offsets: Vec<i32> = Vec::with_capacity(row_heights.len() + 1);
        row_offsets.push(0);
        let mut acc = 0i32;
        for h in &row_heights { acc += *h; row_offsets.push(acc); }

        // Pass 2: position each cell at its grid-local (col_x, row_y). We
        // span the full column width (so mouse hit-testing covers the cell
        // visually), but keep the cell's *own* measured height — short cells
        // in a tall row top-align within the row, which is what tables do.
        for (idx, cell) in cells.iter().enumerate() {
            let row = idx / n_cols;
            let col = idx % n_cols;
            if row >= row_heights.len() { break; }
            let cell_x = padding.left + col_offs[col];
            let cell_y = padding.top + row_offsets[row];
            let cell_w = col_offs[col + 1] - col_offs[col];
            let cell_h = measured_heights[idx];
            cell.borrow_mut().set_rect(rect((cell_x, cell_y), (cell_x + cell_w, cell_y + cell_h)));
        }

        *self.row_heights.borrow_mut() = row_heights;
        *self.row_offsets.borrow_mut() = row_offsets;
    }

    fn fire_click(&self, ui: &mut UI) {
        self.base_fire_event(ui, EventType::Click, &EventData::None);
    }
}

impl View for Grid {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) { return; }
        match name {
            "widths" => {
                let parts: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
                let mut ws: Vec<ColumnWidth> = Vec::with_capacity(parts.len());
                for p in &parts {
                    if let Some(w) = parse_column_width(p) { ws.push(w); }
                }
                *self.widths.borrow_mut() = ws;
                self.needs_relayout.set(true);
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
        let effective = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(effective.clone()));
        self.base_set_scale(scale);

        let padding = self.get_padding(scale);
        let h_pad = padding.left + padding.right;
        let v_pad = padding.top + padding.bottom;

        let (mut new_w, mut new_h) = self.calculate_size(width, height, scale);
        // Min dimensions: derive from content after columns/rows are resolved.
        // First do a column-resolve at the available width (minus padding) to
        // know cell widths.
        let avail_w = if matches!(self.state.borrow().width, Dimension::Min) {
            width.max(0)
        } else {
            new_w.max(0)
        };
        self.relayout_columns((avail_w - h_pad).max(0), scale);
        self.layout_cells(scale, &effective);

        // Compute content-derived size for Min sizing. Cells live inside the
        // padded content box, so the grid's own Min size includes padding.
        let content_w: i32 = self.column_offsets.borrow().last().copied().unwrap_or(0);
        let content_h: i32 = self.row_offsets.borrow().last().copied().unwrap_or(0);
        if matches!(self.state.borrow().width, Dimension::Min) { new_w = content_w + h_pad; }
        if matches!(self.state.borrow().height, Dimension::Min) { new_h = content_h + v_pad; }

        let r = rect((x, y), (x + new_w, y + new_h));
        self.set_rect(r);
        self.needs_relayout.set(false);
        r
    }

    fn fits_in_rect(&self, w: i32, h: i32, _scale: f64) -> bool {
        let r = self.get_rect();
        r.width() <= w && r.height() <= h
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let r = self.state.borrow().rect;
        let abs = Point { x: r.min.x + origin.x, y: r.min.y + origin.y };

        // Optional background fill if configured via `background` attribute.
        if let Some(bg) = self.base_get_background() {
            let mut full = r;
            full.move_by(origin);
            theme.draw_rect(full, bg);
        }

        // Cells' rects are in grid-local coords, so the origin we pass is
        // the grid's screen-space top-left.
        for cell in self.cells.borrow().iter() {
            cell.borrow().paint(abs, theme);
        }
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
        let scale = self.state.borrow().scale;
        let padding = self.get_padding(scale);
        let w = self.column_offsets.borrow().last().copied().unwrap_or(0) + padding.left + padding.right;
        let h = self.row_offsets.borrow().last().copied().unwrap_or(0) + padding.top + padding.bottom;
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

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.fire_click(ui);
        true
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        if self.needs_relayout.replace(false) {
            let r = self.get_rect();
            let scale = self.state.borrow().scale;
            let typeface = self.state.borrow().font_manager.get().unwrap_or_default();
            let padding = self.get_padding(scale);
            let h_pad = padding.left + padding.right;
            // Refresh inner layout against the cached rect (safe — Min/Max
            // dimensions are evaluated only inside `layout_content`, not here).
            self.relayout_columns((r.width() - h_pad).max(0), scale);
            self.layout_cells(scale, &typeface);
            return true;
        }
        false
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        let r = self.state.borrow().rect;
        let local = Point::new(position.x - r.min.x, position.y - r.min.y);
        let mut processed = false;
        for cell in self.cells.borrow().iter().rev() {
            let vis = { let cb = cell.borrow(); cb.get_visibility() == Visibility::Visible && cb.is_enabled() };
            if !vis { continue; }
            processed |= cell.borrow().on_mouse_move(ui, local);
        }
        processed
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) { return false; }
        let local = Point::new(position.x - r.min.x, position.y - r.min.y);
        for cell in self.cells.borrow().iter().rev() {
            let vis = { let cb = cell.borrow(); cb.get_visibility() == Visibility::Visible && cb.is_enabled() };
            if !vis { continue; }
            if cell.borrow().on_mouse_button_down(ui, local, button) {
                return true;
            }
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        let r = self.state.borrow().rect;
        let local = Point::new(position.x - r.min.x, position.y - r.min.y);
        for cell in self.cells.borrow().iter().rev() {
            let vis = { let cb = cell.borrow(); cb.get_visibility() == Visibility::Visible && cb.is_enabled() };
            if !vis { continue; }
            if cell.borrow().on_mouse_button_up(ui, local, button) {
                return true;
            }
        }
        false
    }
}

impl Container for Grid {
    fn add_view(&mut self, view: Element) {
        self.cells.borrow_mut().push(view);
        self.needs_relayout.set(true);
    }

    fn get_view(&self, id: &str) -> Option<Element> {
        for cell in self.cells.borrow().iter() {
            if cell.borrow().get_id() == id { return Some(cell.clone()); }
            if let Some(c) = cell.borrow().as_container() {
                if let Some(found) = c.get_view(id) { return Some(found); }
            }
        }
        None
    }

    fn get_view_count(&self) -> usize { self.cells.borrow().len() }

    fn get_views(&self) -> Vec<Element> { self.cells.borrow().clone() }

    fn remove_view(&mut self, id: &str) -> bool {
        let pos = self.cells.borrow().iter().position(|c| c.borrow().get_id() == id);
        if let Some(p) = pos {
            self.cells.borrow_mut().remove(p);
            self.needs_relayout.set(true);
            return true;
        }
        for cell in self.cells.borrow().iter() {
            if let Some(container) = cell.borrow_mut().as_container_mut() {
                if container.remove_view(id) {
                    self.needs_relayout.set(true);
                    return true;
                }
            }
        }
        false
    }
}

impl Default for Grid {
    fn default() -> Self {
        Grid::new(rect((0, 0), (200, 100)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid() -> Grid { Grid::new(rect((0, 0), (300, 100))) }

    #[test]
    fn widths_attribute_resolves_columns() {
        let mut g = make_grid();
        g.set_any("widths", "100,*,50");
        assert_eq!(g.column_count(), 3);
        g.relayout_columns(300, 1.0);
        let offs = g.column_offsets.borrow();
        // Fixed 100, Star = 300-150 = 150, Fixed 50.
        assert_eq!(offs[0], 0);
        assert_eq!(offs[1], 100);
        assert_eq!(offs[2], 250);
        assert_eq!(offs[3], 300);
    }

    #[test]
    fn content_size_includes_padding() {
        let mut g = make_grid();
        g.set_scale(1.0);
        g.set_padding(4, 8, 8, 4); // top, left, right, bottom
        // Seed resolved offsets directly (avoids needing a font for layout).
        *g.column_offsets.borrow_mut() = vec![0, 100, 250];
        *g.row_offsets.borrow_mut() = vec![0, 30, 60];
        let (w, h) = g.get_content_size();
        assert_eq!(w, 250 + 8 + 8); // content width + left + right
        assert_eq!(h, 60 + 4 + 4);  // content height + top + bottom
    }

    #[test]
    fn row_count_partial_last_row() {
        let g = make_grid();
        g.set_widths(vec![ColumnWidth::Fixed(50); 3]);
        // 5 cells in 3 columns → 2 rows (the last with one cell).
        for _ in 0..5 {
            let lbl = crate::views::Label::new(rect((0, 0), (0, 0)), "x", 14.0);
            // Wrapping in Rc<RefCell> mirrors how the parser feeds children.
            let elem: Element = std::rc::Rc::new(std::cell::RefCell::new(lbl));
            // SAFETY: we know set_widths set 3 columns.
            let mut cells = g.cells.borrow_mut();
            cells.push(elem);
        }
        assert_eq!(g.row_count(), 2);
    }
}
