use std::cell::RefCell;
use speedy2d::dimen::Vector2;
use crate::text::{TextBlock, TextOptions};
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};
use super::super::assets::get_font_family;
use super::super::common::DEFAULT_TEXT_SIZE;
use super::super::events::{EventCallback, EventData, EventType};
use super::super::themes::{Theme, Typeface, ViewState};
use super::super::traits::{Element, View, WeakElement};
use super::super::types::{Point, Rect, rect};
use super::super::ui::UI;
use super::super::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use super::super::view_base::{HasMainFields, ViewBasics};

pub struct List {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<String>>,
    texts: RefCell<Vec<Option<TextBlock>>>,
    text_size: f32,
    scroll_y: RefCell<i32>,
    selected: RefCell<Option<usize>>
}

impl HasMainFields for List {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for List {}

impl List {
    pub fn new(rect: Rect<i32>) -> List {
        List {
            state: RefCell::new(FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min)),
            items: RefCell::new(vec![]),
            texts: RefCell::new(vec![]),
            text_size: crate::drawing::current_text_size("text"),
            scroll_y: RefCell::new(0),
            selected: RefCell::new(None)
        }
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = RefCell::new(items);
        self.texts.borrow_mut().clear();
        let typeface = self.state.borrow().font_manager.get().unwrap();
        let scale = self.state.borrow().scale as f32;
        let base_size = typeface.font_size.unwrap_or(self.text_size);
        for i in self.items.borrow().iter() {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let options = TextOptions::new();
                let text = font.layout_text(&i, base_size * scale, options);
                self.texts.borrow_mut().push(Some(text));
            }
        }
    }

    fn get_hit_item(&self, _x: i32, y: i32) -> Option<usize> {
        let mut index = 0;
        let mut yy = 0;
        let scroll_y = *self.scroll_y.borrow();
        for v in self.texts.borrow().iter() {
            if let Some(text) = v {
                let height = text.height().ceil() as i32;
                if y >= yy + scroll_y && y < yy + scroll_y + height {
                    return Some(index);
                }
                yy += height;
            } else {
                yy += DEFAULT_TEXT_SIZE as i32;
            }
            index += 1;
        }
        None
    }

    pub fn select_item(&self, index: usize) -> bool {
        if index > self.items.borrow().len() {
            return false;
        }
        *self.selected.borrow_mut() = Some(index);
        let mut yy = 0;
        let rect_height = self.get_rect_height();
        let scroll_y = *self.scroll_y.borrow();
        let mut count = 0;
        for t in self.texts.borrow().iter() {
            if let Some(text) = t {
                let height = text.height().ceil() as i32;
                if count != index {
                    yy += height;
                    count += 1;
                    continue;
                }
                let delta = rect_height - (yy + height + scroll_y);
                if delta < 0 {
                    *self.scroll_y.borrow_mut() += delta;
                } else if yy + scroll_y < 0 {
                    *self.scroll_y.borrow_mut() -= yy + scroll_y;
                }
                yy += height;
            }
            count += 1;
        }
        true
    }
}

