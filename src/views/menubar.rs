use std::cell::RefCell;
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use crate::text::{TextBlock, TextOptions};
use speedy2d::window::MouseButton;

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Container, Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::{PopupDirection, PopupMode, UI};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::views::popupmenu::{MenuItem, PopupMenu};
use crate::views::separator::Separator;
use crate::view_base::{HasMainFields, ViewBasics};

/// Vertical padding above/below the title text (dips).
const BAR_PADDING_V: i32 = 4;
/// Horizontal padding inside each title cell (dips).
const TITLE_PADDING_H: i32 = 10;

/// One top-level menu of a MenuBar: a title and its dropdown items.
pub struct MenuData {
    pub title: String,
    pub items: Vec<MenuItem>,
}

/// A horizontal bar of menu titles. Place it as the first child of a
/// vertical root Frame (`width="max" height="min"`); clicking a title opens
/// the menu through the popup layer, so open menus never affect layout.
///
/// Fires `EventType::Click` when a leaf item anywhere in the menu chain is
/// activated; read the chosen item's ID with `clicked_item()`.
pub struct MenuBar {
    state: RefCell<FieldsMain>,
    menus: RefCell<Vec<MenuData>>,
    cached_titles: RefCell<Vec<Option<TextBlock>>>,
    hovered: RefCell<Option<usize>>,
    open_index: RefCell<Option<usize>>,
    open_popup: RefCell<Option<Element>>,
    clicked_item: RefCell<Option<String>>,
}

impl HasMainFields for MenuBar {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for MenuBar {}

#[allow(dead_code)]
impl MenuBar {
    pub fn new() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (0, 0)), Dimension::Max, Dimension::Min);
        main.state.focusable = false;
        MenuBar {
            state: RefCell::new(main),
            menus: RefCell::new(Vec::new()),
            cached_titles: RefCell::new(Vec::new()),
            hovered: RefCell::new(None),
            open_index: RefCell::new(None),
            open_popup: RefCell::new(None),
            clicked_item: RefCell::new(None),
        }
    }

    /// Adds a top-level menu with its dropdown items.
    pub fn add_menu(&mut self, title: &str, items: Vec<MenuItem>) {
        self.menus.borrow_mut().push(MenuData { title: title.to_owned(), items });
        self.cached_titles.borrow_mut().push(None);
    }

    /// The ID of the most recently activated menu item (set before the
    /// Click event fires).
    pub fn clicked_item(&self) -> Option<String> {
        self.clicked_item.borrow().clone()
    }

    pub(crate) fn set_clicked_item(&self, id: &str) {
        *self.clicked_item.borrow_mut() = Some(id.to_owned());
    }

    /// Called by the menu chain when it closes itself (Esc, item activated).
    pub(crate) fn menu_closed(&self) {
        *self.open_index.borrow_mut() = None;
        *self.open_popup.borrow_mut() = None;
    }

    /// Opens the previous/next menu (Left/Right from an open dropdown).
    pub(crate) fn cycle_menu(&self, ui: &mut UI, dir: i32) {
        let n = self.menus.borrow().len() as i32;
        if n == 0 {
            return;
        }
        let current = match *self.open_index.borrow() {
            Some(i) => i as i32,
            None => return,
        };
        let next = (current + dir).rem_euclid(n) as usize;
        self.open_menu(ui, next, true);
    }

    fn layout_titles(&self, typeface: &Typeface, scale: f64) {
        let menus = self.menus.borrow();
        let mut cached = self.cached_titles.borrow_mut();
        // Same resolution as the dropdown items: explicit size wins,
        // otherwise the palette's "menu" typeface role decides.
        let base_size = typeface.font_size.unwrap_or_else(|| crate::drawing::current_text_size("menu"));
        let text_size = base_size * scale as f32;
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            for (i, menu) in menus.iter().enumerate() {
                if cached[i].is_none() {
                    let block = font.layout_text(&menu.title, text_size, TextOptions::new());
                    cached[i] = Some(block);
                }
            }
        }
    }

    /// Local X offset of title `index` within the bar.
    fn title_offset(&self, index: usize) -> i32 {
        let scale = self.state.borrow().scale;
        let pad_h = (TITLE_PADDING_H as f64 * scale).round() as i32;
        let cached = self.cached_titles.borrow();
        let mut x = 0;
        for block in cached.iter().take(index).flatten() {
            x += block.width().ceil() as i32 + 2 * pad_h;
        }
        x
    }

    fn title_width(&self, index: usize) -> i32 {
        let scale = self.state.borrow().scale;
        let pad_h = (TITLE_PADDING_H as f64 * scale).round() as i32;
        match self.cached_titles.borrow().get(index) {
            Some(Some(block)) => block.width().ceil() as i32 + 2 * pad_h,
            _ => 0,
        }
    }

    fn hit_title(&self, x: i32, y: i32) -> Option<usize> {
        let r = self.state.borrow().rect;
        if !r.hit((x, y)) {
            return None;
        }
        let local_x = x - r.min.x;
        let mut acc = 0;
        for i in 0..self.menus.borrow().len() {
            let w = self.title_width(i);
            if local_x < acc + w {
                return Some(i);
            }
            acc += w;
        }
        None
    }

    /// Finds our own Element by pointer identity among the parent's children
    /// (a view does not otherwise know its own `Rc` wrapper).
    fn self_element(&self) -> Option<Element> {
        let parent = self.get_parent()?;
        let me = self as *const MenuBar as *const ();
        let parent_ref = parent.borrow();
        let container = parent_ref.as_container()?;
        container.get_views().iter()
            .find(|el| {
                let b = el.borrow();
                std::ptr::eq(b.as_any() as *const dyn std::any::Any as *const (), me)
            })
            .cloned()
    }

    fn open_menu(&self, ui: &mut UI, index: usize, select_first: bool) {
        self.close_menu(ui);
        let items = match self.menus.borrow().get(index) {
            Some(menu) => menu.items.clone(),
            None => return,
        };
        if items.is_empty() {
            return;
        }
        let mut menu = PopupMenu::new();
        menu.set_id(&format!("{}_menu{}", self.get_id(), index));
        menu.set_items(items);
        if select_first {
            menu.move_selection(1);
        }
        if let Some(self_el) = self.self_element() {
            menu.set_owner(Rc::downgrade(&self_el));
        }
        let el: Element = Rc::new(RefCell::new(menu));
        let pos = self.get_absolute_position();
        let height = self.state.borrow().rect.height();
        let x = pos.x + self.title_offset(index);
        *self.open_index.borrow_mut() = Some(index);
        *self.open_popup.borrow_mut() = Some(Rc::clone(&el));
        ui.show_popup(el, x, pos.y + height, PopupDirection::BottomRight, PopupMode::Popup);
    }

    fn close_menu(&self, ui: &mut UI) {
        if let Some(el) = self.open_popup.borrow_mut().take() {
            {
                let e = el.borrow();
                if let Some(menu) = e.as_any().downcast_ref::<PopupMenu>() {
                    menu.close_submenu(ui);
                }
            }
            ui.remove_overlay(&el);
        }
        *self.open_index.borrow_mut() = None;
    }

    /// Clears stale open state — the popup may have been dismissed by a
    /// click outside or closed itself. Returns true when state was cleared,
    /// so callers can request a redraw to drop the title highlight.
    fn sync_open(&self, ui: &UI) -> bool {
        let stale = match &*self.open_popup.borrow() {
            Some(el) => !ui.overlay_exists(el),
            None => false,
        };
        if stale {
            self.menu_closed();
        }
        stale
    }
}

