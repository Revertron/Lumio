use std::cell::RefCell;

use crate::input::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};

use crate::events::{EventCallback, EventData, EventType};
use crate::image_source::ImageSource;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::view_base::{HasMainFields, ViewBasics};

const DEFAULT_IMAGE_SIZE: u32 = 32;

pub struct ImageButton {
    state: RefCell<FieldsMain>,
    /// The normal image; `None` until set, replaced wholesale on change.
    image: RefCell<Option<ImageSource>>,
    /// The image shown while hovered/pressed (optional).
    hover_image: RefCell<Option<ImageSource>>,
    flat: RefCell<bool>,
    /// When true, suppress the inset border frame on press
    no_inset: RefCell<bool>,
}

impl HasMainFields for ImageButton {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ImageButton {}

impl ImageButton {
    /// Eagerly load both images so `is_loaded` is meaningful during paint.
    fn load_images(&self) {
        if let Some(s) = self.image.borrow_mut().as_mut() {
            s.ensure_loaded();
        }
        if let Some(s) = self.hover_image.borrow_mut().as_mut() {
            s.ensure_loaded();
        }
    }
}

impl View for ImageButton {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "image" => {
                *self.image.borrow_mut() = Some(ImageSource::new(value));
            }
            "hover_image" => {
                *self.hover_image.borrow_mut() = Some(ImageSource::new(value));
            }
            "flat" => {
                *self.flat.borrow_mut() = value.parse().unwrap_or(true);
            }
            "no_inset" => {
                *self.no_inset.borrow_mut() = value.parse().unwrap_or(false);
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

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, _typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.base_set_scale(scale);
        self.load_images();
        let (width, height) = self.calculate_size(width, height, scale);
        let padding = self.get_padding(scale);
        let full_width = padding.left + width + padding.right;
        let full_height = padding.top + height + padding.bottom;
        let r = rect((x, y), (x + full_width, y + full_height));
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
        let flat = *self.flat.borrow();
        let no_inset = *self.no_inset.borrow();
        let has_hover_image = self.hover_image.borrow().is_some();

        theme.push_clip();
        theme.clip_rect(r);

        // Draw background when not flat, or when hovered/pressed (but skip if hover_image handles hover)
        if !flat {
            theme.draw_component("button.back", r, state.state);
        } else if (state.state.hovered || state.state.pressed) && !has_hover_image {
            theme.draw_component("button.back", r, state.state);
        }

        // Use the hover image when hovered/pressed and it actually loaded
        // (loaded eagerly in layout_content), otherwise the normal image.
        let use_hover = (state.state.hovered || state.state.pressed)
            && self.hover_image.borrow().as_ref().is_some_and(|s| s.is_loaded());

        let padding = state.padding.scaled(state.scale);
        let content_w = r.width() - padding.left - padding.right;
        let content_h = r.height() - padding.top - padding.bottom;
        let img_size = content_w.min(content_h);
        let img_x = r.min.x + padding.left + (content_w - img_size) / 2;
        let img_y = r.min.y + padding.top + (content_h - img_size) / 2;
        let img_rect = rect((img_x, img_y), (img_x + img_size, img_y + img_size));

        if img_size > 0 {
            let cell = if use_hover { &self.hover_image } else { &self.image };
            if let Some(img) = cell.borrow_mut().as_mut() {
                img.draw(theme, img_rect, 0xFFFFFFFF);
            }
        }

        // Draw borders when not flat, or when pressed (unless no_inset suppresses it)
        if !flat && !(no_inset && state.state.pressed) {
            theme.draw_component("button.body", r, state.state);
        } else if state.state.pressed && !no_inset {
            theme.draw_component("button.body", r, state.state);
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
        let size = match state.width {
            Dimension::Dip(d) => {
                let padding = state.padding.scaled(state.scale);
                (d as f64 * state.scale).round() as i32 - padding.left - padding.right
            }
            _ => (DEFAULT_IMAGE_SIZE as f64 * state.scale).round() as i32,
        };
        let h = match state.height {
            Dimension::Dip(d) => {
                let padding = state.padding.scaled(state.scale);
                (d as f64 * state.scale).round() as i32 - padding.top - padding.bottom
            }
            _ => size,
        };
        (size, h)
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

    fn get_content_description(&self) -> Option<String> {
        self.base_get_content_description()
    }

    fn set_content_description(&mut self, description: Option<String>) {
        self.base_set_content_description(description);
    }

    fn get_labelled_by(&self) -> Option<String> {
        self.base_get_labelled_by()
    }

    fn set_labelled_by(&mut self, view_id: Option<String>) {
        self.base_set_labelled_by(view_id);
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

    fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::Button);
        // No intrinsic text: the tooltip doubles as the accessible name until
        // a content_description is set (Phase B).
        if let Some(tooltip) = self.get_tooltip()
            && !tooltip.is_empty()
        {
            node.set_label(tooltip);
        }
        node.add_action(accesskit::Action::Click);
        node
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Point<i32>) -> bool {
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        let old_state = self.state.borrow().state;
        self.state.borrow_mut().state.hovered = hit;
        self.state.borrow().state != old_state
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        if hit {
            {
                let mut state = self.state.borrow_mut();
                if matches!(button, MouseButton::Left) {
                    state.state.pressed = true;
                }
                state.state.focused = true;
            }
            if matches!(button, MouseButton::Left) {
                // Press notification for hold-style interactions; Click still
                // fires separately on release inside the button.
                self.fire_event(ui, EventType::MouseDown, &EventData::None);
            }
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        if matches!(button, MouseButton::Left) {
            if self.state.borrow().state.pressed {
                if hit {
                    self.click(ui);
                }
                self.state.borrow_mut().state.pressed = false;
                return true;
            }
        }
        false
    }

    // Space/Enter activate the focused button: press on key down, click on key up.
    fn on_key_down(&self, _ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        if matches!(virtual_key_code, Some(VirtualKeyCode::Space | VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)) {
            self.state.borrow_mut().state.pressed = true;
            return true;
        }
        false
    }

    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        if matches!(virtual_key_code, Some(VirtualKeyCode::Space | VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter))
            && self.state.borrow().state.pressed {
            self.state.borrow_mut().state.pressed = false;
            self.click(ui);
            return true;
        }
        false
    }
}

impl Default for ImageButton {
    fn default() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (DEFAULT_IMAGE_SIZE as i32, DEFAULT_IMAGE_SIZE as i32)), Dimension::Dip(DEFAULT_IMAGE_SIZE), Dimension::Dip(DEFAULT_IMAGE_SIZE));
        main.padding = Borders::with_padding(4);
        ImageButton {
            state: RefCell::new(main),
            image: RefCell::new(None),
            hover_image: RefCell::new(None),
            flat: RefCell::new(true),
            no_inset: RefCell::new(false),
        }
    }
}
