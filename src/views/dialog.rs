use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::font::{FormattedTextBlock, TextAlignment, TextLayout, TextOptions};
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};

use crate::assets::{get_asset, get_font_family};
use crate::common::DEFAULT_TEXT_SIZE;
use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::views::button::Button;
use crate::view_base::{HasMainFields, ViewBasics};

const DIALOG_MIN_WIDTH: i32 = 400;
const DIALOG_ICON_SIZE: i32 = 64;
const ICON_TEXT_GAP: i32 = 12;
const CONTENT_BUTTON_GAP: i32 = 16;
const BUTTON_GAP: i32 = 6;
const DIALOG_PADDING: i32 = 8;
const TEXT_COLOR: u32 = 0xff000000;

/// Which side of the button bar a button belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ButtonSide {
    #[default]
    Right,
    Left,
}

/// Describes a button to add to the dialog.
pub struct DialogButton {
    pub id: String,
    pub text: String,
    pub side: ButtonSide,
    pub is_default: bool,
}

pub struct Dialog {
    state: RefCell<FieldsMain>,
    // Content area
    icon_path: RefCell<String>,
    icon_bytes: RefCell<Option<Vec<u8>>>,
    message: RefCell<String>,
    cached_message: RefCell<Option<FormattedTextBlock>>,
    custom_content: RefCell<Option<Element>>,
    // Button bar
    buttons: Vec<DialogButton>,
    button_views: Vec<Element>,
    pressed_button: RefCell<Option<String>>,
    listeners: RefCell<HashMap<EventType, Box<dyn FnMut(&mut UI, &dyn View) -> bool>>>,
}

impl HasMainFields for Dialog {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for Dialog {}

#[allow(dead_code)]
impl Dialog {
    pub fn new() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (DIALOG_MIN_WIDTH, 100)), Dimension::Min, Dimension::Min);
        main.padding = Borders::with_padding(DIALOG_PADDING);
        main.state.focusable = false;
        Dialog {
            state: RefCell::new(main),
            icon_path: RefCell::new(String::new()),
            icon_bytes: RefCell::new(None),
            message: RefCell::new(String::new()),
            cached_message: RefCell::new(None),
            custom_content: RefCell::new(None),
            buttons: Vec::new(),
            button_views: Vec::new(),
            pressed_button: RefCell::new(None),
            listeners: RefCell::new(HashMap::new()),
        }
    }

    pub fn set_icon(&mut self, icon_path: &str) {
        *self.icon_path.borrow_mut() = icon_path.to_owned();
        *self.icon_bytes.borrow_mut() = None;
    }

    pub fn set_message(&mut self, message: &str) {
        *self.message.borrow_mut() = message.to_owned();
        *self.cached_message.borrow_mut() = None;
    }

    pub fn set_custom_content(&mut self, content: Element) {
        *self.custom_content.borrow_mut() = Some(content);
    }

    pub fn add_button(&mut self, id: &str, text: &str, side: ButtonSide, is_default: bool) {
        self.buttons.push(DialogButton {
            id: id.to_owned(),
            text: text.to_owned(),
            side,
            is_default,
        });
        let btn = Button::new(rect((0, 0), (60, 24)), text, DEFAULT_TEXT_SIZE);
        let element: Element = Rc::new(RefCell::new(btn));
        self.button_views.push(element);
    }

    /// Returns the id of the button that was clicked.
    pub fn get_pressed_button(&self) -> Option<String> {
        self.pressed_button.borrow().clone()
    }

    fn load_icon(&self) {
        if self.icon_bytes.borrow().is_none() {
            let path = self.icon_path.borrow().clone();
            if !path.is_empty() {
                if let Some(bytes) = get_asset(&path) {
                    *self.icon_bytes.borrow_mut() = Some(bytes);
                }
            }
        }
    }

    fn layout_message(&self, max_width: i32, typeface: &Typeface, scale: f64) {
        let msg = self.message.borrow();
        if msg.is_empty() {
            *self.cached_message.borrow_mut() = None;
            return;
        }
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            let base_size = typeface.font_size.unwrap_or(DEFAULT_TEXT_SIZE);
            let size = base_size * scale as f32;
            let options = TextOptions::new().with_wrap_to_width(max_width as f32, TextAlignment::Left);
            let block = font.layout_text(&msg, size, options);
            *self.cached_message.borrow_mut() = Some(block);
        }
    }

    fn has_icon(&self) -> bool {
        self.icon_bytes.borrow().is_some()
    }

    /// Closes this dialog popup. Call from your Click handler.
    pub fn close(&self, ui: &mut UI) {
        let id = self.get_id();
        ui.close_popup(&id);
    }

    /// Find the index of the focused button, if any.
    fn focused_button_index(&self) -> Option<usize> {
        for (i, bv) in self.button_views.iter().enumerate() {
            if bv.borrow().is_focused() {
                return Some(i);
            }
        }
        None
    }

    /// Focus the next button (wraps around).
    fn focus_next_button(&self) {
        let count = self.button_views.len();
        if count == 0 {
            return;
        }
        let current = self.focused_button_index();
        let next = match current {
            Some(i) => (i + 1) % count,
            None => 0,
        };
        for (i, bv) in self.button_views.iter().enumerate() {
            bv.borrow().set_focused(i == next);
        }
    }

    /// Focus the previous button (wraps around).
    fn focus_prev_button(&self) {
        let count = self.button_views.len();
        if count == 0 {
            return;
        }
        let current = self.focused_button_index();
        let prev = match current {
            Some(0) => count - 1,
            Some(i) => i - 1,
            None => 0,
        };
        for (i, bv) in self.button_views.iter().enumerate() {
            bv.borrow().set_focused(i == prev);
        }
    }

    /// Trigger click on the focused button.
    fn click_focused_button(&self, ui: &mut UI) -> bool {
        if let Some(idx) = self.focused_button_index() {
            *self.pressed_button.borrow_mut() = Some(self.buttons[idx].id.clone());
            self.click(ui);
            return true;
        }
        false
    }
}

