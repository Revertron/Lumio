use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::text::{TextBlock, TextOptions};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::image_source::ImageSource;
use crate::themes::{Renderer, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, point, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};

const MIN_THUMB_SIZE: i32 = 16;
/// Distance in dip between the outer border and the inner content, so the
/// 2-pixel `edit.body` border sits cleanly outside the items.
const BORDER_INSET_DIP: i32 = 2;
const DEFAULT_ROW_PAD_V: i32 = 3;
const DEFAULT_ITEM_PAD_H: i32 = 6;
const DEFAULT_ICON_SIZE_DIP: i32 = 16;
const DEFAULT_MAX_ITEM_WIDTH_DIP: i32 = 220;
const MIN_ITEM_WIDTH_DIP: i32 = 60;
const ICON_TEXT_GAP_DIP: i32 = 4;

/// One entry of an [`IconList`].
#[derive(Clone, Debug, Default)]
pub struct IconListItem {
    /// Text shown next to the icon.
    pub text: String,
    /// Optional icon asset path (PNG or SVG).
    pub icon: Option<String>,
    /// ARGB multiplier for the icon; `None` uses the palette's `icon_tint`.
    pub tint: Option<u32>,
    /// App-defined payload (e.g. a full file path); the widget never
    /// interprets it.
    pub key: String,
}

impl IconListItem {
    /// Convenience constructor for the common case.
    pub fn new(text: &str, icon: &str, tint: u32, key: &str) -> IconListItem {
        IconListItem {
            text: text.to_owned(),
            icon: if icon.is_empty() { None } else { Some(icon.to_owned()) },
            tint: Some(tint),
            key: key.to_owned(),
        }
    }
}

/// An Explorer-"List"-mode item view: small icon + text label per item,
/// items flowing top-to-bottom and wrapping into new columns, with a
/// horizontal scrollbar when the columns overflow. Multi-select: plain
/// click selects one item, Ctrl+Click toggles, Shift+Click selects a range
/// (Explorer semantics), and arrow keys navigate (Shift extends).
///
/// Selection changes fire [`EventType::SelectionChanged`] with
/// `EventData::Selected(index)` (`EventData::None` when cleared by an
/// empty-area click); read the full set via
/// [`selected_indices`](IconList::selected_indices). A `DoubleClick`
/// listener works out of the box (central dispatch) — resolve the item under
/// the `Position` payload with [`item_at`](IconList::item_at).
///
/// Items are provided programmatically via [`set_items`](IconList::set_items)
/// (no XML child tags). XML attributes: `font_size`, `row_height` (dip),
/// `icon_size` (dip), `item_width` (max column width in dip).
pub struct IconList {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<IconListItem>>,
    /// One laid-out text block per item; built in `inner_relayout` so the
    /// cache always matches the current scale.
    blocks: RefCell<Vec<Option<TextBlock>>>,
    icons: RefCell<HashMap<String, ImageSource>>,

    selected: RefCell<HashSet<usize>>,
    /// Shift-range origin.
    anchor: Cell<Option<usize>>,
    /// Caret item — the last clicked / keyboard-navigated index.
    lead: Cell<Option<usize>>,

    scroll_x: Cell<i32>, // <= 0
    h_scroll_visible: Cell<bool>,
    dragging_thumb: Cell<bool>,
    drag_anchor_x: Cell<i32>,
    drag_anchor_scroll: Cell<i32>,

    row_height_dip: Cell<Option<i32>>,
    icon_size_dip: Cell<i32>,
    max_item_width_dip: Cell<i32>,
    // Resolved by `inner_relayout`:
    row_height_px: Cell<i32>,
    item_width_px: Cell<i32>,
    rows_per_col: Cell<usize>,
    needs_relayout: Cell<bool>,
}

impl HasMainFields for IconList {
    fn main_fields(&self) -> &RefCell<FieldsMain> { &self.state }
}
impl ViewBasics for IconList {}

