use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::text::{TextBlock, TextOptions};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Renderer, Typeface, ViewState};
use crate::traits::{Container, Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};

/// Horizontal padding inside each tab (in dip).
const TAB_PADDING_H: i32 = 8;
/// Vertical padding inside each tab (in dip).
const TAB_PADDING_V: i32 = 4;
/// Border width around the content area (in dip).
const CONTENT_BORDER: i32 = 2;
/// Side of the square close-button hit area inside a tab (in dip).
const CLOSE_SIZE: i32 = 12;
/// Gap between a tab's title and its close button (in dip).
const CLOSE_GAP: i32 = 4;
/// How far one line of wheel scroll pans the tab strip (in dip).
const STRIP_SCROLL_LINE: i32 = 24;
/// The close-button glyph. A multiplication sign rather than one of the U+27xx
/// crosses: it is present in every UI font we might be rendered with.
const CLOSE_GLYPH: &str = "\u{00D7}";

struct TabInfo {
    title: String,
    cached_title: Option<TextBlock>,
    tab_rect: Rect<i32>,
    /// The close button's hit area, in the same unscrolled strip coordinates as
    /// `tab_rect`. Empty while the strip is not `closable`.
    close_rect: Rect<i32>,
}

pub struct TabView {
    state: RefCell<FieldsMain>,
    views: Vec<Element>,
    tabs: Vec<TabInfo>,
    active_tab: Cell<usize>,
    tab_bar_height: Cell<i32>,
    /// Horizontal pan of the tab strip: negative or zero, like `ScrollView`'s
    /// offset. Tabs are laid out from x=0 and drawn shifted by this.
    strip_offset: Cell<i32>,
    /// Whether tabs draw a close button and report `TabClose`.
    closable: Cell<bool>,
    /// Index of the tab whose close button the pointer is over, for hover feedback.
    hover_close: Cell<Option<usize>>,
    /// The shaped close glyph, shared by every tab (cleared when the font changes).
    close_glyph: RefCell<Option<TextBlock>>,
}

impl HasMainFields for TabView {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for TabView {}

#[allow(dead_code)]
impl TabView {
    pub fn get_active_tab(&self) -> usize {
        self.active_tab.get()
    }

    pub fn set_active_tab(&self, index: usize) {
        if index < self.views.len() {
            self.active_tab.set(index);
            self.ensure_active_visible();
        }
    }

