use std::cell::RefCell;
use std::collections::HashMap;

use speedy2d::dimen::Vector2;
use speedy2d::font::{TextLayout, TextOptions};
use speedy2d::window::MouseButton;

use crate::assets::{get_asset, get_font};
use crate::common::DEFAULT_TEXT_SIZE;
use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain};
use crate::view_base::{HasMainFields, ViewBasics};

const ICON_SIZE: i32 = 16;
const ITEM_HEIGHT: i32 = 28;
const SEPARATOR_HEIGHT: i32 = 3;
const ICON_TEXT_GAP: i32 = 6;
const ITEM_PADDING_LEFT: i32 = 6;
const ITEM_PADDING_RIGHT: i32 = 12;
const HIGHLIGHT_COLOR: u32 = 0xff0000c0;
const HIGHLIGHT_TEXT_COLOR: u32 = 0xffffffff;
const NORMAL_TEXT_COLOR: u32 = 0xff000000;

/// Data for a single menu item.
pub struct MenuItem {
    pub id: String,
    pub icon_path: String,
    pub text: String,
    pub separator: bool,
}

pub struct PopupMenu {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<MenuItem>>,
    icon_bytes: RefCell<Vec<Option<Vec<u8>>>>,
    cached_texts: RefCell<Vec<Option<speedy2d::font::FormattedTextBlock>>>,
    hovered: RefCell<Option<usize>>,
    pressed: RefCell<Option<usize>>,
    listeners: RefCell<HashMap<EventType, Box<dyn FnMut(&mut UI, &dyn View) -> bool>>>,
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
            icon_bytes: RefCell::new(Vec::new()),
            cached_texts: RefCell::new(Vec::new()),
            hovered: RefCell::new(None),
            pressed: RefCell::new(None),
            listeners: RefCell::new(HashMap::new()),
        }
    }

    /// Adds a menu item with icon and text.
    pub fn add_item(&mut self, id: &str, icon_path: &str, text: &str) {
        self.items.borrow_mut().push(MenuItem {
            id: id.to_owned(),
            icon_path: icon_path.to_owned(),
            text: text.to_owned(),
            separator: false,
        });
        self.icon_bytes.borrow_mut().push(None);
        self.cached_texts.borrow_mut().push(None);
    }

    /// Adds a horizontal separator line between menu items.
    pub fn add_separator(&mut self) {
        self.items.borrow_mut().push(MenuItem {
            id: String::new(),
            icon_path: String::new(),
            text: String::new(),
            separator: true,
        });
        self.icon_bytes.borrow_mut().push(None);
        self.cached_texts.borrow_mut().push(None);
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
        let items = self.items.borrow();
        let mut bytes = self.icon_bytes.borrow_mut();
        for (i, item) in items.iter().enumerate() {
            if bytes[i].is_none() && !item.icon_path.is_empty() {
                if let Some(data) = get_asset(&item.icon_path) {
                    bytes[i] = Some(data);
                }
            }
        }
    }

    fn layout_texts(&self, typeface: &Typeface, scale: f64) {
        let items = self.items.borrow();
        let mut cached = self.cached_texts.borrow_mut();
        let text_size = DEFAULT_TEXT_SIZE * scale as f32;
        if let Some(font) = get_font(&typeface.font_name, &typeface.font_style.to_string()) {
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

        let content_w = pad_left + icon_size + gap + max_text_w + pad_right;
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
        theme.draw_component("button_classic_back", r, state.state);

        let padding = state.padding.scaled(scale);
        let icon_size = (ICON_SIZE as f64 * scale).round() as i32;
        let gap = (ICON_TEXT_GAP as f64 * scale).round() as i32;
        let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as i32;
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;

        let hovered = *self.hovered.borrow();
        let items = self.items.borrow();
        let cached = self.cached_texts.borrow();
        let icon_bytes = self.icon_bytes.borrow();

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
                theme.draw_separator(sep_rect, state.state);
                y += sep_h;
                continue;
            }

            let item_rect = rect(
                (content_x, y),
                (r.max.x - padding.right - 1, y + item_h),
            );

            // Highlight hovered item
            let text_color = if hovered == Some(i) {
                theme.draw_rect(item_rect, HIGHLIGHT_COLOR);
                HIGHLIGHT_TEXT_COLOR
            } else {
                NORMAL_TEXT_COLOR
            };

            // Draw icon
            if let Some(Some(ref bytes)) = icon_bytes.get(i) {
                let icon_y = y + (item_h - icon_size) / 2;
                let icon_rect = rect(
                    (content_x + pad_left, icon_y),
                    (content_x + pad_left + icon_size, icon_y + icon_size),
                );
                theme.draw_image(icon_rect, bytes);
            }

            // Draw text
            if let Some(Some(ref text)) = cached.get(i) {
                let text_x = content_x + pad_left + icon_size + gap;
                let text_y = y + (item_h as f32 - text.height()) as i32 / 2;
                theme.draw_text(text_x as f32, text_y as f32, text_color, text);
            }

            y += item_h;
        }

        // Draw border frame (same as Button)
        theme.draw_component("button_classic_body", r, state.state);

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

        let w = pad_left + icon_size + gap + max_text_w + pad_right;
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

    fn on_event(&mut self, event: EventType, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        self.listeners.borrow_mut().insert(event, func);
    }

    fn click(&self, ui: &mut UI) -> bool {
        let listener = self.listeners.borrow_mut().remove(&EventType::Click);
        if let Some(mut click) = listener {
            let result = click(ui, self as &dyn View);
            self.listeners.borrow_mut().insert(EventType::Click, click);
            return result;
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        let hit_item = self.get_hit_item(position.x, position.y);
        let old = *self.hovered.borrow();
        *self.hovered.borrow_mut() = hit_item;
        // Redraw if hover changed
        old != hit_item
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let hit = self.get_hit_item(position.x, position.y);
        *self.pressed.borrow_mut() = hit;
        hit.is_some()
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let pressed = self.pressed.borrow_mut().take();
        let hit = self.get_hit_item(position.x, position.y);
        if let (Some(p), Some(h)) = (pressed, hit) {
            if p == h {
                // Fire click callback, then close this popup
                self.click(ui);
                let id = self.get_id();
                ui.close_popup(&id);
                return true;
            }
        }
        false
    }
}

impl Default for PopupMenu {
    fn default() -> Self {
        PopupMenu::new()
    }
}