#[allow(dead_code)]
impl IconList {
    pub fn new(rect_: Rect<i32>) -> IconList {
        let mut main = FieldsMain::with_rect(rect_, Dimension::Min, Dimension::Min);
        main.state.focusable = true;
        IconList {
            state: RefCell::new(main),
            items: RefCell::new(Vec::new()),
            blocks: RefCell::new(Vec::new()),
            icons: RefCell::new(HashMap::new()),
            selected: RefCell::new(HashSet::new()),
            anchor: Cell::new(None),
            lead: Cell::new(None),
            scroll_x: Cell::new(0),
            h_scroll_visible: Cell::new(false),
            dragging_thumb: Cell::new(false),
            drag_anchor_x: Cell::new(0),
            drag_anchor_scroll: Cell::new(0),
            row_height_dip: Cell::new(None),
            icon_size_dip: Cell::new(DEFAULT_ICON_SIZE_DIP),
            max_item_width_dip: Cell::new(DEFAULT_MAX_ITEM_WIDTH_DIP),
            row_height_px: Cell::new(0),
            item_width_px: Cell::new(0),
            rows_per_col: Cell::new(1),
            needs_relayout: Cell::new(false),
        }
    }

    // --- Public API ---

    /// Replace all items. Clears the selection and resets the scroll.
    pub fn set_items(&self, items: Vec<IconListItem>) {
        *self.items.borrow_mut() = items;
        self.selected.borrow_mut().clear();
        self.anchor.set(None);
        self.lead.set(None);
        self.scroll_x.set(0);
        self.needs_relayout.set(true);
    }

    pub fn item_count(&self) -> usize {
        self.items.borrow().len()
    }

    /// A clone of the item at `index`.
    pub fn item(&self, index: usize) -> Option<IconListItem> {
        self.items.borrow().get(index).cloned()
    }

    /// Sorted indices of all selected items.
    pub fn selected_indices(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.selected.borrow().iter().copied().collect();
        v.sort_unstable();
        v
    }

    /// The caret item — the index clicked or keyboard-navigated last.
    pub fn last_selected(&self) -> Option<usize> {
        self.lead.get()
    }

