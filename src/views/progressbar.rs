use std::cell::{Cell, RefCell};

use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::view_base::{HasMainFields, ViewBasics};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain, Visibility};

const DEFAULT_HEIGHT: i32 = 16;
const INDETERMINATE_BLOCK_FRACTION: f64 = 0.25;
const INDETERMINATE_SPEED: f64 = 1.5; // full traversals per second

pub struct ProgressBar {
    state: RefCell<FieldsMain>,
    /// Progress value 0.0..=1.0 (ignored in indeterminate mode)
    value: Cell<f32>,
    /// If true, shows an animated bouncing block instead of a filled bar
    indeterminate: Cell<bool>,
    /// Animation position 0.0..1.0 for indeterminate mode, advanced in update()
    anim_pos: Cell<f64>,
    /// Animation direction: true = forward, false = backward
    anim_forward: Cell<bool>,
}

impl HasMainFields for ProgressBar {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ProgressBar {}

#[allow(dead_code)]
impl ProgressBar {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> ProgressBar {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.state.focusable = false;
        ProgressBar {
            state: RefCell::new(main),
            value: Cell::new(0.0),
            indeterminate: Cell::new(false),
            anim_pos: Cell::new(0.0),
            anim_forward: Cell::new(true),
        }
    }

    pub fn get_value(&self) -> f32 {
        self.value.get()
    }

    pub fn set_value(&self, value: f32) {
        self.value.set(value.clamp(0.0, 1.0));
    }

    pub fn is_indeterminate(&self) -> bool {
        self.indeterminate.get()
    }

    pub fn set_indeterminate(&self, indeterminate: bool) {
        self.indeterminate.set(indeterminate);
        if indeterminate {
            self.anim_pos.set(0.0);
            self.anim_forward.set(true);
        }
    }
}

impl View for ProgressBar {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "value" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.value.set(v.clamp(0.0, 1.0));
                }
            }
            "indeterminate" => {
                self.set_indeterminate(value == "true");
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
        let (new_width, new_height) = self.calculate_size(width, height, scale);
        let r = rect((x, y), (x + new_width, y + new_height));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let r = self.state.borrow().rect;
        r.width() <= width && r.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);

        // Draw sunken track
        theme.draw_progressbar_track(r);

        // Inner area (inside the 2px sunken border)
        let scale = state.scale;
        let border = (2.0 * scale).round() as i32;
        let inner = rect(
            (r.min.x + border, r.min.y + border),
            (r.max.x - border, r.max.y - border),
        );
        let inner_width = inner.width();

        if self.indeterminate.get() {
            // Bouncing block
            let block_width = (inner_width as f64 * INDETERMINATE_BLOCK_FRACTION).round() as i32;
            let travel = inner_width - block_width;
            let pos = (self.anim_pos.get() * travel as f64).round() as i32;
            let fill = rect(
                (inner.min.x + pos, inner.min.y),
                (inner.min.x + pos + block_width, inner.max.y),
            );
            theme.draw_progressbar_fill(fill);
        } else {
            // Determinate fill
            let fill_width = (inner_width as f32 * self.value.get()).round() as i32;
            if fill_width > 0 {
                let fill = rect(
                    (inner.min.x, inner.min.y),
                    (inner.min.x + fill_width, inner.max.y),
                );
                theme.draw_progressbar_fill(fill);
            }
        }
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
        let scale = self.state.borrow().scale;
        let h = (DEFAULT_HEIGHT as f64 * scale).round() as i32;
        (0, h)
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

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        // No events for progress bar
    }

    fn click(&self, _ui: &mut UI) -> bool {
        false
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        if !self.indeterminate.get() {
            return false;
        }
        // Advance animation (~60fps, 15ms per frame)
        let dt = 1.0 / 60.0;
        let speed = INDETERMINATE_SPEED * dt;
        let mut pos = self.anim_pos.get();
        if self.anim_forward.get() {
            pos += speed;
            if pos >= 1.0 {
                pos = 1.0;
                self.anim_forward.set(false);
            }
        } else {
            pos -= speed;
            if pos <= 0.0 {
                pos = 0.0;
                self.anim_forward.set(true);
            }
        }
        self.anim_pos.set(pos);
        true // always redraw when indeterminate
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        let r = rect((0, 0), (200, DEFAULT_HEIGHT));
        ProgressBar::new(r, Dimension::Max, Dimension::Min)
    }
}