impl View for MenuBar {
    fn set_any(&mut self, name: &str, value: &str) {
        let _ = self.base_set_any(name, value);
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, _height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        let typeface = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(typeface.clone()));
        self.base_set_scale(scale);
        self.layout_titles(&typeface, scale);

        let pad_v = (BAR_PADDING_V as f64 * scale).round() as i32;
        let text_h = self.cached_titles.borrow().iter().flatten()
            .map(|b| b.height().ceil() as i32)
            .max()
            .unwrap_or((crate::drawing::current_text_size("menu") * scale as f32).ceil() as i32);
        let bar_h = text_h + 2 * pad_v;

        let r = rect((x, y), (x + width, y + bar_h));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let (cw, ch) = self.get_content_size();
        cw <= width && ch <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);
        let scale = state.scale;

        theme.push_clip();
        theme.clip_rect(r);
        theme.draw_rect(r, theme.color("background"));

        let pad_h = (TITLE_PADDING_H as f64 * scale).round() as i32;
        let hovered = *self.hovered.borrow();
        let open = *self.open_index.borrow();
        let cached = self.cached_titles.borrow();

        let mut x = r.min.x;
        for (i, block) in cached.iter().enumerate() {
            let Some(block) = block else { continue; };
            let w = block.width().ceil() as i32 + 2 * pad_h;
            let cell = rect((x, r.min.y), (x + w, r.max.y));
            let text_color = if open == Some(i) || (open.is_none() && hovered == Some(i)) {
                theme.draw_rect(cell, theme.color("menu_highlight"));
                theme.color("menu_highlight_text")
            } else {
                theme.color("text")
            };
            let text_y = r.min.y + (cell.height() as f32 - block.height()) as i32 / 2;
            theme.draw_text((x + pad_h) as f32, text_y as f32, text_color, block);
            x += w;
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

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let scale = self.state.borrow().scale;
        let pad_h = (TITLE_PADDING_H as f64 * scale).round() as i32;
        let pad_v = (BAR_PADDING_V as f64 * scale).round() as i32;
        let cached = self.cached_titles.borrow();
        let w: i32 = cached.iter().flatten()
            .map(|b| b.width().ceil() as i32 + 2 * pad_h)
            .sum();
        let h = cached.iter().flatten()
            .map(|b| b.height().ceil() as i32)
            .max()
            .unwrap_or(0) + 2 * pad_v;
        (w, h)
    }

    fn is_focused(&self) -> bool {
        self.base_is_focused()
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
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
        Some(self)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self)
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

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() {
            return false;
        }
        self.base_fire_event(ui, EventType::Click, &EventData::None)
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        // Drop the title highlight as soon as the menu chain is dismissed by
        // a click outside (the bar never sees that click).
        self.sync_open(ui)
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        // Redraw when stale open state is cleared (menu chain dismissed by a
        // click outside) so the title highlight goes away.
        let cleared = self.sync_open(ui);
        let hit = self.hit_title(position.x, position.y);
        let old = *self.hovered.borrow();
        *self.hovered.borrow_mut() = hit;
        // Copy the open index out BEFORE the if: a let-chain scrutinee's
        // RefCell borrow stays alive inside the block (unlike plain if-let),
        // and open_menu needs to borrow_mut the same cell.
        let open = *self.open_index.borrow();
        // While a menu is open, hovering another title switches to its menu.
        if let (Some(i), Some(o)) = (hit, open)
            && o != i {
            self.open_menu(ui, i, false);
            return true;
        }
        cleared || old != hit
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() || !matches!(button, MouseButton::Left) {
            return false;
        }
        self.sync_open(ui);
        let Some(i) = self.hit_title(position.x, position.y) else {
            return false;
        };
        // A click on the open title's own menu never reaches us — the UI
        // closes all popups first — so any click here opens a menu.
        self.open_menu(ui, i, false);
        true
    }
}