    /// Item index at a window position (e.g. the `Position` payload of a
    /// `DoubleClick` event), or None over empty space / chrome.
    pub fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let item_w = self.item_width_px.get().max(1);
        let row_h = self.row_height_px.get().max(1);
        let rows = self.rows_per_col.get().max(1);
        let local_x = x - r.min.x - inset - self.scroll_x.get();
        let local_y = y - r.min.y - inset;
        if local_x < 0 || local_y < 0 || local_y >= self.body_height() { return None; }
        let row = (local_y / row_h) as usize;
        if row >= rows { return None; }
        let col = (local_x / item_w) as usize;
        let idx = col * rows + row;
        if idx < self.items.borrow().len() { Some(idx) } else { None }
    }

    /// Make `index` the only selected item (programmatic; fires no event).
    pub fn select_only(&self, index: usize) {
        if index >= self.items.borrow().len() { return; }
        let mut sel = self.selected.borrow_mut();
        sel.clear();
        sel.insert(index);
        drop(sel);
        self.anchor.set(Some(index));
        self.lead.set(Some(index));
        self.ensure_visible(index);
    }

    /// Deselect everything (programmatic; fires no event).
    pub fn clear_selection(&self) {
        self.selected.borrow_mut().clear();
        self.anchor.set(None);
        self.lead.set(None);
    }

    // --- Internals ---

    /// Explorer selection semantics for a click on `idx`.
    fn apply_click(&self, idx: usize, ctrl: bool, shift: bool) {
        if shift {
            let a = self.anchor.get().unwrap_or(idx);
            let (lo, hi) = (a.min(idx), a.max(idx));
            let mut sel = self.selected.borrow_mut();
            if !ctrl { sel.clear(); } // Ctrl+Shift adds the range to the selection
            sel.extend(lo..=hi);
        } else if ctrl {
            let mut sel = self.selected.borrow_mut();
            if !sel.remove(&idx) { sel.insert(idx); }
            self.anchor.set(Some(idx));
        } else {
            let mut sel = self.selected.borrow_mut();
            sel.clear();
            sel.insert(idx);
            self.anchor.set(Some(idx));
        }
        self.lead.set(Some(idx));
    }

    fn inner_relayout(&self, scale: f64, typeface: &Typeface) {
        let base_size = typeface.font_size
            .unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let icon_size = (self.icon_size_dip.get() as f64 * scale).round() as i32;
        let row_h = match self.row_height_dip.get() {
            Some(dip) => (dip as f64 * scale).round() as i32,
            None => {
                let line = (base_size * scale as f32).ceil() as i32;
                let pad = (DEFAULT_ROW_PAD_V as f64 * scale).round() as i32 * 2;
                line.max(icon_size) + pad
            }
        };
        self.row_height_px.set(row_h);

        let mut blocks = Vec::new();
        let mut max_text_w = 0i32;
        let font = get_font_family(&typeface.font_name, typeface.font_style);
        for item in self.items.borrow().iter() {
            let block = font.as_ref().map(|f| {
                f.layout_text(&item.text, base_size * scale as f32, TextOptions::new())
            });
            if let Some(b) = &block {
                max_text_w = max_text_w.max(b.width().ceil() as i32);
            }
            blocks.push(block);
        }
        *self.blocks.borrow_mut() = blocks;

        // One uniform column width (classic Windows List mode), clamped.
        let pad_h = (DEFAULT_ITEM_PAD_H as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP_DIP as f64 * scale).round() as i32;
        let natural = pad_h + icon_size + gap + max_text_w + pad_h;
        let min_w = (MIN_ITEM_WIDTH_DIP as f64 * scale).round() as i32;
        let max_w = (self.max_item_width_dip.get() as f64 * scale).round() as i32;
        self.item_width_px.set(natural.clamp(min_w, max_w.max(min_w)));

        // Column flow, with a one-iteration scrollbar recompute: if the
        // columns overflow, the h scrollbar eats body height, which reduces
        // rows per column and only adds columns — the bar stays needed.
        let n = self.items.borrow().len() as i32;
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let body_w = (r.width() - 2 * inset).max(0);
        let full_h = (r.height() - 2 * inset).max(0);
        let rows = (full_h / row_h.max(1)).max(1);
        let cols = (n + rows - 1) / rows.max(1);
        let overflow = cols * self.item_width_px.get() > body_w;
        self.h_scroll_visible.set(overflow);
        let body_h = if overflow { (full_h - self.scrollbar_thickness()).max(0) } else { full_h };
        self.rows_per_col.set(((body_h / row_h.max(1)).max(1)) as usize);

        self.clamp_scroll();
    }

    fn border_inset(&self) -> i32 {
        let scale = self.state.borrow().scale;
        (BORDER_INSET_DIP as f64 * scale).round() as i32
    }

    fn scrollbar_thickness(&self) -> i32 {
        let scale = self.state.borrow().scale;
        (crate::drawing::current_dimension("scrollbar.thickness") as f64 * scale).round() as i32
    }

    fn body_width(&self) -> i32 {
        let r = self.state.borrow().rect;
        (r.width() - 2 * self.border_inset()).max(0)
    }

    fn body_height(&self) -> i32 {
        let r = self.state.borrow().rect;
        let mut h = r.height() - 2 * self.border_inset();
        if self.h_scroll_visible.get() { h -= self.scrollbar_thickness(); }
        h.max(0)
    }

    fn col_count(&self) -> i32 {
        let n = self.items.borrow().len() as i32;
        let rows = self.rows_per_col.get().max(1) as i32;
        (n + rows - 1) / rows
    }

    fn content_width(&self) -> i32 {
        self.col_count() * self.item_width_px.get()
    }

    fn clamp_scroll(&self) {
        let max_neg = -(self.content_width() - self.body_width()).max(0);
        let x = self.scroll_x.get().clamp(max_neg, 0);
        self.scroll_x.set(x);
    }

    /// Item rect in widget-local coordinates (before the border inset shift).
    fn item_rect_local(&self, idx: usize) -> Rect<i32> {
        let rows = self.rows_per_col.get().max(1);
        let item_w = self.item_width_px.get();
        let row_h = self.row_height_px.get();
        let col = (idx / rows) as i32;
        let row = (idx % rows) as i32;
        let x = col * item_w;
        let y = row * row_h;
        rect((x, y), (x + item_w, y + row_h))
    }

    fn ensure_visible(&self, idx: usize) {
        let item = self.item_rect_local(idx);
        let bw = self.body_width();
        let cur = self.scroll_x.get();
        if item.min.x + cur < 0 {
            self.scroll_x.set(-item.min.x);
        } else if item.max.x + cur > bw {
            self.scroll_x.set(bw - item.max.x);
        }
        self.clamp_scroll();
    }

    fn fire_selection(&self, ui: &mut UI, data: &EventData) {
        self.base_fire_event(ui, EventType::SelectionChanged, data);
    }

    // Scrollbar geometry (horizontal only), matching the TableView chrome.

    fn h_scrollbar_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let thickness = self.scrollbar_thickness();
        let x_min = r.min.x + origin.x + inset;
        let x_max = r.min.x + origin.x + r.width() - inset;
        let y_max = r.min.y + origin.y + r.height() - inset;
        let y_min = y_max - thickness;
        rect((x_min, y_min), (x_max, y_max))
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

    fn h_track_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.h_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        if sb.width() < 2 * size {
            return rect((sb.min.x, sb.min.y), (sb.min.x, sb.max.y));
        }
        rect((sb.min.x + size, sb.min.y), (sb.max.x - size, sb.max.y))
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

    fn h_track_length(&self) -> i32 {
        let bw = self.body_width();
        let t = self.scrollbar_thickness();
        if bw < 2 * t { 0 } else { bw - 2 * t }
    }
}

