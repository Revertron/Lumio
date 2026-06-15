use std::cell::{Cell, RefCell};
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use crate::text::{TextBlock, TextOptions};
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::image_source::ImageSource;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::{PopupDirection, PopupMode, UI};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::view_base::{HasMainFields, ViewBasics};

const ICON_SIZE: i32 = 16;
const ITEM_HEIGHT: i32 = 24;
const SEPARATOR_HEIGHT: i32 = 3;
const ICON_TEXT_GAP: i32 = 6;
const ITEM_PADDING_LEFT: i32 = 6;
const ITEM_PADDING_RIGHT: i32 = 12;
/// Width always reserved at the right edge for the submenu arrow (and,
/// later, accelerator text) — Windows menus keep this column too (dips).
const ARROW_AREA: i32 = 16;
/// Horizontal overlap of a submenu over its parent menu (dips).
const SUBMENU_OVERLAP: i32 = 2;

/// Data for a single menu item.
#[derive(Clone)]
pub struct MenuItem {
    pub id: String,
    pub icon_path: String,
    pub text: String,
    pub separator: bool,
    /// Non-empty = this item opens a submenu instead of firing a click.
    pub children: Vec<MenuItem>,
}

pub struct PopupMenu {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<MenuItem>>,
    /// One slot per item; `None` for items without an icon. Parallel to `items`.
    icons: RefCell<Vec<Option<ImageSource>>>,
    cached_texts: RefCell<Vec<Option<TextBlock>>>,
    hovered: RefCell<Option<usize>>,
    pressed: RefCell<Option<usize>>,
    /// The view (a MenuBar, or the root PopupMenu of a context-menu chain)
    /// that is notified when a leaf item is activated anywhere in the chain.
    owner: RefCell<Option<WeakElement>>,
    /// True when this popup was opened as a submenu of another PopupMenu.
    is_submenu: Cell<bool>,
    /// Index and element of the currently open child submenu.
    submenu_open: RefCell<Option<(usize, Element)>>,
    /// ID of the most recently activated leaf item (set before Click fires).
    clicked_item: RefCell<Option<String>>,
}