impl Container for MenuBar {
    fn add_view(&mut self, view: Element) {
        // The XML parser hands us children unconditionally; consume <Menu>
        // data holders, ignore anything else.
        let mut borrowed = view.borrow_mut();
        if let Some(menu) = borrowed.as_any_mut().downcast_mut::<Menu>() {
            let title = menu.title.borrow().clone();
            let items = std::mem::take(&mut *menu.items.borrow_mut());
            drop(borrowed);
            self.add_menu(&title, items);
        }
    }

    fn get_view(&self, _id: &str) -> Option<Element> {
        None
    }

    fn get_view_count(&self) -> usize {
        0
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        MenuBar::new()
    }
}

// =========================================================================
// Menu — XML-only data holder: a titled list of menu items. Nested <Menu>
// elements become submenus.
// =========================================================================

pub struct Menu {
    state: RefCell<FieldsMain>,
    pub(crate) title: RefCell<String>,
    pub(crate) items: RefCell<Vec<MenuItem>>,
}

impl HasMainFields for Menu {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for Menu {}

impl Default for Menu {
    fn default() -> Self {
        Menu {
            state: RefCell::new(FieldsMain::with_rect(rect((0, 0), (0, 0)), Dimension::Min, Dimension::Min)),
            title: RefCell::new(String::new()),
            items: RefCell::new(Vec::new()),
        }
    }
}

impl View for Menu {
    fn set_any(&mut self, name: &str, value: &str) {
        if name == "title" {
            *self.title.borrow_mut() = value.to_owned();
            return;
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
    fn set_padding(&self, t: i32, l: i32, r: i32, b: i32) { self.base_set_padding(t, l, r, b); }
    fn set_margin(&self, t: i32, l: i32, r: i32, b: i32) { self.base_set_margin(t, l, r, b); }
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

impl Container for Menu {
    fn add_view(&mut self, view: Element) {
        let mut borrowed = view.borrow_mut();
        if let Some(item) = borrowed.as_any_mut().downcast_mut::<MenuItemTag>() {
            let id = item.get_id();
            let text = item.text.borrow().clone();
            let icon = item.icon.borrow().clone();
            self.items.borrow_mut().push(MenuItem {
                id,
                icon_path: icon,
                text,
                separator: false,
                children: Vec::new(),
            });
        } else if let Some(sub) = borrowed.as_any_mut().downcast_mut::<Menu>() {
            // A nested <Menu> becomes a submenu item.
            let id = sub.get_id();
            let text = sub.title.borrow().clone();
            let children = std::mem::take(&mut *sub.items.borrow_mut());
            self.items.borrow_mut().push(MenuItem {
                id,
                icon_path: String::new(),
                text,
                separator: false,
                children,
            });
        } else if borrowed.as_any().downcast_ref::<Separator>().is_some() {
            self.items.borrow_mut().push(MenuItem {
                id: String::new(),
                icon_path: String::new(),
                text: String::new(),
                separator: true,
                children: Vec::new(),
            });
        }
    }

    fn get_view(&self, _id: &str) -> Option<Element> {
        None
    }

    fn get_view_count(&self) -> usize {
        0
    }
}

// =========================================================================
// MenuItemTag — XML-only data holder for a single <MenuItem .../>.
// =========================================================================

pub struct MenuItemTag {
    state: RefCell<FieldsMain>,
    pub(crate) text: RefCell<String>,
    pub(crate) icon: RefCell<String>,
}

impl HasMainFields for MenuItemTag {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for MenuItemTag {}

impl Default for MenuItemTag {
    fn default() -> Self {
        MenuItemTag {
            state: RefCell::new(FieldsMain::with_rect(rect((0, 0), (0, 0)), Dimension::Min, Dimension::Min)),
            text: RefCell::new(String::new()),
            icon: RefCell::new(String::new()),
        }
    }
}

impl View for MenuItemTag {
    fn set_any(&mut self, name: &str, value: &str) {
        match name {
            "text" => {
                *self.text.borrow_mut() = value.to_owned();
                return;
            }
            "icon" => {
                *self.icon.borrow_mut() = value.to_owned();
                return;
            }
            _ => {}
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
    fn set_padding(&self, t: i32, l: i32, r: i32, b: i32) { self.base_set_padding(t, l, r, b); }
    fn set_margin(&self, t: i32, l: i32, r: i32, b: i32) { self.base_set_margin(t, l, r, b); }
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

#[cfg(test)]
mod tests {
    use super::*;
    use speedy2d::window::{ModifiersState, VirtualKeyCode};

    const XML: &str = r#"
    <Frame id="root" width="max" height="max" direction="vertical">
        <MenuBar id="bar">
            <Menu title="File">
                <MenuItem id="new" text="New"/>
                <Menu id="recent" title="Recent">
                    <MenuItem id="r1" text="One"/>
                </Menu>
                <Separator/>
                <MenuItem id="exit" text="Exit"/>
            </Menu>
            <Menu title="Help">
                <MenuItem id="about" text="About"/>
            </Menu>
        </MenuBar>
    </Frame>"#;

    fn bar_ui() -> (UI, Element) {
        let ui = UI::from_xml(XML, 800, 600, Typeface::default(), 1.0).unwrap();
        let bar = ui.get_view("bar").unwrap();
        (ui, bar)
    }

    #[test]
    fn xml_parses_into_menu_data() {
        let (_ui, bar_el) = bar_ui();
        let bar = bar_el.borrow();
        let bar = bar.as_any().downcast_ref::<MenuBar>().unwrap();
        let menus = bar.menus.borrow();
        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].title, "File");
        assert_eq!(menus[0].items.len(), 4);
        assert_eq!(menus[0].items[0].id, "new");
        assert_eq!(menus[0].items[1].text, "Recent");
        assert_eq!(menus[0].items[1].children.len(), 1);
        assert_eq!(menus[0].items[1].children[0].id, "r1");
        assert!(menus[0].items[2].separator);
        assert_eq!(menus[0].items[3].id, "exit");
        assert_eq!(menus[1].title, "Help");
    }

    #[test]
    fn open_and_cycle_menus() {
        let (mut ui, bar_el) = bar_ui();
        let bar = bar_el.borrow();
        let bar = bar.as_any().downcast_ref::<MenuBar>().unwrap();
        bar.open_menu(&mut ui, 0, false);
        assert!(ui.is_popup_open("bar_menu0"));
        bar.cycle_menu(&mut ui, 1);
        assert!(!ui.is_popup_open("bar_menu0"));
        assert!(ui.is_popup_open("bar_menu1"));
        bar.cycle_menu(&mut ui, 1); // wraps around
        assert!(ui.is_popup_open("bar_menu0"));
        bar.close_menu(&mut ui);
        assert!(!ui.has_popups());
        assert!(bar.open_index.borrow().is_none());
    }

    #[test]
    fn keyboard_enter_routes_clicked_item_to_bar() {
        let (mut ui, bar_el) = bar_ui();
        let popup = {
            let bar = bar_el.borrow();
            let bar = bar.as_any().downcast_ref::<MenuBar>().unwrap();
            bar.open_menu(&mut ui, 1, true); // "Help", first item pre-selected
            bar.open_popup.borrow().as_ref().cloned().unwrap()
        };
        let handled = popup.borrow().on_key_down(&mut ui, Some(VirtualKeyCode::Return), 0, ModifiersState::default());
        assert!(handled);
        assert!(!ui.has_popups());
        let bar = bar_el.borrow();
        let bar = bar.as_any().downcast_ref::<MenuBar>().unwrap();
        assert_eq!(bar.clicked_item().as_deref(), Some("about"));
        assert!(bar.open_index.borrow().is_none());
    }
}