impl View for IconList {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) { return; }
        match name {
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.state.borrow_mut().font_manager.set_font_size(size);
                }
            }
            "font" => { self.state.borrow_mut().font_manager.set_font(value); }
            "font_style" => { self.state.borrow_mut().font_manager.set_font_style(value); }
            "row_height" => {
                if let Ok(h) = value.parse::<i32>() {
                    self.row_height_dip.set(Some(h));
                }
            }
            "icon_size" => {
                if let Ok(s) = value.parse::<i32>() {
                    self.icon_size_dip.set(s);
                }
            }
            "item_width" => {
                if let Ok(w) = value.parse::<i32>() {
                    self.max_item_width_dip.set(w);
                }
            }
            _ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) { self.base_set_parent(parent); }
    fn get_parent(&self) -> Option<Element> { self.base_get_parent() }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        let effective = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(effective.clone()));
        self.base_set_scale(scale);

        let (mut new_w, mut new_h) = self.calculate_size(width, height, scale);
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

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Renderer) {
        let mut r = self.state.borrow().rect;
        r.move_by(origin);

        theme.push_clip();
        theme.clip_rect(r);

        let main_state = self.state.borrow().state;
        // A 9-patch background replaces the back and the body components;
        // scrollbars stay drawable-based.
        let ninepatch = self.base_draw_ninepatch(theme, r);
        if !ninepatch {
            theme.draw_component("edit.back", r, main_state);
        }

        let inset = self.border_inset();
        let scale = self.state.borrow().scale;
        let row_h = self.row_height_px.get();
        let item_w = self.item_width_px.get().max(1);
        let rows = self.rows_per_col.get().max(1);
        let bw = self.body_width();
        let bh = self.body_height();
        let scroll_x = self.scroll_x.get();
        let icon_size = (self.icon_size_dip.get() as f64 * scale).round() as i32;
        let pad_h = (DEFAULT_ITEM_PAD_H as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP_DIP as f64 * scale).round() as i32;

        let body_origin = Point { x: r.min.x + inset, y: r.min.y + inset };
        let body_clip = rect((body_origin.x, body_origin.y), (body_origin.x + bw, body_origin.y + bh));

        theme.push_clip();
        theme.clip_rect(body_clip);

        let items = self.items.borrow();
        let blocks = self.blocks.borrow();
        let selected = self.selected.borrow();
        let n = items.len();
        let first_col = ((-scroll_x) / item_w).max(0) as usize;
        let last_col = (((bw - scroll_x).max(0) / item_w) as usize + 1).min(self.col_count() as usize);
        let lead = self.lead.get();
        let focused = main_state.focused;

        for col in first_col..last_col {
            for row in 0..rows {
                let idx = col * rows + row;
                if idx >= n { break; }
                let item = &items[idx];
                let x0 = body_origin.x + col as i32 * item_w + scroll_x;
                let y0 = body_origin.y + row as i32 * row_h;
                let item_rect = rect((x0, y0), (x0 + item_w, y0 + row_h));

                let mut text_color = theme.color("text");
                if selected.contains(&idx) {
                    theme.draw_rect(item_rect, theme.color("item_highlight"));
                    text_color = theme.color("item_highlight_text");
                }
                if focused && lead == Some(idx) {
                    let width = (scale.round() as i32).max(1);
                    theme.draw_rect_outline(item_rect, theme.color("focus"), width);
                }

                let mut x = x0 + pad_h;
                if let Some(icon_path) = &item.icon {
                    let icon_rect = rect(
                        (x, y0 + (row_h - icon_size) / 2),
                        (x + icon_size, y0 + (row_h - icon_size) / 2 + icon_size),
                    );
                    let tint = item.tint.unwrap_or_else(|| theme.color("icon_tint"));
                    let mut icons = self.icons.borrow_mut();
                    let icon = icons.entry(icon_path.clone())
                        .or_insert_with(|| ImageSource::new(icon_path));
                    icon.draw(theme, icon_rect, tint);
                }
                x += icon_size + gap;

                if let Some(Some(block)) = blocks.get(idx) {
                    let th = block.height().ceil() as i32;
                    let ty = y0 + (row_h - th) / 2;
                    // Crop long names to the item box so they can't bleed
                    // into the neighbouring column.
                    let crop = rect((item_rect.min.x, item_rect.min.y), (item_rect.max.x - pad_h, item_rect.max.y));
                    theme.draw_text_cropped(x as f32, ty as f32, crop, text_color, block);
                }
            }
        }
        drop(items);
        drop(blocks);
        drop(selected);

        theme.pop_clip();

        // ---- Scrollbar ----
        if self.h_scroll_visible.get() {
            let unfocused = ViewState::no_focus();
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
            s.pressed = self.dragging_thumb.get();
            s.focused = false; // no focus ring on scrollbar thumbs
            theme.draw_component("button.back", thumb, s);
            theme.draw_component("button.body", thumb, s);
        }

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
        (self.content_width().max(200), 150)
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
        let mut node = accesskit::Node::new(accesskit::Role::ListBox);
        node.set_multiselectable();
        node
    }

    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        let id = self.get_id();
        let inset = self.border_inset();
        let scroll_x = self.scroll_x.get();
        let selected = self.selected.borrow();
        let mut result = Vec::new();
        for (i, item) in self.items.borrow().iter().enumerate() {
            let mut node = accesskit::Node::new(accesskit::Role::ListBoxOption);
            node.set_label(item.text.clone());
            node.set_selected(selected.contains(&i));
            node.add_action(accesskit::Action::Click);
            let r = self.item_rect_local(i);
            // View-local; the tree builder translates to window space.
            node.set_bounds(accesskit::Rect {
                x0: (inset + r.min.x + scroll_x) as f64,
                y0: (inset + r.min.y) as f64,
                x1: (inset + r.max.x + scroll_x) as f64,
                y1: (inset + r.max.y) as f64,
            });
            result.push((crate::accessibility::item_node_id(&id, i), node));
        }
        result
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.base_fire_event(ui, EventType::Click, &EventData::None);
        true
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        if self.needs_relayout.replace(false) {
            // Self-heal: refresh inner layout against the cached rect.
            // Do NOT call `layout_content` — that would re-apply
            // `Dimension::Percent` and shrink the view every tick.
            let scale = self.state.borrow().scale;
            let typeface = self.state.borrow().font_manager.get().unwrap_or_default();
            self.inner_relayout(scale, &typeface);
            return true;
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Point<i32>) -> bool {
        if !self.dragging_thumb.get() { return false; }
        let r = self.state.borrow().rect;
        let local_x = position.x - r.min.x;
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
        true
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) { return false; }
        if !matches!(button, MouseButton::Left) { return false; }

        self.state.borrow_mut().state.focused = true;

        if self.h_scroll_visible.get() {
            let thumb = self.h_thumb_rect(point(0, 0));
            if thumb.hit((position.x, position.y)) {
                self.dragging_thumb.set(true);
                self.drag_anchor_x.set(position.x - r.min.x);
                self.drag_anchor_scroll.set(self.scroll_x.get());
                return true;
            }
            let step = self.item_width_px.get().max(20);
            if self.h_arrow_left_rect(point(0, 0)).hit((position.x, position.y)) {
                self.scroll_x.set(self.scroll_x.get() + step);
                self.clamp_scroll();
                return true;
            }
            if self.h_arrow_right_rect(point(0, 0)).hit((position.x, position.y)) {
                self.scroll_x.set(self.scroll_x.get() - step);
                self.clamp_scroll();
                return true;
            }
            let sb = self.h_scrollbar_rect(point(0, 0));
            if sb.hit((position.x, position.y)) {
                // Track click between thumb and arrows: page-scroll toward it.
                let bw = self.body_width();
                let dir = if position.x < thumb.min.x { 1 } else { -1 };
                self.scroll_x.set(self.scroll_x.get() + dir * bw);
                self.clamp_scroll();
                return true;
            }
        }

        let (ctrl, shift) = {
            let m = ui.modifiers();
            (m.ctrl(), m.shift())
        };
        match self.item_at(position.x, position.y) {
            Some(idx) => {
                self.apply_click(idx, ctrl, shift);
                self.ensure_visible(idx);
                self.fire_selection(ui, &EventData::Selected(idx));
            }
            None => {
                if !ctrl && !shift && !self.selected.borrow().is_empty() {
                    self.clear_selection();
                    self.fire_selection(ui, &EventData::None);
                }
            }
        }
        true
    }

    fn on_mouse_button_up(&self, _ui: &mut UI, _position: Point<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) { return false; }
        if self.dragging_thumb.replace(false) {
            return true;
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) { return false; }
        let step = self.item_width_px.get().max(20);
        let bw = self.body_width();
        // Vertical wheel scrolls horizontally — Explorer List-mode behavior.
        let dx = match distance {
            MouseScrollDistance::Lines { x, y, .. } => (x + y) as i32 * step,
            MouseScrollDistance::Pixels { x, y, .. } => (x + y) as i32,
            MouseScrollDistance::Pages { x, y, .. } => (x + y) as i32 * bw,
        };
        self.scroll_x.set(self.scroll_x.get() + dx);
        self.clamp_scroll();
        true
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, state: ModifiersState) -> bool {
        if !self.base_is_focused() { return false; }
        let Some(code) = virtual_key_code else { return false; };
        if code == VirtualKeyCode::Tab { return false; }
        let n = self.items.borrow().len();
        if n == 0 { return false; }
        let rows = self.rows_per_col.get().max(1);
        let cur = self.lead.get();

        let new_idx = match code {
            VirtualKeyCode::Up => Some(cur.map(|i| i.saturating_sub(1)).unwrap_or(0)),
            VirtualKeyCode::Down => Some(cur.map(|i| (i + 1).min(n - 1)).unwrap_or(0)),
            VirtualKeyCode::Left => Some(cur.map(|i| i.saturating_sub(rows)).unwrap_or(0)),
            VirtualKeyCode::Right => Some(cur.map(|i| (i + rows).min(n - 1)).unwrap_or(0)),
            VirtualKeyCode::Home => Some(0),
            VirtualKeyCode::End => Some(n - 1),
            _ => None,
        };
        if let Some(idx) = new_idx {
            self.apply_click(idx, false, state.shift());
            self.ensure_visible(idx);
            self.fire_selection(ui, &EventData::Selected(idx));
            return true;
        }
        false
    }
}

