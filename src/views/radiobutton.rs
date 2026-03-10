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
use crate::view_base::{HasMainFields, ViewBasics};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Visibility};
use crate::styles::selector::FontSelector;
use crate::views::{FieldsMain, FieldsTexted};
use crate::views::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH};

pub struct RadioButton {
    state: RefCell<FieldsTexted>,
    text_margin: i32,
    group: RefCell<String>,
}

impl HasMainFields for RadioButton {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for RadioButton {}

const DEFAULT_TEXT_MARGIN: i32 = 6;
const DEFAULT_BOX_SIZE: i32 = 16;
const DEFAULT_LEFT_INSET: i32 = 4;

#[allow(dead_code)]
impl RadioButton {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> RadioButton {
        let main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        RadioButton {
            state: RefCell::new(FieldsTexted {
                main,
                text: text.to_owned(),
                text_size,
                line_height: 0f32,
                single_line: true,
                cached_text: None,
                font: FontSelector::new(),
                listeners: HashMap::new()
            }),
            text_margin: DEFAULT_TEXT_MARGIN,
            group: RefCell::new(String::new()),
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

    pub fn is_checked(&self) -> bool {
        self.state.borrow().main.state.checked
    }

    pub fn set_checked(&self, checked: bool) {
        self.state.borrow_mut().main.state.checked = checked;
    }

    pub fn get_group(&self) -> String {
        self.group.borrow().clone()
    }

    pub fn set_group(&self, group: &str) {
        *self.group.borrow_mut() = group.to_owned();
    }

    pub fn set_single_line(&self, single_line: bool) {
        let mut state = self.state.borrow_mut();
        state.single_line = single_line;
        state.cached_text = None;
    }

    /// Returns the checked RadioButton in the given group, or None if none is checked.
    pub fn get_selected(ui: &UI, group: &str) -> Option<Element> {
        let results = ui.find_with(&|view: &dyn View| {
            if let Some(rb) = view.as_any().downcast_ref::<RadioButton>() {
                *rb.group.borrow() == group && rb.is_checked()
            } else {
                false
            }
        });
        results.into_iter().next()
    }

    fn uncheck_siblings_in_group(&self) {
        let group = self.group.borrow().clone();
        if group.is_empty() {
            return;
        }
        let my_id = self.base_get_id();
        if let Some(parent) = self.base_get_parent() {
            let parent_ref = parent.borrow();
            if let Some(container) = parent_ref.as_container() {
                for sibling in container.get_views() {
                    let sibling_ref = sibling.borrow();
                    if sibling_ref.get_id() == my_id {
                        continue;
                    }
                    if let Some(rb) = sibling_ref.as_any().downcast_ref::<RadioButton>() {
                        if *rb.group.borrow() == group {
                            rb.set_checked(false);
                        }
                    }
                }
            }
        }
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
                let scale = scale.round() as i32;
                let box_size = DEFAULT_BOX_SIZE * scale;
                let text_margin = self.text_margin * scale;
                let width = max_width - box_size - text_margin;
                let options = match single_line {
                    true => TextOptions::new(),
                    false => TextOptions::new().with_wrap_to_width(width as f32, TextAlignment::Left)
                };
                let size = self.state.borrow().text_size * scale as f32;
                let text = font.layout_text(&self.state.borrow().text, size, options);
                self.state.borrow_mut().cached_text = Some(text);
            }
        }
    }
}

impl View for RadioButton {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "text" => { self.set_text(value) }
            "group" => { self.set_group(value) }
            "checked" => {
                if let Ok(checked) = value.parse::<bool>() {
                    self.set_checked(checked);
                }
            }
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
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface));
        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let max_width = width.max(DEFAULT_BOX_SIZE) - horizontal;
        let max_height = height.max(DEFAULT_BOX_SIZE) - vertical;
        let (new_width, _new_height) = self.calculate_size(max_width, max_height, scale);
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
        let box_size = DEFAULT_BOX_SIZE * state.main.scale.round() as i32;
        let mut rect = state.main.rect;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        let left_inset = DEFAULT_LEFT_INSET * state.main.scale.round() as i32;
        let box_y = (self.get_rect_height() - box_size) / 2;
        let box_rect = super::super::types::rect((rect.min.x + left_inset, rect.min.y + box_y), (rect.min.x + left_inset + box_size, rect.min.y + box_y + box_size));

        // Step 1: Draw radio background (circle)
        theme.draw_radiobutton_back(box_rect, state.main.state);

        // Step 2: Draw text label
        if let Some(text) = &state.cached_text {
            let x = (rect.min.x as f32 + left_inset as f32 + box_size as f32 + self.text_margin as f32 * state.main.scale as f32) as f32;
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
            theme.draw_text(x.round(), (rect.min.y as f32 + y).round(), color, text);
        }

        // Step 3: Draw radio border (circle)
        theme.draw_radiobutton_body(box_rect, state.main.state);

        // Step 4: Draw indicator dot if checked
        if state.main.state.checked {
            theme.draw_radiobutton_indicator(box_rect, state.main.state);
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

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let state = self.state.borrow();
        let scale = state.main.scale.round() as i32;
        let box_size = DEFAULT_BOX_SIZE * scale;
        let text_margin = self.text_margin * scale;
        let left_inset = DEFAULT_LEFT_INSET * scale;
        match &state.cached_text {
            None => (left_inset + box_size, box_size),
            Some(text) => {
                let width = left_inset + text.width().ceil() as i32 + box_size + text_margin;
                let height = max(text.height().ceil() as i32, box_size / 2);
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

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        // RadioButton always sets checked to true (no toggle off)
        self.state.borrow_mut().main.state.checked = true;
        // Uncheck siblings in the same group
        self.uncheck_siblings_in_group();
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
        if !self.base_is_enabled() { return false; }
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
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        if matches!(button, MouseButton::Left) {
            if self.state.borrow().main.state.pressed {
                if hit {
                    self.click(ui);
                }
                let mut state = self.state.borrow_mut();
                state.main.state.pressed = false;
                return true;
            }
        }
        false
    }
}

impl Default for RadioButton {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        RadioButton::new(rect, "", DEFAULT_TEXT_SIZE)
    }
}
