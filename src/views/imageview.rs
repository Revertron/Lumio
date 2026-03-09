use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;

use image::GenericImageView;
use speedy2d::dimen::Vector2;
use speedy2d::window::MouseButton;

use crate::assets::get_asset;
use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain};
use crate::view_base::{HasMainFields, ViewBasics};

const DEFAULT_IMAGE_SIZE: u32 = 32;

pub struct ImageView {
    state: RefCell<FieldsMain>,
    image_path: RefCell<String>,
    image_bytes: RefCell<Option<Vec<u8>>>,
    /// Natural image dimensions (width, height) in pixels, decoded from image data
    natural_size: RefCell<(u32, u32)>,
    listeners: RefCell<HashMap<EventType, Box<dyn FnMut(&mut UI, &dyn View) -> bool>>>,
}

impl HasMainFields for ImageView {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ImageView {}

impl ImageView {
    fn load_image(&self) {
        if self.image_bytes.borrow().is_some() {
            return;
        }
        let path = self.image_path.borrow().clone();
        if path.is_empty() {
            return;
        }
        if let Some(bytes) = get_asset(&path) {
            // Decode natural image dimensions
            match image::load(Cursor::new(&bytes), image::ImageFormat::from_path(&path).unwrap_or(image::ImageFormat::Png)) {
                Ok(img) => {
                    let (w, h) = img.dimensions();
                    *self.natural_size.borrow_mut() = (w, h);
                }
                Err(e) => {
                    println!("ImageView: failed to decode image dimensions: {}", e);
                }
            }
            *self.image_bytes.borrow_mut() = Some(bytes);
        } else {
            println!("ImageView: asset not found: {}", path);
        }
    }
}

impl View for ImageView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "image" => {
                *self.image_path.borrow_mut() = value.to_owned();
                *self.image_bytes.borrow_mut() = None;
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
        self.load_image();
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

        if let Some(ref bytes) = *self.image_bytes.borrow() {
            let padding = state.padding.scaled(state.scale);
            let content_w = r.width() - padding.left - padding.right;
            let content_h = r.height() - padding.top - padding.bottom;

            let (nat_w, nat_h) = *self.natural_size.borrow();
            let aspect = if nat_w > 0 && nat_h > 0 { nat_w as f64 / nat_h as f64 } else { 1.0 };

            // Fit image within content area preserving aspect ratio
            let (img_w, img_h) = if (content_w as f64 / aspect) <= content_h as f64 {
                (content_w, (content_w as f64 / aspect).round() as i32)
            } else {
                ((content_h as f64 * aspect).round() as i32, content_h)
            };
            let img_x = r.min.x + padding.left + (content_w - img_w) / 2;
            let img_y = r.min.y + padding.top + (content_h - img_h) / 2;
            let img_rect = rect((img_x, img_y), (img_x + img_w, img_y + img_h));
            theme.draw_image(img_rect, bytes);
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

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let state = self.state.borrow();
        let (nat_w, nat_h) = *self.natural_size.borrow();
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

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        let old_state = self.state.borrow().state;
        self.state.borrow_mut().state.hovered = hit;
        let changed = self.state.borrow().state != old_state;
        // Fire MouseMove listener if hovered
        if hit {
            let listener = self.listeners.borrow_mut().remove(&EventType::MouseMove);
            if let Some(mut func) = listener {
                func(ui, self as &dyn View);
                self.listeners.borrow_mut().insert(EventType::MouseMove, func);
            }
        }
        changed
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
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

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
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
            image_path: RefCell::new(String::new()),
            image_bytes: RefCell::new(None),
            natural_size: RefCell::new((0, 0)),
            listeners: RefCell::new(HashMap::new()),
        }
    }
}