impl HasMainFields for PopupMenu {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for PopupMenu {}

#[allow(dead_code)]
impl PopupMenu {
    pub fn new() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (120, 100)), Dimension::Min, Dimension::Min);
        main.padding = Borders::with_padding(2);
        main.state.focusable = false;
        PopupMenu {
            state: RefCell::new(main),
            items: RefCell::new(Vec::new()),
            icons: RefCell::new(Vec::new()),
            cached_texts: RefCell::new(Vec::new()),
            hovered: RefCell::new(None),
            pressed: RefCell::new(None),
            owner: RefCell::new(None),
            is_submenu: Cell::new(false),
            submenu_open: RefCell::new(None),
            clicked_item: RefCell::new(None),
        }
    }

    /// Adds a menu item with icon and text.
    pub fn add_item(&mut self, id: &str, icon_path: &str, text: &str) {
        self.items.borrow_mut().push(MenuItem {
            id: id.to_owned(),
            icon_path: icon_path.to_owned(),
            text: text.to_owned(),
            separator: false,
            children: Vec::new(),
        });
        self.icons.borrow_mut().push(ImageSource::for_path(icon_path));
        self.cached_texts.borrow_mut().push(None);
    }

    /// Adds an item that opens a submenu with the given child items.
    pub fn add_submenu(&mut self, id: &str, icon_path: &str, text: &str, children: Vec<MenuItem>) {
        self.items.borrow_mut().push(MenuItem {
            id: id.to_owned(),
            icon_path: icon_path.to_owned(),
            text: text.to_owned(),
            separator: false,
            children,
        });
        self.icons.borrow_mut().push(ImageSource::for_path(icon_path));
        self.cached_texts.borrow_mut().push(None);
    }

    /// Adds a horizontal separator line between menu items.
    pub fn add_separator(&mut self) {
        self.items.borrow_mut().push(MenuItem {
            id: String::new(),
            icon_path: String::new(),
            text: String::new(),
            separator: true,
            children: Vec::new(),
        });
        self.icons.borrow_mut().push(None);
        self.cached_texts.borrow_mut().push(None);
    }

    /// Replaces all items at once (used by MenuBar and submenu creation).
    pub fn set_items(&mut self, items: Vec<MenuItem>) {
        let n = items.len();
        *self.icons.borrow_mut() = items
            .iter()
            .map(|it| ImageSource::for_path(&it.icon_path))
            .collect();
        *self.cached_texts.borrow_mut() = (0..n).map(|_| None).collect();
        *self.items.borrow_mut() = items;
        *self.hovered.borrow_mut() = None;
    }

    /// Sets the view notified when a leaf item is activated anywhere in this
    /// menu's chain. Set automatically by MenuBar and for submenus.
    pub fn set_owner(&self, owner: WeakElement) {
        *self.owner.borrow_mut() = Some(owner);
    }

    /// The ID of the most recently activated leaf item. Read this from a
    /// Click handler to learn which item (including submenu items) was chosen.
    pub fn clicked_item(&self) -> Option<String> {
        self.clicked_item.borrow().clone()
    }

    pub(crate) fn set_clicked_item(&self, id: &str) {
        *self.clicked_item.borrow_mut() = Some(id.to_owned());
    }

    /// Returns the index of the currently hovered item.
    pub fn get_hovered_index(&self) -> Option<usize> {
        *self.hovered.borrow()
    }

    /// Returns a reference to the items. Caller must not hold borrow across mutations.
    pub fn item_count(&self) -> usize {
        self.items.borrow().len()
    }

    fn load_icons(&self) {
        for src in self.icons.borrow_mut().iter_mut().flatten() {
            src.ensure_loaded();
        }
    }

    fn layout_texts(&self, typeface: &Typeface, scale: f64) {
        let items = self.items.borrow();
        let mut cached = self.cached_texts.borrow_mut();
        // Explicit (own or inherited) size wins; otherwise the palette's
        // "menu" typeface role decides.
        let base_size = typeface.font_size.unwrap_or_else(|| crate::drawing::current_text_size("menu"));
        let text_size = base_size * scale as f32;
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            for (i, item) in items.iter().enumerate() {
                if cached[i].is_none() {
                    let options = TextOptions::new();
                    let block = font.layout_text(&item.text, text_size, options);
                    cached[i] = Some(block);
                }
            }
        }
    }

    fn get_hit_item(&self, x: i32, y: i32) -> Option<usize> {
        let state = self.state.borrow();
        let r = state.rect;
        if !r.hit((x, y)) {
            return None;
        }
        let scale = state.scale;
        let padding = state.padding.scaled(scale);
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;
        let sep_h = (SEPARATOR_HEIGHT as f64 * scale).round() as i32;
        let local_y = y - r.min.y - padding.top;
        if local_y < 0 {
            return None;
        }
        let items = self.items.borrow();
        let mut accumulated = 0;
        for (i, item) in items.iter().enumerate() {
            let h = if item.separator { sep_h } else { item_h };
            if local_y < accumulated + h {
                return if item.separator { None } else { Some(i) };
            }
            accumulated += h;
        }
        None
    }

    /// Y offset of an item's top edge within the menu rect.
    fn item_top(&self, index: usize) -> i32 {
        let state = self.state.borrow();
        let scale = state.scale;
        let padding = state.padding.scaled(scale);
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;
        let sep_h = (SEPARATOR_HEIGHT as f64 * scale).round() as i32;
        let mut y = padding.top;
        for (i, item) in self.items.borrow().iter().enumerate() {
            if i == index {
                break;
            }
            y += if item.separator { sep_h } else { item_h };
        }
        y
    }

    /// Moves the keyboard selection by `dir` (+1/-1), skipping separators
    /// and wrapping around.
    pub(crate) fn move_selection(&self, dir: i32) {
        let items = self.items.borrow();
        let n = items.len() as i32;
        if n == 0 {
            return;
        }
        let start = match *self.hovered.borrow() {
            Some(i) => i as i32,
            None => if dir > 0 { -1 } else { n },
        };
        let mut idx = start;
        for _ in 0..n {
            idx = (idx + dir).rem_euclid(n);
            if !items[idx as usize].separator {
                drop(items);
                *self.hovered.borrow_mut() = Some(idx as usize);
                return;
            }
        }
    }

    /// Activates an item: opens its submenu, or — for a leaf — records the
    /// clicked ID, fires Click on self and on the owner, and closes the chain.
    fn activate_item(&self, ui: &mut UI, index: usize) {
        let (item_id, has_children) = {
            let items = self.items.borrow();
            let Some(item) = items.get(index) else { return; };
            if item.separator {
                return;
            }
            (item.id.clone(), !item.children.is_empty())
        };
        if has_children {
            self.open_submenu(ui, index, true);
            return;
        }
        self.set_clicked_item(&item_id);
        self.click(ui);
        if let Some(owner) = self.owner.borrow().as_ref().and_then(|w| w.upgrade()) {
            notify_owner_clicked(&owner, &item_id);
            owner.borrow().click(ui);
        }
        ui.close_all_popups();
    }

    /// Opens the submenu of `index` to the right of the item. `select_first`
    /// pre-selects the first child item (keyboard activation).
    fn open_submenu(&self, ui: &mut UI, index: usize, select_first: bool) {
        if let Some((open_idx, _)) = &*self.submenu_open.borrow() && *open_idx == index {
            return;
        }
        self.close_submenu(ui);
        let children = match self.items.borrow().get(index) {
            Some(item) => item.children.clone(),
            None => return,
        };
        if children.is_empty() {
            return;
        }
        // Locate ourselves among the overlays to learn our window position.
        let Some((self_el, ox, oy)) = ui.find_self_overlay(self) else { return; };
        let mut sub = PopupMenu::new();
        sub.set_id(&format!("{}_sub{}", self.get_id(), index));
        sub.set_items(children);
        sub.is_submenu.set(true);
        if select_first {
            sub.move_selection(1);
        }
        // Leaf clicks in the child route to our owner (menu bar), or to us
        // (root context menu) — whichever ultimately holds the Click listener.
        let owner = match self.owner.borrow().clone() {
            Some(w) => w,
            None => Rc::downgrade(&self_el),
        };
        sub.set_owner(owner);
        let el: Element = Rc::new(RefCell::new(sub));
        let r = self.state.borrow().rect;
        let scale = self.state.borrow().scale;
        let overlap = (SUBMENU_OVERLAP as f64 * scale).round() as i32;
        let x = ox + r.width() - overlap;
        let y = oy + self.item_top(index);
        *self.submenu_open.borrow_mut() = Some((index, Rc::clone(&el)));
        ui.show_popup(el, x, y, PopupDirection::BottomRight, PopupMode::Popup);
    }

    /// Closes the open child submenu (and its descendants).
    pub(crate) fn close_submenu(&self, ui: &mut UI) {
        if let Some((_, child)) = self.submenu_open.borrow_mut().take() {
            {
                let c = child.borrow();
                if let Some(menu) = c.as_any().downcast_ref::<PopupMenu>() {
                    menu.close_submenu(ui);
                }
            }
            ui.remove_overlay(&child);
        }
    }

    /// Clears stale submenu state — the child may have closed itself (Esc,
    /// Left) or been dismissed by a click outside the chain.
    fn sync_submenu(&self, ui: &UI) {
        let stale = match &*self.submenu_open.borrow() {
            Some((_, el)) => !ui.overlay_exists(el),
            None => false,
        };
        if stale {
            *self.submenu_open.borrow_mut() = None;
        }
    }

    /// Closes this menu (and any open descendants).
    fn close_self(&self, ui: &mut UI) {
        self.close_submenu(ui);
        if let Some((el, _, _)) = ui.find_self_overlay(self) {
            ui.remove_overlay(&el);
        }
        if !self.is_submenu.get()
            && let Some(owner) = self.owner.borrow().as_ref().and_then(|w| w.upgrade()) {
            notify_owner_closed(&owner);
        }
    }

    /// Left/Right at the root of a menu chain: ask the owner (a MenuBar) to
    /// open the previous/next menu. No-op for context menus.
    fn owner_cycle(&self, ui: &mut UI, dir: i32) {
        if let Some(owner) = self.owner.borrow().as_ref().and_then(|w| w.upgrade()) {
            notify_owner_cycle(ui, &owner, dir);
        }
    }
}

