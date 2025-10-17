use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;

use speedy2d::dimen::Vector2;
use speedy2d::font::{TextAlignment, TextLayout, TextOptions};
use speedy2d::window::MouseButton;

use crate::gui::assets::get_font;
use crate::gui::events::EventType;
use crate::gui::common::DEFAULT_TEXT_SIZE;
use crate::gui::themes::{FontStyle, Theme, Typeface, ViewState};
use crate::gui::traits::{Element, View, WeakElement};
use crate::gui::types::{Point, Rect, rect};
use crate::gui::ui::UI;
use crate::gui::views::{Borders, Dimension};
use crate::gui::styles::selector::FontSelector;
use crate::gui::views::{FieldsMain, FieldsTexted};
use super::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH};

pub struct Button {
    state: RefCell<FieldsTexted>
}

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

    fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        match &self.state.borrow().main.typeface {
            None => parent_typeface.clone(),
            Some(t) => {
                if t.font_name.is_empty() {
                    let mut parent = parent_typeface.clone();
                    parent.font_style = t.font_style.clone();
                    parent
                } else {
                    t.clone()
                }
            }
        }
    }

    fn set_font(&self, font_name: &str) {
        let typeface = match self.state.borrow_mut().main.typeface.take() {
            None => Typeface { font_name: font_name.to_owned(), font_style: FontStyle::Regular },
            Some(mut t) => {
                t.font_name = font_name.to_owned();
                t
            }
        };
        self.state.borrow_mut().main.typeface = Some(typeface);
    }

    fn set_font_style(&self, style: &str) {
        let font_style = FontStyle::from(style);
        let typeface = match self.state.borrow_mut().main.typeface.take() {
            None => Typeface { font_name: String::new(), font_style },
            Some(t) => Typeface { font_name: t.font_name, font_style },
        };
        self.state.borrow_mut().main.typeface = Some(typeface)
    }

    fn layout_text(&self, max_width: i32, single_line: bool, scale: f64) {
        if max_width <= 0 {
            self.state.borrow_mut().cached_text = None;
            return;
        }
        let typeface = self.state.borrow().main.typeface.clone();
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
        match name {
            "left" => { self.set_x(value.parse().unwrap()) }
            "top" => { self.set_y(value.parse().unwrap()) }
            "width" => { self.set_width(value.parse().unwrap()) }
            "height" => { self.set_height(value.parse().unwrap()) }
            "padding" => { self.state.borrow_mut().main.padding.set_all(value.parse().unwrap_or(0)) }
            "padding_top" => { self.state.borrow_mut().main.padding.top = value.parse().unwrap_or(0) }
            "padding_left" => { self.state.borrow_mut().main.padding.left = value.parse().unwrap_or(0) }
            "padding_right" => { self.state.borrow_mut().main.padding.right = value.parse().unwrap_or(0) }
            "padding_bottom" => { self.state.borrow_mut().main.padding.bottom = value.parse().unwrap_or(0) }
            "margin" => { self.state.borrow_mut().main.margin.set_all(value.parse().unwrap_or(0)) }
            "margin_left" => { self.state.borrow_mut().main.margin.left = value.parse().unwrap_or(0) }
            "margin_right" => { self.state.borrow_mut().main.margin.right = value.parse().unwrap_or(0) }
            "margin_top" => { self.state.borrow_mut().main.margin.top = value.parse().unwrap_or(0) }
            "margin_bottom" => { self.state.borrow_mut().main.margin.bottom = value.parse().unwrap_or(0) }
            "id" => { self.set_id(value) }
            "text" => { self.set_text(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "break" => { self.state.borrow_mut().main.break_line = value.parse().unwrap_or(false) }
            &_ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.state.borrow_mut().main.parent = parent;
    }

    fn get_parent(&self) -> Option<Element> {
        match &self.state.borrow().main.parent {
            None => { None }
            Some(weak) => {
                match weak.upgrade() {
                    None => { None }
                    Some(parent) => { Some(parent) }
                }
            }
        }
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        //println!("{} for width {}", self.get_id(), width);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.typeface = Some(typeface);
        self.state.borrow_mut().main.scale = scale;
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
        theme.draw_button_back(rect, state.main.state);
        theme.draw_button_body(rect, state.main.state);
        // TODO use padding
        if let Some(text) = &state.cached_text {
            let x = (self.get_rect_width() as f32 - text.width()) / 2f32;
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let color = theme.get_text_color(state.main.state, &state.main.foreground);
            theme.draw_text((rect.min.x as f32 + x).round(), (rect.min.y as f32 + y).round(), color, text);
        }
        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().main.state)
    }

    fn get_rect(&self) -> Rect<i32> {
        self.state.borrow().main.rect
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.state.borrow_mut().main.rect = rect;
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.state.borrow().main.padding.scaled(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut state = self.state.borrow_mut();
        state.main.padding.top = top;
        state.main.padding.left = left;
        state.main.padding.right = right;
        state.main.padding.bottom = bottom;
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.state.borrow().main.margin.scaled(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut state = self.state.borrow_mut();
        state.main.margin.top = top;
        state.main.margin.left = left;
        state.main.margin.right = right;
        state.main.margin.bottom = bottom;
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        let state = self.state.borrow();
        (state.main.width, state.main.height)
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
        self.state.borrow().main.state.focused
    }

    fn is_break(&self) -> bool {
        self.state.borrow().main.break_line
    }

    fn set_focused(&self, focused: bool) {
        self.state.borrow_mut().main.state.focused = focused;
    }

    fn set_focusable(&self, focusable: bool) {
        self.state.borrow_mut().main.state.focusable = focusable;
    }

    fn set_width(&mut self, width: Dimension) {
        self.state.borrow_mut().main.width = width;
    }

    fn set_height(&mut self, height: Dimension) {
        self.state.borrow_mut().main.height = height;
    }

    fn set_scale(&mut self, scale: f64) {
        self.state.borrow_mut().main.scale = scale;
    }

    fn set_id(&mut self, id: &str) {
        self.state.borrow_mut().main.id = id.to_owned();
    }

    fn get_id(&self) -> String {
        self.state.borrow().main.id.clone()
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
