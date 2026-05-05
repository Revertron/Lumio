use std::cell::RefCell;
use std::collections::HashMap;

use speedy2d::font::{TextAlignment, TextLayout, TextOptions};
use crate::assets::get_font_family;
use crate::events::EventType;

use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Gravity, Visibility};
use crate::styles::selector::FontSelector;
use crate::views::{FieldsMain, FieldsTexted};
use crate::view_base::{HasMainFields, ViewBasics};

pub struct Label {
    state: RefCell<FieldsTexted>,
}

impl HasMainFields for Label {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for Label {}

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
                single_line: false,
                cached_text: None,
                font: FontSelector::new(),
                listeners: HashMap::new()
            })
        }
    }

    fn rebuild_text(&self) {
        let state = self.state.borrow();
        let typeface = state.main.font_manager.get_typeface(&Typeface::default());
        let font = match get_font_family(&typeface.font_name, typeface.font_style) {
            Some(f) => f,
            None => return,
        };
        let scale = state.main.scale;
        let padding = &state.main.padding;
        let pad_h = (padding.left as f64 * scale).round() as i32 + (padding.right as f64 * scale).round() as i32;
        // Use parent width for wrapping when label is width=Min
        let available_width = if matches!(state.main.width, Dimension::Min) {
            if let Some(parent) = state.main.parent.as_ref().and_then(|w| w.upgrade()) {
                let parent_rect = parent.borrow().get_rect();
                let label_x = state.main.rect.min.x;
                (parent_rect.max.x - label_x - pad_h).max(0)
            } else {
                (state.main.rect.width() - pad_h).max(0)
            }
        } else {
            (state.main.rect.width() - pad_h).max(0)
        };
        let options = match state.single_line {
            true => TextOptions::new(),
            false => TextOptions::new().with_wrap_to_width(available_width as f32, TextAlignment::Left),
        };
        let base_size = typeface.font_size
            .map(|dip| dip * scale as f32)
            .unwrap_or(state.text_size);
        let text = font.layout_text(&state.text, base_size, options);
        // Update rect to fit new text
        let new_width = text.width().ceil() as i32 + pad_h;
        let pad_v = (padding.top as f64 * scale).round() as i32 + (padding.bottom as f64 * scale).round() as i32;
        let new_height = text.height().ceil() as i32 + pad_v;
        drop(state);
        let mut state = self.state.borrow_mut();
        if matches!(state.main.width, Dimension::Min) {
            state.main.rect.max.x = state.main.rect.min.x + new_width;
        }
        if matches!(state.main.height, Dimension::Min) {
            state.main.rect.max.y = state.main.rect.min.y + new_height;
        }
        state.cached_text = Some(text);
    }

    pub fn set_text(&mut self, text: &str) {
        let mut state = self.state.borrow_mut();
        state.text.clear();
        state.text.push_str(text);
        let _ = state.cached_text.take();
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

    fn set_font_size(&self, size: f32) {
        let mut state = self.state.borrow_mut();
        state.main.font_manager.set_font_size(size);
        state.cached_text = None;
    }
}

impl View for Label {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "text" => { self.set_text(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
            }
            "single_line" => { self.state.borrow_mut().single_line = value.parse().unwrap_or(false) }
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
        if self.state.borrow().cached_text.is_some() {
            // TODO check if area changed
            return self.get_rect();
        }

        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let (new_width, new_height) = self.calculate_size(width - horizontal, height - vertical, scale);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface.clone()));
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            let single_line = self.state.borrow().single_line;
            let options = match single_line {
                true => TextOptions::new(),
                false => TextOptions::new().with_wrap_to_width(new_width as f32, TextAlignment::Left),
            };
            let base_size = typeface.font_size
                .map(|dip| dip * scale as f32)
                .unwrap_or(self.state.borrow().text_size);
            let text = font.layout_text(&self.state.borrow().text, base_size, options);
            self.state.borrow_mut().cached_text = Some(text);
        }
        let (content_width, content_height) = self.calculate_full_size(scale);
        let (b_width, b_height) = self.get_bounds();
        let final_width = match b_width {
            Dimension::Min => content_width,
            _ => new_width + horizontal,
        };
        let final_height = match b_height {
            Dimension::Min => content_height,
            _ => new_height + vertical,
        };
        let rect = rect((x, y), (x + final_width, y + final_height));
        self.set_rect(rect.clone());
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let state = self.state.borrow();
        match &state.cached_text {
            Some(text) => text.width() <= width as f32 && text.height() <= height as f32,
            None => true
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        // Rebuild cached text if it was invalidated (e.g. by set_text)
        if self.state.borrow().cached_text.is_none() {
            self.rebuild_text();
        }
        let state = self.state.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        if let Some(text) = &state.cached_text {
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
            theme.draw_text(rect.min.x as f32, (rect.min.y as f32 + y).round(), color, text);
        }
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
        let state = self.state.borrow();
        match &state.cached_text {
            None => (0, 0),
            Some(text) => {
                let width = text.width().round() as i32;
                let height = text.height().round() as i32;
                (width, height)
            }
        }
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
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