    pub fn set_tab_title(&mut self, index: usize, title: &str) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.title = title.to_owned();
            tab.cached_title = None;
        }
    }

    pub fn get_tab_count(&self) -> usize {
        self.views.len()
    }

    pub fn get_tab_title(&self, index: usize) -> Option<String> {
        self.tabs.get(index).map(|t| t.title.clone())
    }

    /// Give every tab a close button. Clicking one fires [`EventType::TabClose`]
    /// with the tab index; the TabView does NOT remove the tab itself, so the
    /// app can confirm first (and then call `UI::remove_view` with the child's id).
    pub fn set_closable(&self, closable: bool) {
        self.closable.set(closable);
    }

    pub fn is_closable(&self) -> bool {
        self.closable.get()
    }

    /// Total width of all tabs laid end to end, in physical px.
    fn strip_width(&self) -> i32 {
        self.tabs.last().map(|t| t.tab_rect.max.x).unwrap_or(0)
    }

    /// The most negative the strip may be panned (0 when everything fits).
    fn min_strip_offset(&self) -> i32 {
        (self.state.borrow().rect.width() - self.strip_width()).min(0)
    }

    fn clamp_strip_offset(&self) {
        let offset = self.strip_offset.get().min(0).max(self.min_strip_offset());
        self.strip_offset.set(offset);
    }

    /// Pan the strip so the active tab is fully visible. Called when the active
    /// tab changes by keyboard or by the app — a wheel pan is left alone, since
    /// the user is deliberately looking elsewhere.
    fn ensure_active_visible(&self) {
        let active = self.active_tab.get();
        let Some(tab) = self.tabs.get(active) else { return };
        let width = self.state.borrow().rect.width();
        let mut offset = self.strip_offset.get();
        if tab.tab_rect.max.x + offset > width {
            offset = width - tab.tab_rect.max.x;
        }
        if tab.tab_rect.min.x + offset < 0 {
            offset = -tab.tab_rect.min.x;
        }
        self.strip_offset.set(offset);
        self.clamp_strip_offset();
    }

    /// Whether the tab strip itself (not a view inside a tab) holds keyboard
    /// focus. While it does, Left/Right switch tabs.
    fn strip_focused(&self) -> bool {
        self.state.borrow().state.focused
    }

    /// Switches to `index` and fires `SelectionChanged`; no-op when out of
    /// range or already active.
    fn change_tab(&self, ui: &mut UI, index: usize) {
        if index >= self.views.len() || index == self.active_tab.get() {
            return;
        }
        self.active_tab.set(index);
        self.ensure_active_visible();
        self.base_fire_event(ui, EventType::SelectionChanged, &EventData::Selected(index));
    }

    fn set_font(&mut self, font_name: &str) {
        self.state.borrow_mut().font_manager.set_font(font_name);
    }

    fn set_font_style(&mut self, style: &str) {
        self.state.borrow_mut().font_manager.set_font_style(style);
    }

    fn set_font_size(&mut self, size: f32) {
        self.state.borrow_mut().font_manager.set_font_size(size);
        for tab in self.tabs.iter_mut() {
            tab.cached_title = None;
        }
        *self.close_glyph.borrow_mut() = None;
    }

    /// Draw one tab: background component, centred title, close button and (for the
    /// active tab) the keyboard-focus outline. `start` is the widget's absolute origin;
    /// tab rects are in unscrolled strip coordinates, so the pan is applied here.
    fn draw_tab(&self, theme: &mut dyn Renderer, index: usize, start: Point<i32>, view_state: ViewState, scale: f64, active: bool) {
        let tab = &self.tabs[index];
        let pan = Point::new(start.x + self.strip_offset.get(), start.y);
        let mut tr = tab.tab_rect;
        tr.move_by(pan);
        if !active {
            // Inactive tabs are shorter — offset top by 2 scaled pixels
            tr.min.y += (2.0 * scale).round() as i32;
        }
        theme.draw_component(if active { "tab.active" } else { "tab.inactive" }, tr, view_state);

        let closable = self.closable.get();
        let close_room = if closable {
            ((CLOSE_SIZE + CLOSE_GAP) as f64 * scale).round() as i32
        } else {
            0
        };
        let text_color = theme.get_text_color(view_state, self.state.borrow().foreground.as_ref());
        if let Some(ref text) = tab.cached_title {
            // Centred in the tab MINUS the close button's room, so adding a close
            // button doesn't shove the label off-centre.
            let label_w = tr.width() - close_room;
            let text_x = tr.min.x as f32 + (label_w as f32 - text.width()) / 2.0;
            let text_y = tr.min.y as f32 + (tr.height() as f32 - text.height()) / 2.0;
            theme.draw_text(text_x.round(), text_y.round(), text_color, text);
        }

        if closable {
            let mut cr = tab.close_rect;
            cr.move_by(pan);
            if self.hover_close.get() == Some(index) {
                let radius = (2.0 * scale).round() as i32;
                let hover_color = theme.color("background_hover");
                theme.draw_rounded_rect(cr, hover_color, radius);
            }
            if let Some(ref glyph) = *self.close_glyph.borrow() {
                let gx = cr.min.x as f32 + (cr.width() as f32 - glyph.width()) / 2.0;
                let gy = cr.min.y as f32 + (cr.height() as f32 - glyph.height()) / 2.0;
                theme.draw_text(gx.round(), gy.round(), text_color, glyph);
            }
        }

        // Keyboard-focus indicator: thin outline inside the active tab while the
        // strip holds focus.
        if active && view_state.focused && view_state.enabled {
            let inset = (3.0 * scale).round() as i32;
            let fr = rect(
                (tr.min.x + inset, tr.min.y + inset),
                (tr.max.x - inset, tr.max.y - inset),
            );
            if fr.width() > 0 && fr.height() > 0 {
                let width = (scale.round() as i32).max(1);
                let focus_color = theme.color("focus");
                theme.draw_rect_outline(fr, focus_color, width);
            }
        }
    }

    fn layout_tab_titles(&mut self, scale: f64) {
        let typeface = self.state.borrow().font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let base_size = typeface.font_size.unwrap_or_else(|| crate::drawing::current_text_size("text"));
                let size = base_size * scale as f32;
                for tab in self.tabs.iter_mut() {
                    if tab.cached_title.is_none() {
                        let text = font.layout_text(&tab.title, size, TextOptions::new());
                        tab.cached_title = Some(text);
                    }
                }
                if self.close_glyph.borrow().is_none() {
                    let glyph = font.layout_text(CLOSE_GLYPH, size, TextOptions::new());
                    *self.close_glyph.borrow_mut() = Some(glyph);
                }
            }
        }
    }
}