// Owner notifications live as free functions so a future owner type only
// touches this spot.
fn notify_owner_clicked(owner: &Element, item_id: &str) {
    let o = owner.borrow();
    if let Some(menu) = o.as_any().downcast_ref::<PopupMenu>() {
        menu.set_clicked_item(item_id);
    } else if let Some(bar) = o.as_any().downcast_ref::<crate::views::MenuBar>() {
        bar.set_clicked_item(item_id);
        bar.menu_closed();
    }
}

fn notify_owner_closed(owner: &Element) {
    let o = owner.borrow();
    if let Some(bar) = o.as_any().downcast_ref::<crate::views::MenuBar>() {
        bar.menu_closed();
    }
}

fn notify_owner_cycle(ui: &mut UI, owner: &Element, dir: i32) {
    let o = owner.borrow();
    if let Some(bar) = o.as_any().downcast_ref::<crate::views::MenuBar>() {
        bar.cycle_menu(ui, dir);
    }
}

impl View for PopupMenu {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        let _ = (name, value);
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        let typeface = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(typeface.clone()));
        self.base_set_scale(scale);
        self.load_icons();
        self.layout_texts(&typeface, scale);

        let padding = self.get_padding(scale);
        let icon_size = (ICON_SIZE as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP as f64 * scale).round() as i32;
        let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as i32;
        let pad_right = (ITEM_PADDING_RIGHT as f64 * scale).round() as i32;
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;

        // Calculate max text width
        let mut max_text_w = 0i32;
        {
            let cached = self.cached_texts.borrow();
            for text in cached.iter().flatten() {
                let w = text.width().ceil() as i32;
                if w > max_text_w {
                    max_text_w = w;
                }
            }
        }

        let sep_h = (SEPARATOR_HEIGHT as f64 * scale).round() as i32;
        let arrow_w = (ARROW_AREA as f64 * scale).round() as i32;
        let min_w = (crate::drawing::current_dimension("menu.min_width") as f64 * scale).round() as i32;

        let content_w = (pad_left + icon_size + gap + max_text_w + arrow_w + pad_right).max(min_w);
        let content_h: i32 = self.items.borrow().iter()
            .map(|item| if item.separator { sep_h } else { item_h })
            .sum();

        let total_w = (padding.left + content_w + padding.right).min(width);
        let total_h = (padding.top + content_h + padding.bottom).min(height);

        let r = rect((x, y), (x + total_w, y + total_h));
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

        // Draw background (same frame as Button)
        theme.draw_component("button.back", r, state.state);

        let padding = state.padding.scaled(scale);
        let icon_size = (ICON_SIZE as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP as f64 * scale).round() as i32;
        let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as i32;
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;

        let hovered = *self.hovered.borrow();
        let items = self.items.borrow();
        let cached = self.cached_texts.borrow();
        let mut icons = self.icons.borrow_mut();

        let content_x = r.min.x + padding.left;
        let mut y = r.min.y + padding.top;

        let sep_h = (SEPARATOR_HEIGHT as f64 * scale).round() as i32;

        for (i, item) in items.iter().enumerate() {
            if item.separator {
                // Draw separator line spanning full item width
                let sep_rect = rect(
                    (content_x, y),
                    (r.max.x - padding.right - 1, y + sep_h),
                );
                theme.draw_component("separator.h", sep_rect, state.state);
                y += sep_h;
                continue;
            }

            let item_rect = rect(
                (content_x, y),
                (r.max.x - padding.right - 1, y + item_h),
            );

            // Highlight hovered item
            let text_color = if hovered == Some(i) {
                theme.draw_rect(item_rect, theme.color("menu_highlight"));
                theme.color("menu_highlight_text")
            } else {
                theme.color("text")
            };

            // Draw icon, tinted to the item's text color so monochrome (white)
            // icons match the text and stay visible in any theme.
            if let Some(Some(src)) = icons.get_mut(i) {
                let icon_y = y + (item_h - icon_size) / 2;
                let icon_rect = rect(
                    (content_x + pad_left, icon_y),
                    (content_x + pad_left + icon_size, icon_y + icon_size),
                );
                src.draw(theme, icon_rect, text_color);
            }

            // Draw text
            if let Some(Some(text)) = cached.get(i) {
                let text_x = content_x + pad_left + icon_size + gap;
                let text_y = y + (item_h as f32 - text.height()) as i32 / 2;
                theme.draw_text(text_x as f32, text_y as f32, text_color, text);
            }

            // Submenu arrow at the right edge
            if !item.children.is_empty() {
                let arrow_w = (ARROW_AREA as f64 * scale).round() as i32;
                let arrow_rect = rect(
                    (item_rect.max.x - arrow_w, y + (item_h - arrow_w) / 2),
                    (item_rect.max.x, y + (item_h + arrow_w) / 2),
                );
                let mut arrow_state = state.state;
                arrow_state.hovered = hovered == Some(i);
                theme.draw_component("menu.arrow", arrow_rect, arrow_state);
            }

            y += item_h;
        }

        // Draw border frame (same as Button)
        theme.draw_component("button.body", r, state.state);

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
        let state = self.state.borrow();
        let scale = state.scale;
        let icon_size = (ICON_SIZE as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP as f64 * scale).round() as i32;
        let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as i32;
        let pad_right = (ITEM_PADDING_RIGHT as f64 * scale).round() as i32;
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;

        let mut max_text_w = 0i32;
        let cached = self.cached_texts.borrow();
        for text in cached.iter().flatten() {
            let w = text.width().ceil() as i32;
            if w > max_text_w {
                max_text_w = w;
            }
        }

        let sep_h = (SEPARATOR_HEIGHT as f64 * scale).round() as i32;
        let arrow_w = (ARROW_AREA as f64 * scale).round() as i32;
        let min_w = (crate::drawing::current_dimension("menu.min_width") as f64 * scale).round() as i32;

        let w = (pad_left + icon_size + gap + max_text_w + arrow_w + pad_right).max(min_w);
        let h: i32 = self.items.borrow().iter()
            .map(|item| if item.separator { sep_h } else { item_h })
            .sum();
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
        self.base_fire_event(ui, EventType::Click, &EventData::None)
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        self.sync_submenu(ui);
        let hit_item = self.get_hit_item(position.x, position.y);
        let old = *self.hovered.borrow();
        match hit_item {
            Some(i) => {
                *self.hovered.borrow_mut() = Some(i);
                let has_children = !self.items.borrow()[i].children.is_empty();
                if has_children {
                    self.open_submenu(ui, i, false);
                } else {
                    // Hovering a leaf closes a sibling's open submenu.
                    self.close_submenu(ui);
                }
            }
            None => {
                // Keep the highlight while the pointer travels into our open
                // submenu; clear it only when no submenu is open.
                if self.submenu_open.borrow().is_none() {
                    *self.hovered.borrow_mut() = None;
                }
            }
        }
        old != *self.hovered.borrow()
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let hit = self.get_hit_item(position.x, position.y);
        *self.pressed.borrow_mut() = hit;
        hit.is_some()
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let pressed = self.pressed.borrow_mut().take();
        let hit = self.get_hit_item(position.x, position.y);
        if let (Some(p), Some(h)) = (pressed, hit) {
            if p == h {
                self.activate_item(ui, h);
                return true;
            }
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        self.sync_submenu(ui);
        // An open child (topmost overlay) sees keys before us; anything that
        // reaches us while a child is open is swallowed — menus are modal.
        if self.submenu_open.borrow().is_some() {
            return true;
        }
        let Some(vk) = virtual_key_code else { return true; };
        match vk {
            VirtualKeyCode::Down => {
                self.move_selection(1);
                true
            }
            VirtualKeyCode::Up => {
                self.move_selection(-1);
                true
            }
            VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                let hovered = *self.hovered.borrow();
                if let Some(i) = hovered {
                    self.activate_item(ui, i);
                }
                true
            }
            VirtualKeyCode::Right => {
                let hovered = *self.hovered.borrow();
                if let Some(i) = hovered && !self.items.borrow()[i].children.is_empty() {
                    self.open_submenu(ui, i, true);
                    return true;
                }
                self.owner_cycle(ui, 1);
                true
            }
            VirtualKeyCode::Left => {
                if self.is_submenu.get() {
                    self.close_self(ui);
                } else {
                    self.owner_cycle(ui, -1);
                }
                true
            }
            VirtualKeyCode::Escape => {
                self.close_self(ui);
                true
            }
            // Menus capture the keyboard while open.
            _ => true,
        }
    }

    fn on_key_char(&self, _ui: &mut UI, _unicode_codepoint: char, _state: ModifiersState) -> bool {
        // Swallow typed characters so the underlying UI doesn't receive them
        // while a menu is open.
        true
    }
}

