use std::cell::{Cell, RefCell};
use std::collections::HashMap;

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
/// 2-pixel `edit.body` border sits cleanly outside the rows.
const BORDER_INSET_DIP: i32 = 2;
const DEFAULT_ROW_PAD_V: i32 = 3;
const DEFAULT_INDENT_DIP: i32 = 16;
const DEFAULT_ICON_SIZE_DIP: i32 = 16;
const ICON_TEXT_GAP_DIP: i32 = 4;

/// One node of a [`TreeView`]. The tree is managed by the application:
/// set a small initial tree with [`TreeView::set_roots`], then react to
/// [`EventType::Expanded`] and grow the branch with
/// [`TreeView::set_children`] (lazy loading).
#[derive(Clone, Debug, Default)]
pub struct TreeNode {
    /// Text shown for the node.
    pub text: String,
    /// Optional icon asset path (PNG or SVG), drawn left of the text.
    pub icon: Option<String>,
    /// ARGB multiplier for the icon; `None` uses the palette's `icon_tint`.
    pub tint: Option<u32>,
    /// App-defined stable id, unique within the tree (e.g. a full file path).
    /// All by-key APIs ([`TreeView::set_children`], [`TreeView::select_key`],
    /// ...) address nodes through it.
    pub key: String,
    /// Show an expand chevron even while `children` is still empty — the
    /// signal for "children unknown, load them on expand".
    pub has_children: bool,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Convenience constructor for the common case.
    pub fn new(text: &str, key: &str, has_children: bool) -> TreeNode {
        TreeNode {
            text: text.to_owned(),
            key: key.to_owned(),
            has_children,
            ..TreeNode::default()
        }
    }
}

/// One visible row of the flattened tree.
#[derive(Clone, Debug)]
struct FlatRow {
    key: String,
    text: String,
    icon: Option<String>,
    tint: Option<u32>,
    depth: usize,
    has_children: bool,
    expanded: bool,
}

fn find_node<'a>(nodes: &'a [TreeNode], key: &str) -> Option<&'a TreeNode> {
    for n in nodes {
        if n.key == key { return Some(n); }
        if let Some(found) = find_node(&n.children, key) { return Some(found); }
    }
    None
}

fn find_node_mut<'a>(nodes: &'a mut [TreeNode], key: &str) -> Option<&'a mut TreeNode> {
    for n in nodes {
        if n.key == key { return Some(n); }
        if let Some(found) = find_node_mut(&mut n.children, key) { return Some(found); }
    }
    None
}

/// Child-index path from the roots to the node with `key`, or None.
fn find_path(nodes: &[TreeNode], key: &str, path: &mut Vec<usize>) -> bool {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        if n.key == key { return true; }
        if find_path(&n.children, key, path) { return true; }
        path.pop();
    }
    false
}

/// A hierarchical tree of expandable nodes (e.g. a directory tree) with
/// single selection, keyboard navigation and a vertical scrollbar.
///
/// The node data lives in the application: populate with
/// [`set_roots`](TreeView::set_roots), listen for [`EventType::Expanded`]
/// (read the node via [`expanded_key`](TreeView::expanded_key)) and append
/// children with [`set_children`](TreeView::set_children) — nodes created
/// with `has_children: true` show a chevron before any children are loaded.
/// Selection fires [`EventType::SelectionChanged`] with
/// `EventData::Selected(visible_row)`; read the node via
/// [`selected_key`](TreeView::selected_key).
///
/// Nodes are provided programmatically (no XML child tags). XML attributes:
/// `font_size`, `row_height` (dip), `icon_size` (dip), `indent` (dip per
/// depth level).
pub struct TreeView {
    state: RefCell<FieldsMain>,
    roots: RefCell<Vec<TreeNode>>,
    /// Flattened visible rows, rebuilt by `rebuild_flat` after every mutation.
    flat: RefCell<Vec<FlatRow>>,
    /// One laid-out text block per flat row; built in `inner_relayout` so the
    /// cache always matches the current scale.
    blocks: RefCell<Vec<Option<TextBlock>>>,
    icons: RefCell<HashMap<String, ImageSource>>,
    /// Selection is tracked by key (survives rebuilds); the flat index is
    /// re-derived in `rebuild_flat` and is None while the node is hidden
    /// inside a collapsed branch.
    selected_key: RefCell<Option<String>>,
    selected_flat: Cell<Option<usize>>,
    /// Key of the node whose chevron was toggled last; read it inside
    /// `Expanded`/`Collapsed` handlers.
    last_expanded: RefCell<Option<String>>,