impl Default for IconList {
    fn default() -> Self {
        IconList::new(rect((0, 0), (300, 200)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_list(n: usize) -> IconList {
        let list = IconList::default();
        let items = (0..n)
            .map(|i| IconListItem::new(&format!("item {i}"), "", 0xFFFFFFFF, &format!("k{i}")))
            .collect();
        list.set_items(items);
        list
    }

    /// Set the resolved flow geometry directly (unit tests run without fonts,
    /// so `inner_relayout` can't produce text blocks).
    fn set_flow(list: &IconList, rows: usize, item_w: i32, row_h: i32) {
        list.rows_per_col.set(rows);
        list.item_width_px.set(item_w);
        list.row_height_px.set(row_h);
    }

    #[test]
    fn column_flow_math() {
        let list = make_list(10);
        set_flow(&list, 4, 100, 20);
        assert_eq!(list.col_count(), 3); // 4 + 4 + 2
        assert_eq!(list.content_width(), 300);

        let r = list.item_rect_local(5); // col 1, row 1
        assert_eq!((r.min.x, r.min.y), (100, 20));

        set_flow(&list, 4, 100, 20);
        let empty = make_list(0);
        set_flow(&empty, 4, 100, 20);
        assert_eq!(empty.col_count(), 0);
        assert_eq!(empty.content_width(), 0);
    }

    #[test]
    fn hit_test_round_trip() {
        let list = make_list(10);
        set_flow(&list, 4, 100, 20);
        // rect is (0,0)-(300,200) from Default; inset is 0 at scale 0 rounding?
        // Force a known scale so border_inset is deterministic.
        list.state.borrow_mut().scale = 1.0;
        let inset = list.border_inset();
        for idx in 0..10usize {
            let r = list.item_rect_local(idx);
            let x = inset + r.min.x + 1;
            let y = inset + r.min.y + 1;
            assert_eq!(list.item_at(x, y), Some(idx), "idx {idx}");
        }
        // Below the last row of the final partial column → None.
        let r = list.item_rect_local(9); // col 2, row 1
        assert_eq!(list.item_at(inset + r.min.x + 1, inset + r.max.y + 21), None);
    }

    #[test]
    fn apply_click_matrix() {
        let list = make_list(10);

        // Plain click selects one.
        list.apply_click(3, false, false);
        assert_eq!(list.selected_indices(), vec![3]);
        assert_eq!(list.last_selected(), Some(3));

        // Ctrl+Click toggles on.
        list.apply_click(5, true, false);
        assert_eq!(list.selected_indices(), vec![3, 5]);

        // Ctrl+Click toggles off.
        list.apply_click(3, true, false);
        assert_eq!(list.selected_indices(), vec![5]);

        // Shift+Click selects anchor..=idx (anchor = 3 from the last ctrl-click).
        list.apply_click(7, false, true);
        assert_eq!(list.selected_indices(), vec![3, 4, 5, 6, 7]);

        // Plain click resets.
        list.apply_click(0, false, false);
        assert_eq!(list.selected_indices(), vec![0]);

        // Ctrl+Shift adds a range without clearing.
        list.apply_click(8, true, false);
        list.apply_click(9, true, true);
        assert_eq!(list.selected_indices(), vec![0, 8, 9]);
    }
}