impl Default for PopupMenu {
    fn default() -> Self {
        PopupMenu::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::Typeface;

    fn flat_menu() -> PopupMenu {
        let mut m = PopupMenu::new();
        m.add_item("a", "", "A");
        m.add_separator();
        m.add_item("b", "", "B");
        m
    }

    #[test]
    fn selection_skips_separators_and_wraps() {
        let m = flat_menu();
        assert_eq!(m.get_hovered_index(), None);
        m.move_selection(1);
        assert_eq!(m.get_hovered_index(), Some(0));
        m.move_selection(1);
        assert_eq!(m.get_hovered_index(), Some(2)); // skips the separator
        m.move_selection(1);
        assert_eq!(m.get_hovered_index(), Some(0)); // wraps around
        m.move_selection(-1);
        assert_eq!(m.get_hovered_index(), Some(2));
    }

    fn menu_with_submenu(ui: &mut UI) -> Element {
        let mut menu = PopupMenu::new();
        menu.set_id("ctx");
        menu.add_item("one", "", "One");
        menu.add_submenu("more", "", "More", vec![MenuItem {
            id: "deep".to_owned(),
            icon_path: String::new(),
            text: "Deep".to_owned(),
            separator: false,
            children: Vec::new(),
        }]);
        let el: Element = Rc::new(RefCell::new(menu));
        ui.show_popup(Rc::clone(&el), 0, 0, PopupDirection::BottomRight, PopupMode::Popup);
        el
    }

    fn as_menu(el: &Element) -> std::cell::Ref<'_, dyn View> {
        el.borrow()
    }