    scroll_y: Cell<i32>, // <= 0
    v_scroll_visible: Cell<bool>,
    dragging_thumb: Cell<bool>,
    drag_anchor_y: Cell<i32>,
    drag_anchor_scroll: Cell<i32>,

    row_height_dip: Cell<Option<i32>>,
    icon_size_dip: Cell<i32>,
    indent_dip: Cell<i32>,
    row_height_px: Cell<i32>,
    needs_relayout: Cell<bool>,
}

impl HasMainFields for TreeView {
    fn main_fields(&self) -> &RefCell<FieldsMain> { &self.state }
}
impl ViewBasics for TreeView {}

#[allow(dead_code)]
impl TreeView {
    pub fn new(rect_: Rect<i32>) -> TreeView {
        let mut main = FieldsMain::with_rect(rect_, Dimension::Min, Dimension::Min);
        main.state.focusable = true;
        TreeView {
            state: RefCell::new(main),
            roots: RefCell::new(Vec::new()),
            flat: RefCell::new(Vec::new()),
            blocks: RefCell::new(Vec::new()),
            icons: RefCell::new(HashMap::new()),
            selected_key: RefCell::new(None),
            selected_flat: Cell::new(None),
            last_expanded: RefCell::new(None),
            scroll_y: Cell::new(0),
            v_scroll_visible: Cell::new(false),
            dragging_thumb: Cell::new(false),
            drag_anchor_y: Cell::new(0),
            drag_anchor_scroll: Cell::new(0),
            row_height_dip: Cell::new(None),
            icon_size_dip: Cell::new(DEFAULT_ICON_SIZE_DIP),
            indent_dip: Cell::new(DEFAULT_INDENT_DIP),
            row_height_px: Cell::new(0),
            needs_relayout: Cell::new(false),
        }
    }

    // --- Public API ---

    /// Replace the whole tree with the given root nodes.
    pub fn set_roots(&self, roots: Vec<TreeNode>) {
        *self.roots.borrow_mut() = roots;
        self.scroll_y.set(0);
        self.rebuild_flat();
    }

    /// Replace the children of the node with `key` (typically from inside an
    /// `Expanded` handler — lazy loading). Also syncs the node's
    /// `has_children` flag to whether any children were supplied.
    /// Returns false when no node with this key exists.
    pub fn set_children(&self, key: &str, children: Vec<TreeNode>) -> bool {
        {
            let mut roots = self.roots.borrow_mut();
            let Some(node) = find_node_mut(&mut roots, key) else { return false; };
            node.has_children = !children.is_empty();
            node.children = children;
        }
        self.rebuild_flat();
        true
    }

    /// Expand or collapse a node programmatically. Fires no event.
    /// Returns false when no node with this key exists.
    pub fn set_expanded(&self, key: &str, expanded: bool) -> bool {
        {
            let mut roots = self.roots.borrow_mut();
            let Some(node) = find_node_mut(&mut roots, key) else { return false; };
            node.expanded = expanded;
        }
        self.rebuild_flat();
        true
    }

    /// Select the node with `key`, expanding all its ancestors so it becomes
    /// visible, and scroll it into view. Fires no event (programmatic
    /// setters never do). Returns false when no node with this key exists.
    pub fn select_key(&self, key: &str) -> bool {
        {
            let mut path = Vec::new();
            let mut roots = self.roots.borrow_mut();
            if !find_path(&roots, key, &mut path) { return false; }
            // Expand every ancestor along the path (not the node itself).
            let mut nodes: &mut Vec<TreeNode> = &mut roots;
            for (step, &i) in path.iter().enumerate() {
                if step + 1 == path.len() { break; }
                nodes[i].expanded = true;
                nodes = &mut nodes[i].children;
            }
        }
        *self.selected_key.borrow_mut() = Some(key.to_owned());
        self.rebuild_flat();
        if let Some(idx) = self.selected_flat.get() {
            self.ensure_visible(idx);
        }
        true
    }