impl Container for TabView {
    fn add_view(&mut self, view: Element) {
        let title = view.borrow().get_id();
        self.tabs.push(TabInfo {
            title,
            cached_title: None,
            tab_rect: rect((0, 0), (0, 0)),
            close_rect: rect((0, 0), (0, 0)),
        });
        self.views.push(view);
    }

    fn get_view(&self, id: &str) -> Option<Element> {
        // Search ALL children, not just active tab
        if let Some(found) = self.views.iter().find(|v| v.borrow().get_id() == id) {
            return Some(Rc::clone(found));
        }
        for v in self.views.iter() {
            if let Some(container) = v.borrow().as_container() {
                if let Some(found) = container.get_view(id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn get_view_count(&self) -> usize {
        self.views.len()
    }

    fn get_views(&self) -> Vec<Element> {
        self.views.clone()
    }

    /// Only the active tab is on screen and interactive, so coordinate-based
    /// hit testing must see just that one — otherwise events like DoubleClick
    /// or ContextMenu would match views on inactive tabs, which are laid out
    /// at the same content rect.
    fn hit_test_views(&self) -> Vec<Element> {
        self.views.get(self.active_tab.get()).cloned().into_iter().collect()
    }

    fn remove_view(&mut self, id: &str) -> bool {
        if let Some(pos) = self.views.iter().position(|v| v.borrow().get_id() == id) {
            self.views.remove(pos);
            self.tabs.remove(pos);
            // Keep the same tab active where possible: removing one before it
            // shifts it left, and removing the last one falls back to the new last.
            let active = self.active_tab.get();
            let new_active = if self.views.is_empty() {
                0
            } else if active > pos {
                active - 1
            } else {
                active.min(self.views.len() - 1)
            };
            self.active_tab.set(new_active);
            self.hover_close.set(None);
            return true;
        }
        for v in &self.views {
            let removed = if let Some(container) = v.borrow_mut().as_container_mut() {
                container.remove_view(id)
            } else {
                false
            };
            if removed {
                return true;
            }
        }
        false
    }
}

impl View for TabView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "active_tab" => {
                if let Ok(idx) = value.parse::<usize>() {
                    self.active_tab.set(idx);
                }
            }
            "closable" => self.set_closable(value.parse().unwrap_or(false)),
            "font" => self.set_font(value),
            "font_style" => self.set_font_style(value),
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
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

        // Resolve typeface
        let typeface = match self.state.borrow().font_manager.get() {
            None => typeface.clone(),
            Some(t) => t,
        };
        self.state.borrow_mut().font_manager.set(Some(typeface.clone()));

        let (new_width, new_height) = self.calculate_size(width, height, scale);

        // Layout tab title text blocks
        self.layout_tab_titles(scale);

        // Calculate tab bar height
        let pad_v = (TAB_PADDING_V as f64 * scale).round() as i32;
        let pad_h = (TAB_PADDING_H as f64 * scale).round() as i32;
        let border = (CONTENT_BORDER as f64 * scale).round() as i32;

        let mut max_text_height = 0i32;
        for tab in self.tabs.iter() {
            if let Some(ref text) = tab.cached_title {
                let h = text.height().ceil() as i32;
                if h > max_text_height {
                    max_text_height = h;
                }
            }
        }
        let tab_bar_h = max_text_height + pad_v * 2;
        self.tab_bar_height.set(tab_bar_h);

        // Compute each tab's rect (positioned horizontally)
        let closable = self.closable.get();
        let close_size = (CLOSE_SIZE as f64 * scale).round() as i32;
        let close_gap = (CLOSE_GAP as f64 * scale).round() as i32;
        let close_room = if closable { close_size + close_gap } else { 0 };
        let mut tab_x = 0i32;
        for tab in self.tabs.iter_mut() {
            let text_width = tab.cached_title.as_ref().map(|t| t.width().ceil() as i32).unwrap_or(40);
            let tab_w = text_width + pad_h * 2 + close_room;
            tab.tab_rect = rect((tab_x, 0), (tab_x + tab_w, tab_bar_h));
            tab.close_rect = if closable {
                let cx = tab_x + tab_w - pad_h - close_size;
                let cy = (tab_bar_h - close_size) / 2;
                rect((cx, cy), (cx + close_size, cy + close_size))
            } else {
                rect((0, 0), (0, 0))
            };
            tab_x += tab_w;
        }

        // Content area: below tab bar, inset by border
        let content_x = border;
        let content_y = tab_bar_h + border;
        let content_w = new_width - border * 2;
        let content_h = new_height - tab_bar_h - border * 2;

        // Layout all children so they are ready when the user switches tabs
        for v in self.views.iter() {
            let mut v = v.try_borrow_mut().unwrap();
            let margins = v.get_margin(scale);
            v.layout_content(
                content_x + margins.left,
                content_y + margins.top,
                content_w - margins.left - margins.right,
                content_h - margins.top - margins.bottom,
                &typeface,
                scale,
            );
        }

        let my_rect = rect((x, y), (x + new_width, y + new_height));
        self.base_set_rect(my_rect);
        // A narrower widget (or a removed tab) can leave the pan out of range.
        // Needs the rect just set, hence after `base_set_rect`.
        self.clamp_strip_offset();
        my_rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, scale: f64) -> bool {
        let size = self.calculate_full_size(scale);
        size.0 <= width && size.1 <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Renderer) {
        let my_rect = {
            let mut r = self.state.borrow().rect;
            r.move_by(origin);
            r
        };
        let start = my_rect.min;
        let tab_bar_h = self.tab_bar_height.get();
        let scale = self.state.borrow().scale;
        let view_state = self.state.borrow().state;

        theme.push_clip();
        theme.clip_rect(my_rect);

        // Draw content area panel (below tab bar). A 9-patch background
        // replaces the content panel; the tab strip stays drawable-based.
        let content_rect = rect(
            (my_rect.min.x, my_rect.min.y + tab_bar_h),
            (my_rect.max.x, my_rect.max.y),
        );
        if !self.base_draw_ninepatch(theme, content_rect) {
            theme.draw_component("tab.content", content_rect, view_state);
        }

        let active = self.active_tab.get();

        // Draw inactive tabs first (behind active), then the active one on top.
        for i in 0..self.tabs.len() {
            if i != active {
                self.draw_tab(theme, i, start, view_state, scale, false);
            }
        }
        if active < self.tabs.len() {
            self.draw_tab(theme, active, start, view_state, scale, true);
        }

        // Paint only the active child
        if active < self.views.len() {
            let v = self.views[active].try_borrow().unwrap();
            v.paint(start, theme);
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
        // Content size is the full area including tab bar
        let scale = self.state.borrow().scale;
        let tab_bar_h = self.tab_bar_height.get();
        let border = (CONTENT_BORDER as f64 * scale).round() as i32;
        let active = self.active_tab.get();
        if active < self.views.len() {
            let v = self.views[active].borrow();
            let (cw, ch) = v.get_content_size();
            let margins = v.get_margin(scale);
            (cw + margins.left + margins.right + border * 2,
             ch + margins.top + margins.bottom + tab_bar_h + border * 2)
        } else {
            (border * 2, tab_bar_h + border * 2)
        }
    }

    fn is_focused(&self) -> bool {
        // The strip itself, or a view inside the active tab.
        if self.state.borrow().state.focused {
            return true;
        }
        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow().is_focused();
        }
        false
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn set_focused(&self, focused: bool) {
        // `true` focuses the tab strip; children get focus individually.
        self.state.borrow_mut().state.focused = focused;
        if !focused {
            for v in self.views.iter() {
                v.borrow().set_focused(false);
            }
        }
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
    fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.base_set_tooltip(tooltip);
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

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self as &dyn Container)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self as &mut dyn Container)
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
        accesskit::Node::new(accesskit::Role::TabList)
    }

    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        let id = self.get_id();
        let active = self.active_tab.get();
        self.tabs.iter().enumerate().map(|(i, tab)| {
            let mut node = accesskit::Node::new(accesskit::Role::Tab);
            node.set_label(tab.title.clone());
            node.set_selected(i == active);
            node.add_action(accesskit::Action::Click);
            // tab_rect is view-local; the tree builder translates it.
            node.set_bounds(accesskit::Rect {
                x0: tab.tab_rect.min.x as f64,
                y0: tab.tab_rect.min.y as f64,
                x1: tab.tab_rect.max.x as f64,
                y1: tab.tab_rect.max.y as f64,
            });
            (crate::accessibility::item_node_id(&id, i), node)
        }).collect()
    }