impl View for List {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.state.borrow_mut().font_manager.set_font_size(size);
                    self.text_size = size;
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
        self.state.borrow_mut().font_manager.set(Some(typeface.clone()));
        self.base_set_scale(scale);
        let (new_width, new_height) = self.calculate_size(width, height, scale);
        let (width, height) = {
            let state = self.state.borrow_mut();
            let ww;
            let hh;
            match &state.width {
                Dimension::Min => ww = 0,
                Dimension::Max => ww = new_width,
                Dimension::Dip(dip) => ww = (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => ww = (width as f32 * p / 100f32).round() as i32
            }
            match &state.height {
                Dimension::Min => hh = 0,
                Dimension::Max => hh = new_height,
                Dimension::Dip(dip) => hh = (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => hh = (height as f32 * p / 100f32).round() as i32
            }
            (ww, hh)
        };
        let rect = rect((x, y), (x + width, y + height));
        self.set_rect(rect);
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let rect = self.get_rect();
        rect.width() <= width && rect.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let mut rect = self.get_rect();
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        let state = self.get_state().unwrap();

        // Step 1: Draw background (before items)
        theme.draw_component("edit.back", rect, state);

        //let color = theme.get_text_color(self.state.borrow().state, &self.state.borrow().foreground);
        let mut y = rect.min.y;
        let mut index = 0usize;
        let selected = *self.selected.borrow();
        let scroll_y = *self.scroll_y.borrow();
        for v in self.texts.borrow().iter() {
            if let Some(text) = v {
                let text_height = text.height().ceil() as i32;
                let mut text_color: u32 = theme.color("text");
                if let Some(s) = selected {
                    if s == index {
                        let rect = super::super::types::rect((rect.min.x + 2, (y + scroll_y)), (rect.max.x - 2, (y + scroll_y) + text_height));
                        theme.draw_rect(rect, theme.color("item_highlight"));
                        text_color = theme.color("item_highlight_text");
                    }
                }

                theme.draw_text((rect.min.x + 10) as f32, (y + scroll_y) as f32, text_color, text);
                y += text_height;
            }
            index += 1;
        }

        // Step 2: Draw borders (after items)
        theme.draw_component("edit.body", rect, state);

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
        // Return content size matching what was set in the rect
        let state = self.state.borrow();
        let scale = state.scale;
        let width = match &state.width {
            Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
            _ => {
                // For Max/Min/Percent, use the current rect size minus padding
                let rect = self.get_rect();
                let padding = self.get_padding(scale);
                (rect.width() - padding.left - padding.right).max(0)
            }
        };
        let height = match &state.height {
            Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
            _ => {
                let rect = self.get_rect();
                let padding = self.get_padding(scale);
                (rect.height() - padding.top - padding.bottom).max(0)
            }
        };
        (width, height)
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

    fn click(&self, _ui: &mut UI) -> bool {
        todo!()
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        println!("Mouse down in {}", self.get_id());
        if self.state.borrow().rect.hit((position.x, position.y)) {
            println!("hit list");
            if matches!(button, MouseButton::Left) {
                self.state.borrow_mut().state.pressed = true;
            }
            self.state.borrow_mut().state.focused = true;
            let rect = self.state.borrow_mut().rect;
            if let Some(index) = self.get_hit_item(position.x - rect.min.x, position.y - rect.min.y) {
                self.select_item(index);
                println!("Selected item {:?}", *self.selected.borrow());
            }
            return true;
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Vector2<i32>, distance: MouseScrollDistance) -> bool {
        if self.state.borrow().rect.hit((position.x, position.y)) {
            let mut scroll_y = *self.scroll_y.borrow();

            // Get line height from first text item (or use default)
            let line_height = self.texts.borrow().first()
                .and_then(|t| t.as_ref().map(|text| text.height().ceil() as i32))
                .unwrap_or(DEFAULT_TEXT_SIZE as i32);

            // Calculate total content height
            let mut total_height = 0i32;
            for v in self.texts.borrow().iter() {
                if let Some(text) = v {
                    total_height += text.height().ceil() as i32;
                }
            }

            let rect_height = self.get_rect().height();
            let max_scroll = -(total_height - rect_height).max(0);

            match &distance {
                MouseScrollDistance::Lines { y, .. } => {
                    // Scroll by lines (positive y = scroll down, negative = scroll up)
                    scroll_y += (*y as i32) * line_height;
                }
                MouseScrollDistance::Pixels { y, .. } => {
                    // Scroll by pixels
                    scroll_y += *y as i32;
                }
                MouseScrollDistance::Pages { y, .. } => {
                    // Scroll by pages
                    scroll_y += (*y as i32) * rect_height;
                }
            }

            // Clamp scroll to valid range (max_scroll <= scroll_y <= 0)
            scroll_y = scroll_y.clamp(max_scroll, 0);
            *self.scroll_y.borrow_mut() = scroll_y;

            true
        } else {
            false
        }
    }


    fn on_key_down(&self, _ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if let Some(code) = virtual_key_code {
            if !self.state.borrow().state.focused || code == VirtualKeyCode::Tab {
                return false;
            }
            let length = self.items.borrow().len();

            if code == VirtualKeyCode::PageUp {
                if length > 0 {
                    self.select_item(0);
                }
            }
            if code == VirtualKeyCode::PageDown {
                if length > 0 {
                    self.select_item(length - 1);
                }
            }
            if code == VirtualKeyCode::Up {
                let selected = self.selected.borrow().clone();
                match selected {
                    None => {
                        if length > 0 {
                            self.select_item(length - 1);
                        }
                    }
                    Some(s) => {
                        if s > 0 {
                            self.select_item(s - 1);
                        }
                    }
                }
            }
            if code == VirtualKeyCode::Down {
                let selected = self.selected.borrow().clone();
                match selected {
                    None => {
                        if self.items.borrow().len() > 0 {
                            self.select_item(0);
                        }
                    }
                    Some(s) => {
                        if s < self.items.borrow().len() - 1 {
                            self.select_item(s + 1);
                        }
                    }
                }
            }
        }
        true
    }
}

impl Default for List {
    fn default() -> Self {
        let rect = rect((0, 0), (100, 200));
        List::new(rect)
    }
}