    /// Key of the currently selected node, if any.
    pub fn selected_key(&self) -> Option<String> {
        self.selected_key.borrow().clone()
    }

    /// Key of the node whose chevron was toggled last — the node an
    /// `Expanded`/`Collapsed` event is about.
    pub fn expanded_key(&self) -> Option<String> {
        self.last_expanded.borrow().clone()
    }

    /// A clone of the node (and its subtree) with `key`.
    pub fn node(&self, key: &str) -> Option<TreeNode> {
        find_node(&self.roots.borrow(), key).cloned()
    }

    /// Number of currently visible (flattened) rows.
    pub fn visible_count(&self) -> usize {
        self.flat.borrow().len()
    }

    // --- Internals ---

    /// Rebuild the flattened visible-row list from the tree, re-anchor the
    /// selection by key, and request an inner relayout (text blocks).
    fn rebuild_flat(&self) {
        fn walk(nodes: &[TreeNode], depth: usize, out: &mut Vec<FlatRow>) {
            for n in nodes.iter() {
                out.push(FlatRow {
                    key: n.key.clone(),
                    text: n.text.clone(),
                    icon: n.icon.clone(),
                    tint: n.tint,
                    depth,
                    has_children: n.has_children,
                    expanded: n.expanded,
                });
                if n.expanded {
                    walk(&n.children, depth + 1, out);
                }
            }
        }
        let mut flat = Vec::new();
        walk(&self.roots.borrow(), 0, &mut flat);

        let selected = self.selected_key.borrow().clone();
        let idx = selected.and_then(|key| flat.iter().position(|r| r.key == key));
        self.selected_flat.set(idx);

        *self.flat.borrow_mut() = flat;
        self.needs_relayout.set(true);
    }

    fn inner_relayout(&self, scale: f64, typeface: &Typeface) {
        let base_size = typeface.font_size
            .unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let row_h = match self.row_height_dip.get() {
            Some(dip) => (dip as f64 * scale).round() as i32,
            None => {
                let line = (base_size * scale as f32).ceil() as i32;
                let icon = (self.icon_size_dip.get() as f64 * scale).round() as i32;
                let pad = (DEFAULT_ROW_PAD_V as f64 * scale).round() as i32 * 2;
                line.max(icon) + pad
            }
        };
        self.row_height_px.set(row_h);

        let mut blocks = Vec::new();
        let font = get_font_family(&typeface.font_name, typeface.font_style);
        for row in self.flat.borrow().iter() {
            let block = font.as_ref().map(|f| {
                f.layout_text(&row.text, base_size * scale as f32, TextOptions::new())
            });
            blocks.push(block);
        }
        *self.blocks.borrow_mut() = blocks;

        self.resolve_scrollbar_visibility();
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
        let mut w = r.width() - 2 * self.border_inset();
        if self.v_scroll_visible.get() { w -= self.scrollbar_thickness(); }
        w.max(0)
    }

    fn body_height(&self) -> i32 {
        let r = self.state.borrow().rect;
        (r.height() - 2 * self.border_inset()).max(0)
    }

    fn content_height(&self) -> i32 {
        self.flat.borrow().len() as i32 * self.row_height_px.get()
    }

    fn resolve_scrollbar_visibility(&self) {
        self.v_scroll_visible.set(self.content_height() > self.body_height());
    }

    fn clamp_scroll(&self) {
        let max_neg = -(self.content_height() - self.body_height()).max(0);
        let y = self.scroll_y.get().clamp(max_neg, 0);
        self.scroll_y.set(y);
    }

    fn ensure_visible(&self, idx: usize) {
        let row_h = self.row_height_px.get().max(1);
        let bh = self.body_height();
        let top = idx as i32 * row_h;
        let bot = top + row_h;
        let cur = self.scroll_y.get();
        if top + cur < 0 {
            self.scroll_y.set(-top);
        } else if bot + cur > bh {
            self.scroll_y.set(bh - bot);
        }
        self.clamp_scroll();
    }

