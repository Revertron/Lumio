use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;

use speedy2d::dimen::Vector2;
use speedy2d::font::{TextAlignment, TextLayout, TextOptions};
use speedy2d::window::MouseButton;

use crate::assets::get_font;
use crate::events::EventType;
use crate::common::DEFAULT_TEXT_SIZE;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension};
use crate::styles::selector::FontSelector;
use crate::views::{FieldsMain, FieldsTexted};
use crate::view_base::{HasMainFields, ViewBasics};
use super::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH};

pub struct Button {
    state: RefCell<FieldsTexted>
}

impl HasMainFields for Button {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        // SAFETY: We can safely transmute RefCell<FieldsTexted> to &RefCell<FieldsMain>
        // because FieldsTexted starts with FieldsMain as its first field
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for Button {}

#[allow(dead_code)]
impl Button {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> Button {
        let mut main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        main.padding = Borders::with_padding(4);
        Button {
            state: RefCell::new(FieldsTexted {
                main,
                text: text.to_owned(),
                text_size,
                line_height: 0f32,
                single_line: true,
                cached_text: None,
                font: FontSelector::new(),
                listeners: HashMap::new()
            })
        }
    }

    pub fn set_text(&self, text: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.text.clear();
            state.text.push_str(text);
            state.cached_text = None;
        }
        let scale = self.state.borrow().main.scale;
        let single_line = self.state.borrow().single_line;
        self.layout_text(self.get_rect_width(), single_line, scale);
    }

    pub fn set_single_line(&self, single_line: bool) {
        let mut state = self.state.borrow_mut();
        state.single_line = single_line;
        state.cached_text = None;
    }

    fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        self.state.borrow().main.font_manager.get_typeface(parent_typeface)
    }

    fn set_font(&self, font_name: &str) {
        self.state.borrow_mut().main.font_manager.set_font(font_name);
    }

    fn set_font_style(&self, style: &str) {
        self.state.borrow_mut().main.font_manager.set_font_style(style);
    }

    fn layout_text(&self, max_width: i32, single_line: bool, scale: f64) {
        if max_width <= 0 {
            self.state.borrow_mut().cached_text = None;
            return;
        }
        let typeface = self.state.borrow().main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font(&typeface.font_name, &typeface.font_style.to_string()) {
                let options = match single_line {
                    true => TextOptions::new(),
                    false => TextOptions::new().with_wrap_to_width(max_width as f32, TextAlignment::Left)
                };
                let size = self.state.borrow().text_size * scale as f32;
                let text = font.layout_text(&self.state.borrow().text, size, options);
                self.state.borrow_mut().cached_text = Some(text);
            }
        }
    }
}

impl View for Button {
    fn set_any(&mut self, name: &str, value: &str) {
        // Try to handle common properties first
        if self.base_set_any(name, value) {
            return;
        }

        // Handle Button-specific properties
        match name {
            "text" => { self.set_text(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "single_line" => { self.state.borrow_mut().single_line = value.parse().unwrap_or(true) }
            &_ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        //println!("{} for width {}", self.get_id(), width);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface));
        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let (new_width, _new_height) = self.calculate_size(width.max(BUTTON_MIN_WIDTH) - horizontal, height.max(BUTTON_MIN_HEIGHT) - vertical, scale);
        let single_line = self.state.borrow().single_line;
        self.layout_text(new_width, single_line, scale);
        let (width, height) = self.calculate_full_size(scale);
        let rect = rect((x, y), (x + width, y + height));
        self.set_rect(rect);
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let state = self.state.borrow();
        match &state.cached_text {
            Some(text) => text.width() <= width as f32 && text.height() <= height as f32,
            None => width <= BUTTON_MIN_WIDTH && height <= BUTTON_MIN_HEIGHT
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);

        // Step 1: Draw background (before text)
        theme.draw_component("button_classic_back", rect, state.main.state);

        // Step 2: Draw text
        if let Some(text) = &state.cached_text {
            let x = (self.get_rect_width() as f32 - text.width()) / 2f32;
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
            theme.draw_text((rect.min.x as f32 + x).round(), (rect.min.y as f32 + y).round(), color, text);
        }

        // Step 3: Draw borders/body (after text, covers edges)
        theme.draw_component("button_classic_body", rect, state.main.state);

        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().main.state)
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
        match &state.cached_text {
            None => (BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT),
            Some(text) => {
                let width = max(text.width().ceil() as i32, BUTTON_MIN_WIDTH);
                let height = max(text.height().ceil() as i32, BUTTON_MIN_HEIGHT);
                (width, height)
            }
        }
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
        self.state.borrow_mut().listeners.insert(event, func);
    }

    fn click(&self, ui: &mut UI) -> bool {
        let listener = self.state.borrow_mut().listeners.remove(&EventType::Click);
        if let Some(mut click) = listener {
            let result = click(ui, self as &dyn View);
            self.state.borrow_mut().listeners.insert(EventType::Click, click);
            return result;
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        let old_state = self.state.borrow_mut().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow_mut().main.state != old_state
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        if hit {
            let mut state = self.state.borrow_mut();
            if matches!(button, MouseButton::Left) {
                state.main.state.pressed = true;
            }
            state.main.state.focused = true;
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        if matches!(button, MouseButton::Left) {
            if self.state.borrow().main.state.pressed {
                if hit {
                    println!("Doing click!");
                    self.click(ui);
                } else {
                    println!("Cancelled click");
                }
                let mut state = self.state.borrow_mut();
                state.main.state.pressed = false;
                return true;
            }
        }
        false
    }
}

impl Default for Button {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        Button::new(rect, "", DEFAULT_TEXT_SIZE)
    }
}
