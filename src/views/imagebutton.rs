use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use speedy2d::dimen::Vector2;
use speedy2d::window::MouseButton;

use crate::assets::get_asset;
use crate::events::EventType;
use crate::svg;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::view_base::{HasMainFields, ViewBasics};

const DEFAULT_IMAGE_SIZE: u32 = 32;

pub struct ImageButton {
    state: RefCell<FieldsMain>,
    image_path: RefCell<String>,
    image_bytes: RefCell<Option<Vec<u8>>>,
    image_is_svg: RefCell<bool>,
    rasterized: RefCell<Option<(u32, u32, Vec<u8>)>>,
    hover_image_path: RefCell<String>,
    hover_image_bytes: RefCell<Option<Vec<u8>>>,
    hover_image_is_svg: RefCell<bool>,
    hover_rasterized: RefCell<Option<(u32, u32, Vec<u8>)>>,
    flat: RefCell<bool>,
    /// When true, suppress the inset border frame on press
    no_inset: RefCell<bool>,
    listeners: RefCell<HashMap<EventType, Box<dyn FnMut(&mut UI, &dyn View) -> bool>>>,
}

fn path_size_key(path: &str, w: u32, h: u32) -> u64 {
    let mut h_state = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h_state);
    w.hash(&mut h_state);
    h.hash(&mut h_state);
    h_state.finish()
}

impl HasMainFields for ImageButton {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ImageButton {}

impl ImageButton {
    fn load_image(&self) {
        if self.image_bytes.borrow().is_none() {
            let path = self.image_path.borrow().clone();
            if !path.is_empty() {
                if let Some(bytes) = get_asset(&path) {
                    let is_svg = path.to_ascii_lowercase().ends_with(".svg") || svg::looks_like_svg(&bytes);
                    *self.image_is_svg.borrow_mut() = is_svg;
                    *self.image_bytes.borrow_mut() = Some(bytes);
                } else {
                    println!("ImageButton: asset not found: {}", path);
                }
            }
        }
        if self.hover_image_bytes.borrow().is_none() {
            let path = self.hover_image_path.borrow().clone();
            if !path.is_empty() {
                if let Some(bytes) = get_asset(&path) {
                    let is_svg = path.to_ascii_lowercase().ends_with(".svg") || svg::looks_like_svg(&bytes);
                    *self.hover_image_is_svg.borrow_mut() = is_svg;
                    *self.hover_image_bytes.borrow_mut() = Some(bytes);
                } else {
                    println!("ImageButton: hover asset not found: {}", path);
                }
            }
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
                *self.image_path.borrow_mut() = value.to_owned();
                *self.image_bytes.borrow_mut() = None;
                *self.image_is_svg.borrow_mut() = false;
                *self.rasterized.borrow_mut() = None;
            }
            "hover_image" => {
                *self.hover_image_path.borrow_mut() = value.to_owned();
                *self.hover_image_bytes.borrow_mut() = None;
                *self.hover_image_is_svg.borrow_mut() = false;
                *self.hover_rasterized.borrow_mut() = None;
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
        self.load_image();
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
        let has_hover_image = self.hover_image_path.borrow().len() > 0;

        theme.push_clip();
        theme.clip_rect(r);

        // Draw background when not flat, or when hovered/pressed (but skip if hover_image handles hover)
        if !flat {
            theme.draw_component("button_classic_back", r, state.state);
        } else if (state.state.hovered || state.state.pressed) && !has_hover_image {
            theme.draw_component("button_classic_back", r, state.state);
        }

        // Pick which image bytes to draw: hover image when hovered/pressed, otherwise normal
        let use_hover = (state.state.hovered || state.state.pressed)
            && self.hover_image_bytes.borrow().is_some();
        let active_is_svg = if use_hover { *self.hover_image_is_svg.borrow() } else { *self.image_is_svg.borrow() };

        let padding = state.padding.scaled(state.scale);
        let content_w = r.width() - padding.left - padding.right;
        let content_h = r.height() - padding.top - padding.bottom;
        let img_size = content_w.min(content_h);
        let img_x = r.min.x + padding.left + (content_w - img_size) / 2;
        let img_y = r.min.y + padding.top + (content_h - img_size) / 2;
        let img_rect = rect((img_x, img_y), (img_x + img_size, img_y + img_size));

        if active_is_svg && img_size > 0 {
            let w = img_size as u32;
            let h = img_size as u32;
            let path = if use_hover { self.hover_image_path.borrow().clone() } else { self.image_path.borrow().clone() };
            let raster_cell = if use_hover { &self.hover_rasterized } else { &self.rasterized };
            let bytes_cell = if use_hover { &self.hover_image_bytes } else { &self.image_bytes };

            let needs_render = match &*raster_cell.borrow() {
                Some((cw, ch, _)) => *cw != w || *ch != h,
                None => true,
            };
            if needs_render {
                if let Some(ref src) = *bytes_cell.borrow() {
                    if let Some(rgba) = svg::rasterize(src, w, h) {
                        *raster_cell.borrow_mut() = Some((w, h, rgba));
                    }
                }
            }
            if let Some((cw, ch, rgba)) = &*raster_cell.borrow() {
                let key = path_size_key(&path, *cw, *ch);
                theme.draw_raw_image(img_rect, rgba, (*cw, *ch), key);
            }
        } else {
            let hover_bytes = self.hover_image_bytes.borrow();
            let normal_bytes = self.image_bytes.borrow();
            let active_bytes = if use_hover { &hover_bytes } else { &normal_bytes };
            if let Some(ref bytes) = **active_bytes {
                theme.draw_image(img_rect, bytes);
            }
        }

        // Draw borders when not flat, or when pressed (unless no_inset suppresses it)
        if !flat && !(no_inset && state.state.pressed) {
            theme.draw_component("button_classic_body", r, state.state);
        } else if state.state.pressed && !no_inset {
            theme.draw_component("button_classic_body", r, state.state);
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
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        let old_state = self.state.borrow().state;
        self.state.borrow_mut().state.hovered = hit;
        self.state.borrow().state != old_state
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().rect.hit((position.x, position.y));
        if hit {
            let mut state = self.state.borrow_mut();
            if matches!(button, MouseButton::Left) {
                state.state.pressed = true;
            }
            state.state.focused = true;
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
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

impl Default for ImageButton {
    fn default() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (DEFAULT_IMAGE_SIZE as i32, DEFAULT_IMAGE_SIZE as i32)), Dimension::Dip(DEFAULT_IMAGE_SIZE), Dimension::Dip(DEFAULT_IMAGE_SIZE));
        main.padding = Borders::with_padding(4);
        ImageButton {
            state: RefCell::new(main),
            image_path: RefCell::new(String::new()),
            image_bytes: RefCell::new(None),
            image_is_svg: RefCell::new(false),
            rasterized: RefCell::new(None),
            hover_image_path: RefCell::new(String::new()),
            hover_image_bytes: RefCell::new(None),
            hover_image_is_svg: RefCell::new(false),
            hover_rasterized: RefCell::new(None),
            flat: RefCell::new(true),
            no_inset: RefCell::new(false),
            listeners: RefCell::new(HashMap::new()),
        }
    }
}