    // Scrollbar geometry (vertical only), matching the TableView chrome.

    fn v_scrollbar_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let inset = self.border_inset();
        let thickness = self.scrollbar_thickness();
        let x_max = r.min.x + origin.x + r.width() - inset;
        let x_min = x_max - thickness;
        let y_min = r.min.y + origin.y + inset;
        let y_max = r.min.y + origin.y + r.height() - inset;
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

    fn v_track_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        if sb.height() < 2 * size {
            return rect((sb.min.x, sb.min.y), (sb.max.x, sb.min.y));
        }
        rect((sb.min.x, sb.min.y + size), (sb.max.x, sb.max.y - size))
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

    fn v_track_length(&self) -> i32 {
        let bh = self.body_height();
        let t = self.scrollbar_thickness();
        if bh < 2 * t { 0 } else { bh - 2 * t }
    }

    /// Flat row index at a widget-local y, or None below the last row.
    fn row_at_local_y(&self, local_y: i32) -> Option<usize> {
        let row_h = self.row_height_px.get().max(1);
        let y = local_y - self.border_inset() - self.scroll_y.get();
        if y < 0 { return None; }
        let idx = (y / row_h) as usize;
        if idx < self.flat.borrow().len() { Some(idx) } else { None }
    }

    /// True when a widget-local x lands in row `idx`'s chevron column.
    fn is_chevron_hit(&self, idx: usize, local_x: i32) -> bool {
        let flat = self.flat.borrow();
        let Some(row) = flat.get(idx) else { return false; };
        if !row.has_children { return false; }
        let scale = self.state.borrow().scale;
        let indent = (self.indent_dip.get() as f64 * scale).round() as i32;
        let x = local_x - self.border_inset();
        let start = row.depth as i32 * indent;
        x >= start && x < start + indent
    }

    fn select_row(&self, ui: &mut UI, idx: usize) {
        let key = match self.flat.borrow().get(idx) {
            Some(row) => row.key.clone(),
            None => return,
        };
        let changed = self.selected_flat.get() != Some(idx);
        *self.selected_key.borrow_mut() = Some(key);
        self.selected_flat.set(Some(idx));
        self.ensure_visible(idx);
        if changed {
            self.base_fire_event(ui, EventType::SelectionChanged, &EventData::Selected(idx));
        }
    }

    /// Toggle a node's expansion (chevron click / Left / Right key) and fire
    /// `Expanded`/`Collapsed`. Nothing happens on nodes without children.
    fn toggle_expand(&self, ui: &mut UI, idx: usize) {
        let (key, was_expanded, has_children) = match self.flat.borrow().get(idx) {
            Some(row) => (row.key.clone(), row.expanded, row.has_children),
            None => return,
        };
        if !has_children { return; }
        {
            let mut roots = self.roots.borrow_mut();
            if let Some(node) = find_node_mut(&mut roots, &key) {
                node.expanded = !was_expanded;
            }
        }
        *self.last_expanded.borrow_mut() = Some(key.clone());
        self.rebuild_flat();
        let event = if was_expanded { EventType::Collapsed } else { EventType::Expanded };
        self.base_fire_event(ui, event, &EventData::Selected(idx));
        if !was_expanded {
            // Lazy-load handshake: the handler had its chance to supply
            // children. If none arrived, this branch is really empty —
            // drop the chevron so it doesn't invite another useless expand.
            let empty = {
                let roots = self.roots.borrow();
                find_node(&roots, &key).map(|n| n.children.is_empty()).unwrap_or(false)
            };
            if empty {
                let mut roots = self.roots.borrow_mut();
                if let Some(node) = find_node_mut(&mut roots, &key) {
                    node.has_children = false;
                    node.expanded = false;
                }
                drop(roots);
                self.rebuild_flat();
            }
        }
    }

