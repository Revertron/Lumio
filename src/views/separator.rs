use std::cell::RefCell;
use std::str::FromStr;

use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::view_base::{HasMainFields, ViewBasics};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Direction, FieldsMain, Visibility};

const DEFAULT_THICKNESS: i32 = 2;

pub struct Separator {
    state: RefCell<FieldsMain>,
    direction: Direction,
}

impl HasMainFields for Separator {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for Separator {}

#[allow(dead_code)]
impl Separator {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension, direction: Direction) -> Separator {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.state.focusable = false;
        Separator {
            state: RefCell::new(main),
            direction,
        }
    }

    pub fn get_direction(&self) -> Direction {
        self.direction
    }
}

impl View for Separator {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "direction" => {
                if let Ok(d) = Direction::from_str(value) {
                    self.direction = d;
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
        theme.draw_separator(r, state.state);
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
        let thickness = (DEFAULT_THICKNESS as f64 * scale).round() as i32;
        match self.direction {
            Direction::Horizontal => (0, thickness),
            Direction::Vertical => (thickness, 0),
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

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        // No events for separator
    }

    fn click(&self, _ui: &mut UI) -> bool {
        false
    }
}

impl Default for Separator {
    fn default() -> Self {
        let r = rect((0, 0), (0, DEFAULT_THICKNESS));
        Separator::new(r, Dimension::Max, Dimension::Dip(DEFAULT_THICKNESS as u32), Direction::Horizontal)
    }
}