    fn click(&self, _ui: &mut UI) -> bool {
        false
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow_mut().update(ui);
        }
        false
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        let local = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);

        // Hover feedback for the close buttons. Tracked whatever the pointer does
        // afterwards, so leaving the strip clears the highlight.
        let old_hover = self.hover_close.get();
        let mut hover = None;
        if self.closable.get() && local.1 >= 0 && local.1 < self.tab_bar_height.get() {
            let strip_x = local.0 - self.strip_offset.get();
            hover = self.tabs.iter().position(|t| t.close_rect.hit((strip_x, local.1)));
        }
        self.hover_close.set(hover);
        let hover_changed = hover != old_hover;

        let active = self.active_tab.get();
        if active < self.views.len() {
            let handled = self.views[active].borrow().on_mouse_move(ui, Point::from(local));
            return handled || hover_changed;
        }
        hover_changed
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        let rect = self.state.borrow().rect;
        if !rect.hit((position.x, position.y)) {
            return false;
        }
        let local_x = position.x - rect.min.x;
        let local_y = position.y - rect.min.y;

        // Check if click is in the tab bar
        let tab_bar_h = self.tab_bar_height.get();
        if local_y < tab_bar_h {
            // Tab rects are unscrolled strip coordinates; undo the pan to test them.
            let strip_x = local_x - self.strip_offset.get();
            // A close button wins over the tab under it: clicking it must not also
            // switch to that tab.
            if self.closable.get() {
                for (i, tab) in self.tabs.iter().enumerate() {
                    if tab.close_rect.hit((strip_x, local_y)) {
                        self.base_fire_event(ui, EventType::TabClose, &EventData::Selected(i));
                        return true;
                    }
                }
            }
            for (i, tab) in self.tabs.iter().enumerate() {
                if tab.tab_rect.hit((strip_x, local_y)) {
                    // Clicking the strip gives it keyboard focus (children lose theirs).
                    self.state.borrow_mut().state.focused = true;
                    for v in self.views.iter() {
                        v.borrow().set_focused(false);
                    }
                    if i != self.active_tab.get() {
                        self.active_tab.set(i);
                        // Fire SelectionChanged listener
                        self.base_fire_event(ui, EventType::SelectionChanged, &EventData::Selected(i));
                    }
                    return true;
                }
            }
            return false;
        }

        // Forward to active child
        let active = self.active_tab.get();
        if active < self.views.len() {
            let focused;
            let v = &self.views[active];
            let f = v.borrow().is_focused();
            if v.borrow().on_mouse_button_down(ui, Point::new(local_x, local_y), button) {
                focused = !f && v.borrow().is_focused();
                if focused {
                    // Focus moved into the content: the strip loses it, and so
                    // do other tabs' children.
                    self.state.borrow_mut().state.focused = false;
                    for (j, vv) in self.views.iter().enumerate() {
                        if j != active {
                            vv.borrow().set_focused(false);
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        let local = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow().on_mouse_button_up(ui, Point::from(local), button);
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        let local = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);

        // Over the strip the wheel pans it horizontally; the active tab does not
        // change (that is a click). A vertical wheel pans too — it is the only
        // wheel most mice have.
        if local.1 >= 0 && local.1 < self.tab_bar_height.get() {
            if self.min_strip_offset() == 0 {
                return false; // everything fits, nothing to pan
            }
            let scale = self.state.borrow().scale;
            let delta = match distance {
                MouseScrollDistance::Lines { x, y, z: _ } => {
                    let lines = if x != 0.0 { x } else { y };
                    (lines * STRIP_SCROLL_LINE as f64 * scale).round() as i32
                }
                MouseScrollDistance::Pixels { x, y, z: _ } => {
                    if x != 0.0 { x as i32 } else { y as i32 }
                }
                MouseScrollDistance::Pages { x, y, z: _ } => {
                    let pages = if x != 0.0 { x } else { y };
                    (pages * self.state.borrow().rect.width() as f64).round() as i32
                }
            };
            self.strip_offset.set(self.strip_offset.get() + delta);
            self.clamp_strip_offset();
            return true;
        }

        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow().on_mouse_wheel_scroll(ui, Point::from(local), distance);
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        // While the strip has focus, Left/Right switch tabs. Both are
        // consumed even at the ends so horizontal arrow focus-traversal in
        // the parent Frame doesn't move focus away mid-strip; Up/Down are
        // left alone so vertical traversal still works.
        if self.strip_focused() {
            if let Some(code) = virtual_key_code {
                let active = self.active_tab.get();
                match code {
                    VirtualKeyCode::Left => {
                        if active > 0 {
                            self.change_tab(ui, active - 1);
                        }
                        return true;
                    }
                    VirtualKeyCode::Right => {
                        self.change_tab(ui, active + 1);
                        return true;
                    }
                    _ => {}
                }
            }
            return false;
        }
        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow().on_key_down(ui, virtual_key_code, scancode, state);
        }
        false
    }

    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        if self.strip_focused() {
            return false;
        }
        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow().on_key_up(ui, virtual_key_code, scancode, state);
        }
        false
    }

    fn on_key_char(&self, ui: &mut UI, unicode_codepoint: char, state: ModifiersState) -> bool {
        if self.strip_focused() {
            return false;
        }
        let active = self.active_tab.get();
        if active < self.views.len() {
            return self.views[active].borrow().on_key_char(ui, unicode_codepoint, state);
        }
        false
    }
}

