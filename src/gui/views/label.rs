use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;

use speedy2d::font::{TextAlignment, TextLayout, TextOptions};
use crate::gui::assets::get_font;
use crate::gui::events::EventType;

use crate::gui::themes::{FontStyle, Theme, Typeface, ViewState};
use crate::gui::traits::{Element, View, WeakElement};
use crate::gui::types::{Point, Rect, rect};
use crate::gui::ui::UI;
use crate::gui::views::{Borders, Dimension};
use crate::gui::styles::selector::FontSelector;
use crate::gui::views::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH, FieldsMain, FieldsTexted};

pub struct Label {
    state: RefCell<FieldsTexted>
}

#[allow(dead_code)]
impl Label {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> Label {
        let mut main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        main.state.focusable = false;
        Label {
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

    pub fn set_text(&mut self, text: &str) {
        let mut state = self.state.borrow_mut();
        state.text.clear();
        state.text.push_str(text);
        let _ = state.cached_text.take();
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
}

impl View for Label {
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

    fn layout_content(&mut self, x: i32, y: i32, width: i32, _height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        if self.state.borrow().cached_text.is_some() {
            // TODO check if area changed
            return self.get_rect();
        }

        self.state.borrow_mut().main.scale = scale;
        let typeface = self.get_typeface(typeface);
        if let Some(font) = get_font(&typeface.font_name, &typeface.font_style.to_string()) {
            let options = TextOptions::new()
                .with_wrap_to_width(width as f32, TextAlignment::Left);
            let text = font.layout_text(&self.state.borrow().text, self.state.borrow().text_size, options);
            self.state.borrow_mut().cached_text = Some(text);
        }
        let (width, height) = self.calculate_full_size(scale);
        let rect = rect((x, y), (x + width, y + height));
        self.set_rect(rect.clone());
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
        if let Some(text) = &self.state.borrow().cached_text {
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
                let width = max(text.width().round() as i32, BUTTON_MIN_WIDTH);
                let height = max(text.height().round() as i32, BUTTON_MIN_HEIGHT);
                (width, height)
            }
        }
    }

    fn is_break(&self) -> bool {
        self.state.borrow().main.break_line
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

    fn click(&self, _ui: &mut UI) -> bool {
        todo!()
    }
}

impl Default for Label {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        Label::new(rect, "", 48_f32)
    }
}