    /// Draw a chevron with rect slivers: right-pointing when collapsed,
    /// down-pointing when expanded. `color` follows the row's text color so
    /// the chevron stays visible on the selection highlight.
    fn paint_chevron(&self, theme: &mut dyn Renderer, zone: Rect<i32>, expanded: bool, color: u32) {
        let scale = self.state.borrow().scale;
        let size = (8.0 * scale).round() as i32;
        let cx = zone.min.x + zone.width() / 2;
        let cy = zone.min.y + zone.height() / 2;
        if expanded {
            for i in 0..(size / 2) {
                let half_w = size / 2 - i;
                let y = cy - size / 4 + i;
                theme.draw_rect(rect((cx - half_w, y), (cx + half_w, y + 1)), color);
            }
        } else {
            for i in 0..(size / 2) {
                let half_h = size / 2 - i;
                let x = cx - size / 4 + i;
                theme.draw_rect(rect((x, cy - half_h), (x + 1, cy + half_h)), color);
            }
        }
    }
}

impl View for TreeView {
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
            "indent" => {
                if let Ok(i) = value.parse::<i32>() {
                    self.indent_dip.set(i);
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
            if matches!(state.width, Dimension::Min) { new_w = width.max(150); }
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
        let bw = self.body_width();
        let bh = self.body_height();
        let scroll_y = self.scroll_y.get();
        let indent = (self.indent_dip.get() as f64 * scale).round() as i32;
        let icon_size = (self.icon_size_dip.get() as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP_DIP as f64 * scale).round() as i32;

        let body_origin = Point { x: r.min.x + inset, y: r.min.y + inset };
        let body_clip = rect((body_origin.x, body_origin.y), (body_origin.x + bw, body_origin.y + bh));

        theme.push_clip();
        theme.clip_rect(body_clip);

        let flat = self.flat.borrow();
        let blocks = self.blocks.borrow();
        let n = flat.len();
        let first = if row_h > 0 { ((-scroll_y) / row_h.max(1)).max(0) as usize } else { 0 };
        let last = if row_h > 0 {
            (((bh - scroll_y).max(0) / row_h.max(1)) as usize + 1).min(n)
        } else { 0 };
        let selected = self.selected_flat.get();

        for i in first..last {
            let row = &flat[i];
            let row_top = body_origin.y + i as i32 * row_h + scroll_y;
            let mut text_color = theme.color("text");

            if selected == Some(i) {
                let hl = rect((body_origin.x, row_top), (body_origin.x + bw, row_top + row_h));
                theme.draw_rect(hl, theme.color("item_highlight"));
                text_color = theme.color("item_highlight_text");
            }

            let chevron_x = body_origin.x + row.depth as i32 * indent;
            if row.has_children {
                let zone = rect((chevron_x, row_top), (chevron_x + indent, row_top + row_h));
                self.paint_chevron(theme, zone, row.expanded, text_color);
            }

            let mut x = chevron_x + indent;
            if let Some(icon_path) = &row.icon {
                let icon_rect = rect(
                    (x, row_top + (row_h - icon_size) / 2),
                    (x + icon_size, row_top + (row_h - icon_size) / 2 + icon_size),
                );
                let tint = row.tint.unwrap_or_else(|| theme.color("icon_tint"));
                let mut icons = self.icons.borrow_mut();
                let icon = icons.entry(icon_path.clone())
                    .or_insert_with(|| ImageSource::new(icon_path));
                icon.draw(theme, icon_rect, tint);
                x += icon_size + gap;
            }

            if let Some(Some(block)) = blocks.get(i) {
                let th = block.height().ceil() as i32;
                let ty = row_top + (row_h - th) / 2;
                theme.draw_text(x as f32, ty as f32, text_color, block);
            }
        }
        drop(flat);
        drop(blocks);

        theme.pop_clip();

        // ---- Scrollbar ----
        if self.v_scroll_visible.get() {
            let unfocused = ViewState::no_focus();
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
        (150, self.content_height().max(100))
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
        accesskit::Node::new(accesskit::Role::Tree)
    }

    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        let id = self.get_id();
        let inset = self.border_inset();
        let scroll_y = self.scroll_y.get();
        let row_h = self.row_height_px.get();
        let width = self.get_rect_width();
        let selected = self.selected_flat.get();
        let mut result = Vec::new();
        for (i, row) in self.flat.borrow().iter().enumerate() {
            let mut node = accesskit::Node::new(accesskit::Role::TreeItem);
            node.set_label(row.text.clone());
            node.set_level(row.depth + 1);
            node.set_selected(selected == Some(i));
            if row.has_children {
                node.set_expanded(row.expanded);
            }
            node.add_action(accesskit::Action::Click);
            let y = inset + i as i32 * row_h + scroll_y;
            // View-local; the tree builder translates to window space.
            node.set_bounds(accesskit::Rect {
                x0: inset as f64,
                y0: y as f64,
                x1: (width - inset) as f64,
                y1: (y + row_h) as f64,
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
        let local_y = position.y - r.min.y;
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
        true
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) { return false; }
        if !matches!(button, MouseButton::Left) { return false; }

        self.state.borrow_mut().state.focused = true;

        if self.v_scroll_visible.get() {
            let thumb = self.v_thumb_rect(point(0, 0));
            if thumb.hit((position.x, position.y)) {
                self.dragging_thumb.set(true);
                self.drag_anchor_y.set(position.y - r.min.y);
                self.drag_anchor_scroll.set(self.scroll_y.get());
                return true;
            }
            let row_h = self.row_height_px.get().max(20);
            if self.v_arrow_top_rect(point(0, 0)).hit((position.x, position.y)) {
                self.scroll_y.set(self.scroll_y.get() + row_h);
                self.clamp_scroll();
                return true;
            }
            if self.v_arrow_bottom_rect(point(0, 0)).hit((position.x, position.y)) {
                self.scroll_y.set(self.scroll_y.get() - row_h);
                self.clamp_scroll();
                return true;
            }
            let sb = self.v_scrollbar_rect(point(0, 0));
            if sb.hit((position.x, position.y)) {
                // Track click between thumb and arrows: page-scroll toward it.
                let bh = self.body_height();
                let dir = if position.y < thumb.min.y { 1 } else { -1 };
                self.scroll_y.set(self.scroll_y.get() + dir * bh);
                self.clamp_scroll();
                return true;
            }
        }

        let local_x = position.x - r.min.x;
        let local_y = position.y - r.min.y;
        if let Some(idx) = self.row_at_local_y(local_y) {
            if self.is_chevron_hit(idx, local_x) {
                self.toggle_expand(ui, idx);
            } else {
                self.select_row(ui, idx);
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
        let row_h = self.row_height_px.get().max(20);
        let bh = self.body_height();
        let dy = match distance {
            MouseScrollDistance::Lines { y, .. } => y as i32 * row_h,
            MouseScrollDistance::Pixels { y, .. } => y as i32,
            MouseScrollDistance::Pages { y, .. } => y as i32 * bh,
        };
        self.scroll_y.set(self.scroll_y.get() + dy);
        self.clamp_scroll();
        true
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_focused() { return false; }
        let Some(code) = virtual_key_code else { return false; };
        if code == VirtualKeyCode::Tab { return false; }
        let n = self.flat.borrow().len();
        if n == 0 { return false; }
        let row_h = self.row_height_px.get().max(1);
        let visible_rows = (self.body_height() / row_h).max(1) as usize;
        let cur = self.selected_flat.get();

        let new_idx = match code {
            VirtualKeyCode::Up => Some(cur.map(|i| i.saturating_sub(1)).unwrap_or(n - 1)),
            VirtualKeyCode::Down => Some(cur.map(|i| (i + 1).min(n - 1)).unwrap_or(0)),
            VirtualKeyCode::Home => Some(0),
            VirtualKeyCode::End => Some(n - 1),
            VirtualKeyCode::PageUp => Some(cur.map(|i| i.saturating_sub(visible_rows)).unwrap_or(0)),
            VirtualKeyCode::PageDown => Some(cur.map(|i| (i + visible_rows).min(n - 1)).unwrap_or(n - 1)),
            VirtualKeyCode::Left => {
                let Some(i) = cur else { return true; };
                let (depth, has_children, expanded) = {
                    let flat = self.flat.borrow();
                    let row = &flat[i];
                    (row.depth, row.has_children, row.expanded)
                };
                if has_children && expanded {
                    self.toggle_expand(ui, i);
                    return true;
                }
                // Jump to the parent: nearest preceding row one level up.
                if depth == 0 { return true; }
                let flat = self.flat.borrow();
                (0..i).rev().find(|&j| flat[j].depth == depth - 1)
            }
            VirtualKeyCode::Right => {
                let Some(i) = cur else { return true; };
                let (has_children, expanded) = {
                    let flat = self.flat.borrow();
                    let row = &flat[i];
                    (row.has_children, row.expanded)
                };
                if has_children && !expanded {
                    self.toggle_expand(ui, i);
                    return true;
                }
                // Already expanded: step into the first child.
                if expanded && i + 1 < n && self.flat.borrow()[i + 1].depth > self.flat.borrow()[i].depth {
                    Some(i + 1)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(idx) = new_idx {
            self.select_row(ui, idx);
            return true;
        }
        false
    }
}

impl Default for TreeView {
    fn default() -> Self {
        TreeView::new(rect((0, 0), (200, 300)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tree() -> Vec<TreeNode> {
        vec![
            TreeNode {
                text: "A".into(), key: "a".into(), has_children: true, expanded: false,
                children: vec![
                    TreeNode::new("A1", "a1", false),
                    TreeNode {
                        text: "A2".into(), key: "a2".into(), has_children: true, expanded: false,
                        children: vec![TreeNode::new("A2x", "a2x", false)],
                        ..TreeNode::default()
                    },
                ],
                ..TreeNode::default()
            },
            TreeNode::new("B", "b", true), // children unknown yet (lazy)
        ]
    }

    #[test]
    fn flatten_counts_follow_expansion() {
        let tv = TreeView::default();
        tv.set_roots(sample_tree());
        assert_eq!(tv.visible_count(), 2); // a, b

        tv.set_expanded("a", true);
        assert_eq!(tv.visible_count(), 4); // a, a1, a2, b

        tv.set_expanded("a2", true);
        assert_eq!(tv.visible_count(), 5);

        tv.set_expanded("a", false);
        assert_eq!(tv.visible_count(), 2); // a2 stays expanded but hidden
    }

    #[test]
    fn set_children_by_key_grows_branch() {
        let tv = TreeView::default();
        tv.set_roots(sample_tree());
        assert!(tv.set_children("b", vec![
            TreeNode::new("B1", "b1", false),
            TreeNode::new("B2", "b2", false),
        ]));
        tv.set_expanded("b", true);
        assert_eq!(tv.visible_count(), 4); // a, b, b1, b2
        assert!(!tv.set_children("missing", vec![]));
    }

    #[test]
    fn set_children_empty_clears_has_children() {
        let tv = TreeView::default();
        tv.set_roots(sample_tree());
        assert!(tv.set_children("b", vec![]));
        assert!(!tv.node("b").unwrap().has_children);
    }

    #[test]
    fn selection_key_survives_sibling_collapse() {
        let tv = TreeView::default();
        tv.set_roots(sample_tree());
        tv.set_expanded("a", true);
        assert!(tv.select_key("a2"));
        assert_eq!(tv.selected_key().as_deref(), Some("a2"));

        // Collapsing an unrelated branch must not shift the selection.
        tv.set_expanded("b", false);
        assert_eq!(tv.selected_key().as_deref(), Some("a2"));

        // Hiding the selected node keeps the key but clears the visible row.
        tv.set_expanded("a", false);
        assert_eq!(tv.selected_key().as_deref(), Some("a2"));
        assert!(tv.selected_flat.get().is_none());

        // Re-expanding re-anchors the highlight.
        tv.set_expanded("a", true);
        assert_eq!(tv.selected_flat.get(), Some(2));
    }

    #[test]
    fn select_key_expands_ancestors() {
        let tv = TreeView::default();
        tv.set_roots(sample_tree());
        assert!(tv.select_key("a2x"));
        assert!(tv.node("a").unwrap().expanded);
        assert!(tv.node("a2").unwrap().expanded);
        assert_eq!(tv.visible_count(), 5);
        assert_eq!(tv.selected_flat.get(), Some(3)); // a, a1, a2, a2x, b
        assert!(!tv.select_key("missing"));
    }
}