impl Default for TabView {
    fn default() -> Self {
        // Focusable: the tab strip is a keyboard-focus stop (Left/Right
        // switch tabs); views inside tabs receive focus individually.
        let main = FieldsMain::with_rect(rect((0, 0), (400, 300)), Dimension::Max, Dimension::Max);
        TabView {
            state: RefCell::new(main),
            views: Vec::new(),
            tabs: Vec::new(),
            active_tab: Cell::new(0),
            tab_bar_height: Cell::new(0),
            strip_offset: Cell::new(0),
            closable: Cell::new(false),
            hover_close: Cell::new(None),
            close_glyph: RefCell::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"
    <Frame id="root" width="max" height="max" direction="vertical">
        <TabView id="tabs" width="max" height="max">
            <Frame id="one" width="max" height="max"/>
            <Frame id="two" width="max" height="max"/>
            <Frame id="three" width="max" height="max"/>
        </TabView>
    </Frame>"#;

    /// A laid-out TabView at (0,0), so local and absolute coordinates coincide.
    /// Unit tests run without fonts, so titles never shape and every tab falls
    /// back to the same fixed width — which is what makes the geometry here
    /// predictable.
    fn tabs_ui(width: i32, closable: bool) -> (UI, Element) {
        let ui = UI::from_xml(XML, width as u32, 300, Typeface::default(), 1.0).unwrap();
        let el = ui.get_view("tabs").unwrap();
        if closable {
            el.borrow().as_any().downcast_ref::<TabView>().unwrap().set_closable(true);
        }
        el.borrow_mut().layout_content(0, 0, width, 300, &Typeface::default(), 1.0);
        (ui, el)
    }

    fn with_tabs<R>(el: &Element, f: impl FnOnce(&TabView) -> R) -> R {
        let view = el.borrow();
        f(view.as_any().downcast_ref::<TabView>().unwrap())
    }

    #[test]
    fn remove_view_keeps_the_active_tab() {
        let (_ui, el) = tabs_ui(400, false);
        with_tabs(&el, |t| t.set_active_tab(2));

        assert!(el.borrow_mut().as_container_mut().unwrap().remove_view("one"));

        with_tabs(&el, |t| {
            assert_eq!(t.get_tab_count(), 2);
            // "three" was at 2, is now at 1, and is still the active tab.
            assert_eq!(t.get_active_tab(), 1);
            assert_eq!(t.get_tab_title(1).as_deref(), Some("three"));
        });
    }

    #[test]
    fn remove_view_clamps_when_the_active_tab_goes() {
        let (_ui, el) = tabs_ui(400, false);
        with_tabs(&el, |t| t.set_active_tab(2));

        assert!(el.borrow_mut().as_container_mut().unwrap().remove_view("three"));

        with_tabs(&el, |t| {
            assert_eq!(t.get_tab_count(), 2);
            assert_eq!(t.get_active_tab(), 1); // fell back to the new last tab
        });
        // Removing everything must not leave the index dangling.
        assert!(el.borrow_mut().as_container_mut().unwrap().remove_view("two"));
        assert!(el.borrow_mut().as_container_mut().unwrap().remove_view("one"));
        with_tabs(&el, |t| {
            assert_eq!(t.get_tab_count(), 0);
            assert_eq!(t.get_active_tab(), 0);
        });
    }

    #[test]
    fn wheel_pans_the_strip_and_clamps() {
        // Narrow enough that three tabs overflow by more than one scroll step,
        // whether or not the test environment has fonts to shape the titles with.
        let (mut ui, el) = tabs_ui(60, false);
        let (min_offset, strip_y) = with_tabs(&el, |t| {
            assert!(
                t.min_strip_offset() < -STRIP_SCROLL_LINE,
                "test needs a strip that overflows by over one scroll step, got {}",
                t.min_strip_offset()
            );
            (t.min_strip_offset(), t.tab_bar_height.get() / 2)
        });

        let pos = Point::new(50, strip_y);
        let down = MouseScrollDistance::Lines { x: 0.0, y: -1.0, z: 0.0 };
        assert!(el.borrow().on_mouse_wheel_scroll(&mut ui, pos, down));
        with_tabs(&el, |t| assert_eq!(t.strip_offset.get(), -(STRIP_SCROLL_LINE)));

        // Far past the end clamps at the last tab's right edge…
        for _ in 0..50 {
            el.borrow().on_mouse_wheel_scroll(&mut ui, pos, down);
        }
        with_tabs(&el, |t| assert_eq!(t.strip_offset.get(), min_offset));

        // …and scrolling back clamps at zero, never positive.
        let up = MouseScrollDistance::Lines { x: 0.0, y: 1.0, z: 0.0 };
        for _ in 0..50 {
            el.borrow().on_mouse_wheel_scroll(&mut ui, pos, up);
        }
        with_tabs(&el, |t| assert_eq!(t.strip_offset.get(), 0));
    }

    #[test]
    fn wheel_below_the_strip_does_not_pan() {
        let (mut ui, el) = tabs_ui(100, false);
        let below = with_tabs(&el, |t| Point::new(50, t.tab_bar_height.get() + 10));
        let down = MouseScrollDistance::Lines { x: 0.0, y: -1.0, z: 0.0 };
        el.borrow().on_mouse_wheel_scroll(&mut ui, below, down);
        with_tabs(&el, |t| assert_eq!(t.strip_offset.get(), 0));
    }

    #[test]
    fn close_button_click_reports_the_tab_and_does_not_switch() {
        let (mut ui, el) = tabs_ui(400, true);
        let seen: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let sink = Rc::clone(&seen);
        el.borrow_mut().on_event(
            EventType::TabClose,
            Box::new(move |_ui, _view, data| {
                if let EventData::Selected(index) = data {
                    sink.set(Some(*index));
                }
                true
            }),
        );

        // Aim at the middle of the second tab's close button.
        let target = with_tabs(&el, |t| {
            let r = t.tabs[1].close_rect;
            Point::new((r.min.x + r.max.x) / 2, t.tab_bar_height.get() / 2)
        });
        assert!(el.borrow().on_mouse_button_down(&mut ui, target, MouseButton::Left));

        assert_eq!(seen.get(), Some(1));
        // The click was consumed by the close button, not by tab switching.
        with_tabs(&el, |t| assert_eq!(t.get_active_tab(), 0));
    }

    #[test]
    fn clicking_a_tab_body_still_switches() {
        let (mut ui, el) = tabs_ui(400, true);
        let target = with_tabs(&el, |t| {
            let r = t.tabs[1].tab_rect;
            // Left edge side, well clear of the close button on the right.
            Point::new(r.min.x + 2, t.tab_bar_height.get() / 2)
        });
        assert!(el.borrow().on_mouse_button_down(&mut ui, target, MouseButton::Left));
        with_tabs(&el, |t| assert_eq!(t.get_active_tab(), 1));
    }
}
