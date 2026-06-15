use std::cell::RefCell;

use crate::input::MouseButton;

use crate::events::{EventCallback, EventData, EventType};
use crate::image_source::ImageSource;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::view_base::{HasMainFields, ViewBasics};

const DEFAULT_IMAGE_SIZE: u32 = 32;

pub struct ImageView {
    state: RefCell<FieldsMain>,
    /// The image (load, SVG rasterization, and GPU-cache lifetime). `None` until
    /// an image is set; replaced wholesale when the source changes.
    image: RefCell<Option<ImageSource>>,
    /// Optional ARGB tint multiplied with the image at draw time (`0xFFFFFFFF`
    /// = no change). Monochrome icons should be authored white to recolor.
    tint: RefCell<Option<u32>>,
}

impl HasMainFields for ImageView {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ImageView {}

impl ImageView {
    /// Change the image source. The previous `ImageSource` is dropped (its
    /// texture freed at the next paint) and a fresh one created.
    pub fn set_image(&mut self, path: &str) {
        *self.image.borrow_mut() = Some(ImageSource::new(path));
    }

    /// Set or clear an ARGB tint multiplied with the image at draw time. Pass
    /// `None` for no tint. Monochrome icons should be authored white to recolor.
    pub fn set_tint(&mut self, color: Option<u32>) {
        *self.tint.borrow_mut() = color;
    }

    /// Natural image size, loading the asset if needed (returns `(0, 0)` when
    /// no image is set).
    fn natural_size(&self) -> (u32, u32) {
        self.image
            .borrow_mut()
            .as_mut()
            .map(|s| s.natural_size())
            .unwrap_or((0, 0))
    }
}

impl View for ImageView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "image" => {
                *self.image.borrow_mut() = Some(ImageSource::new(value));
            }
            "tint" => {
                *self.tint.borrow_mut() = crate::view_base::parse_hex_color(value);
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

    fn layout_content(&mut self, x: i32, y: i32, _width: i32, _height: i32, _typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.base_set_scale(scale);
        let (content_w, content_h) = self.get_content_size();
        let padding = self.get_padding(scale);
        let full_width = padding.left + content_w + padding.right;
        let full_height = padding.top + content_h + padding.bottom;
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

        theme.push_clip();
        theme.clip_rect(r);

        // Read the natural size (loads the asset) under a short borrow, compute
        // the aspect-fitted rect, then draw under a fresh borrow — never nest two
        // borrows of `self.image`.
        let (nat_w, nat_h) = self.natural_size();
        if nat_w > 0 && nat_h > 0 {
            let padding = state.padding.scaled(state.scale);
            let content_w = r.width() - padding.left - padding.right;
            let content_h = r.height() - padding.top - padding.bottom;

            let aspect = nat_w as f64 / nat_h as f64;
            let (img_w, img_h) = if (content_w as f64 / aspect) <= content_h as f64 {
                (content_w, (content_w as f64 / aspect).round() as i32)
            } else {
                ((content_h as f64 * aspect).round() as i32, content_h)
            };
            let img_x = r.min.x + padding.left + (content_w - img_w) / 2;
            let img_y = r.min.y + padding.top + (content_h - img_h) / 2;
            let img_rect = rect((img_x, img_y), (img_x + img_w, img_y + img_h));

            if img_w > 0 && img_h > 0 {
                let tint = self.tint.borrow().unwrap_or(0xFFFFFFFF);
                if let Some(img) = self.image.borrow_mut().as_mut() {
                    img.draw(theme, img_rect, tint);
                }
            }
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
        let (nat_w, nat_h) = self.natural_size();
        let state = self.state.borrow();
        let aspect = if nat_w > 0 && nat_h > 0 {
            nat_w as f64 / nat_h as f64
        } else {
            1.0
        };

        let has_explicit_w = matches!(state.width, Dimension::Dip(_));
        let has_explicit_h = matches!(state.height, Dimension::Dip(_));

        match (has_explicit_w, has_explicit_h) {
            // Both set: use both as-is
            (true, true) => {
                let padding = state.padding.scaled(state.scale);
                let w = match state.width { Dimension::Dip(d) => d, _ => 0 };
                let h = match state.height { Dimension::Dip(d) => d, _ => 0 };
                let w = (w as f64 * state.scale).round() as i32 - padding.left - padding.right;
                let h = (h as f64 * state.scale).round() as i32 - padding.top - padding.bottom;
                (w, h)
            }
            // Only width set: derive height from aspect ratio
            (true, false) => {
                let padding = state.padding.scaled(state.scale);
                let w = match state.width { Dimension::Dip(d) => d, _ => 0 };
                let w = (w as f64 * state.scale).round() as i32 - padding.left - padding.right;
                let h = (w as f64 / aspect).round() as i32;
                (w, h)
            }
            // Only height set: derive width from aspect ratio
            (false, true) => {
                let padding = state.padding.scaled(state.scale);
                let h = match state.height { Dimension::Dip(d) => d, _ => 0 };
                let h = (h as f64 * state.scale).round() as i32 - padding.top - padding.bottom;
                let w = (h as f64 * aspect).round() as i32;
                (w, h)
            }
            // Neither set: use natural image size scaled
            (false, false) => {
                let w = (nat_w.max(DEFAULT_IMAGE_SIZE) as f64 * state.scale).round() as i32;
                let h = (nat_h.max(DEFAULT_IMAGE_SIZE) as f64 * state.scale).round() as i32;
                (w, h)
            }
        }
    }

    fn is_focused(&self) -> bool { false }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn set_focused(&self, _focused: bool) {}

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

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        let old_state = self.state.borrow().state;
        self.state.borrow_mut().state.hovered = hit;
        let changed = self.state.borrow().state != old_state;
        // Fire MouseMove listener if hovered
        if hit {
            let pos = ui.get_mouse_pos();
            self.base_fire_event(ui, EventType::MouseMove, &EventData::Position { x: pos.x, y: pos.y });
        }
        changed
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        if hit {
            let mut state = self.state.borrow_mut();
            if matches!(button, MouseButton::Left) {
                state.state.pressed = true;
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
}

impl Default for ImageView {
    fn default() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (DEFAULT_IMAGE_SIZE as i32, DEFAULT_IMAGE_SIZE as i32)), Dimension::Min, Dimension::Min);
        main.state.focusable = false;
        ImageView {
            state: RefCell::new(main),
            image: RefCell::new(None),
            tint: RefCell::new(None),
        }
    }
}