    #[test]
    fn right_opens_submenu_and_enter_routes_to_root() {
        let mut ui = UI::new(800, 600, Typeface::default(), 1.0);
        let el = menu_with_submenu(&mut ui);
        {
            let m = as_menu(&el);
            let m = m.as_any().downcast_ref::<PopupMenu>().unwrap();
            m.move_selection(1);
            m.move_selection(1); // on "More"
        }
        let handled = el.borrow().on_key_down(&mut ui, Some(VirtualKeyCode::Right), 0, ModifiersState::default());
        assert!(handled);
        assert!(ui.is_popup_open("ctx_sub1"));

        let sub = {
            let m = as_menu(&el);
            let m = m.as_any().downcast_ref::<PopupMenu>().unwrap();
            m.submenu_open.borrow().as_ref().map(|(_, e)| Rc::clone(e)).unwrap()
        };
        // Keyboard opening pre-selected the first submenu item; Enter fires it.
        sub.borrow().on_key_down(&mut ui, Some(VirtualKeyCode::Return), 0, ModifiersState::default());
        assert!(!ui.has_popups());
        let m = as_menu(&el);
        let m = m.as_any().downcast_ref::<PopupMenu>().unwrap();
        assert_eq!(m.clicked_item().as_deref(), Some("deep"));
    }

    #[test]
    fn escape_closes_one_level() {
        let mut ui = UI::new(800, 600, Typeface::default(), 1.0);
        let el = menu_with_submenu(&mut ui);
        {
            let m = as_menu(&el);
            let m = m.as_any().downcast_ref::<PopupMenu>().unwrap();
            m.move_selection(1);
            m.move_selection(1);
        }
        el.borrow().on_key_down(&mut ui, Some(VirtualKeyCode::Right), 0, ModifiersState::default());
        assert!(ui.is_popup_open("ctx_sub1"));

        let sub = {
            let m = as_menu(&el);
            let m = m.as_any().downcast_ref::<PopupMenu>().unwrap();
            m.submenu_open.borrow().as_ref().map(|(_, e)| Rc::clone(e)).unwrap()
        };
        sub.borrow().on_key_down(&mut ui, Some(VirtualKeyCode::Escape), 0, ModifiersState::default());
        assert!(!ui.is_popup_open("ctx_sub1"));
        assert!(ui.is_popup_open("ctx"));
        // The root self-heals its submenu state on the next key event.
        el.borrow().on_key_down(&mut ui, Some(VirtualKeyCode::Escape), 0, ModifiersState::default());
        assert!(!ui.is_popup_open("ctx"));
    }
}