impl View for Dialog {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "icon" => self.set_icon(value),
            "message" => self.set_message(value),
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
        let typeface = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(typeface.clone()));
        self.base_set_scale(scale);
        self.load_icon();

        let padding = self.get_padding(scale);
        let icon_size = (DIALOG_ICON_SIZE as f64 * scale).round() as i32;
        let icon_gap = (ICON_TEXT_GAP as f64 * scale).round() as i32;
        let content_btn_gap = (CONTENT_BUTTON_GAP as f64 * scale).round() as i32;
        let btn_gap = (BUTTON_GAP as f64 * scale).round() as i32;
        let min_w = (DIALOG_MIN_WIDTH as f64 * scale).round() as i32;

        let max_inner_w = width - padding.left - padding.right;

        // --- Layout buttons first to know their total width ---
        let mut btn_bar_h = 0i32;
        let mut total_btn_w = 0i32;
        for bv in &self.button_views {
            bv.borrow_mut().layout_content(0, 0, max_inner_w, height, &typeface, scale);
            let br = bv.borrow().get_rect();
            btn_bar_h = btn_bar_h.max(br.height());
            total_btn_w += br.width();
        }
        if !self.button_views.is_empty() {
            total_btn_w += btn_gap * (self.button_views.len() as i32 - 1);
        }

        // --- Content area: use min_w as text wrap width, then shrink to actual content ---
        let content_h;
        let content_w;
        let has_custom = self.custom_content.borrow().is_some();

        if has_custom {
            let custom = self.custom_content.borrow().clone();
            if let Some(ref el) = custom {
                el.borrow_mut().layout_content(0, 0, max_inner_w, height, &typeface, scale);
                let cr = el.borrow().get_rect();
                content_h = cr.height();
                content_w = cr.width();
            } else {
                content_h = 0;
                content_w = 0;
            }
        } else {
            // Layout message text with min_w wrap width
            let text_wrap_w = if self.has_icon() {
                min_w - icon_size - icon_gap
            } else {
                min_w
            };
            self.layout_message(text_wrap_w, &typeface, scale);

            let msg_h = self.cached_message.borrow().as_ref()
                .map(|t| t.height().ceil() as i32)
                .unwrap_or(0);
            let msg_w = self.cached_message.borrow().as_ref()
                .map(|t| t.width().ceil() as i32)
                .unwrap_or(0);

            content_h = if self.has_icon() {
                msg_h.max(icon_size)
            } else {
                msg_h
            };
            content_w = if self.has_icon() {
                icon_size + icon_gap + msg_w
            } else {
                msg_w
            };
        }

        // --- Compute total dialog size from content ---
        let inner_w = content_w.max(total_btn_w).max(min_w).min(max_inner_w);
        let total_w = padding.left + inner_w + padding.right;
        let actual_inner_w = inner_w;

        let total_h_gap = if !self.button_views.is_empty() && content_h > 0 { content_btn_gap } else { 0 };
        let total_h = (padding.top + content_h + total_h_gap + btn_bar_h + padding.bottom).min(height);

        // --- Position buttons ---
        let btn_y = padding.top + content_h + total_h_gap;

        // Left-side buttons: start from left
        let mut left_x = padding.left;
        for (i, db) in self.buttons.iter().enumerate() {
            if db.side == ButtonSide::Left {
                let bv = &self.button_views[i];
                let bw = bv.borrow().get_rect().width();
                let bh = bv.borrow().get_rect().height();
                bv.borrow_mut().set_rect(rect((left_x, btn_y), (left_x + bw, btn_y + bh)));
                left_x += bw + btn_gap;
            }
        }

        // Right-side buttons: start from right
        let mut right_x = padding.left + actual_inner_w;
        for (i, db) in self.buttons.iter().enumerate().rev() {
            if db.side == ButtonSide::Right {
                let bv = &self.button_views[i];
                let bw = bv.borrow().get_rect().width();
                let bh = bv.borrow().get_rect().height();
                bv.borrow_mut().set_rect(rect((right_x - bw, btn_y), (right_x, btn_y + bh)));
                right_x -= bw + btn_gap;
            }
        }

        // --- Set default button focus ---
        for (i, db) in self.buttons.iter().enumerate() {
            self.button_views[i].borrow().set_focused(db.is_default);
        }

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
        let start = Point { x: r.min.x, y: r.min.y };

        theme.push_clip();
        theme.clip_rect(r);

        // Background frame
        theme.draw_component("button_classic_back", r, state.state);

        let padding = state.padding.scaled(scale);
        let icon_size = (DIALOG_ICON_SIZE as f64 * scale).round() as i32;
        let icon_gap = (ICON_TEXT_GAP as f64 * scale).round() as i32;

        let has_custom = self.custom_content.borrow().is_some();

        if has_custom {
            // Paint custom content
            let custom = self.custom_content.borrow().clone();
            if let Some(ref el) = custom {
                let content_origin = Point {
                    x: start.x + padding.left,
                    y: start.y + padding.top,
                };
                el.borrow().paint(content_origin, theme);
            }
        } else {
            // Paint icon
            let mut text_x = r.min.x + padding.left;
            if self.has_icon() {
                if let Some(ref bytes) = *self.icon_bytes.borrow() {
                    let icon_rect = rect(
                        (r.min.x + padding.left, r.min.y + padding.top),
                        (r.min.x + padding.left + icon_size, r.min.y + padding.top + icon_size),
                    );
                    theme.draw_image(icon_rect, bytes);
                }
                text_x += icon_size + icon_gap;
            }

            // Paint message text
            if let Some(ref text) = *self.cached_message.borrow() {
                let text_y = r.min.y + padding.top;
                theme.draw_text(text_x as f32, text_y as f32, TEXT_COLOR, text);
            }
        }

        // Paint buttons
        for bv in &self.button_views {
            bv.borrow().paint(start, theme);
        }

        // Border frame
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

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let r = self.get_rect();
        let padding = self.state.borrow().padding;
        (r.width() - padding.left - padding.right, r.height() - padding.top - padding.bottom)
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

    fn on_event(&mut self, event: EventType, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        self.listeners.borrow_mut().insert(event, func);
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        let listener = self.listeners.borrow_mut().remove(&EventType::Click);
        if let Some(mut click) = listener {
            let result = click(ui, self as &dyn View);
            self.listeners.borrow_mut().insert(EventType::Click, click);
            return result;
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        let r = self.state.borrow().rect;
        let local = Vector2::new(position.x - r.min.x, position.y - r.min.y);
        let mut redraw = false;
        for bv in &self.button_views {
            redraw |= bv.borrow().on_mouse_move(_ui, local);
        }
        redraw
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) {
            return false;
        }
        let local = Vector2::new(position.x - r.min.x, position.y - r.min.y);
        for (i, bv) in self.button_views.iter().enumerate() {
            if bv.borrow().on_mouse_button_down(ui, local, button) {
                // Defocus all other buttons
                for (j, other) in self.button_views.iter().enumerate() {
                    if j != i {
                        other.borrow().set_focused(false);
                    }
                }
                return true;
            }
        }
        // Also handle custom content mouse events
        if let Some(ref el) = *self.custom_content.borrow() {
            let content_local = Vector2::new(
                local.x - self.get_padding(self.state.borrow().scale).left,
                local.y - self.get_padding(self.state.borrow().scale).top,
            );
            if el.borrow().on_mouse_button_down(ui, content_local, button) {
                return true;
            }
        }
        true // Consume click inside dialog
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let r = self.state.borrow().rect;
        let local = Vector2::new(position.x - r.min.x, position.y - r.min.y);
        for (i, bv) in self.button_views.iter().enumerate() {
            if bv.borrow().on_mouse_button_up(ui, local, button) {
                // Button was clicked — store which one and fire dialog callback
                *self.pressed_button.borrow_mut() = Some(self.buttons[i].id.clone());
                self.click(ui);
                return true;
            }
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        match virtual_key_code {
            Some(VirtualKeyCode::Tab) | Some(VirtualKeyCode::Right) | Some(VirtualKeyCode::Down) => {
                self.focus_next_button();
                true
            }
            Some(VirtualKeyCode::Left) | Some(VirtualKeyCode::Up) => {
                self.focus_prev_button();
                true
            }
            Some(VirtualKeyCode::Return) | Some(VirtualKeyCode::NumpadEnter) => {
                self.click_focused_button(ui)
            }
            _ => false,
        }
    }

    fn on_key_char(&self, ui: &mut UI, unicode_codepoint: char, _state: ModifiersState) -> bool {
        if unicode_codepoint == ' ' {
            return self.click_focused_button(ui);
        }
        false
    }
}

impl Default for Dialog {
    fn default() -> Self {
        Dialog::new()
    }
}
