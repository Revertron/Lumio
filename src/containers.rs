use std::cell::RefCell;
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};
use super::events::EventType;
use super::views::Borders;

use super::themes::{Theme, Typeface, ViewState};
use super::traits::{Container, Element, View, WeakElement};
use super::types::{Point, Rect, rect};
use super::ui::UI;
use super::views::{Dimension, Direction, FieldsMain};
use super::view_base::{HasMainFields, ViewBasics};

pub struct Frame {
    state: RefCell<FieldsMain>,
    direction: Direction,
    views: Vec<Element>,
    breaking: bool
}

impl HasMainFields for Frame {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for Frame {}

impl Frame {
    pub(crate) fn focus_next(&self) -> bool {
        let mut focused = -1;
        for i in 0..self.views.len() {
            let v = &self.views[i];
            if v.borrow().is_focused() {
                focused = i as i32;
                continue;
            }
            if let Some(state) = v.borrow().get_state() {
                if state.focusable && focused >= 0 {
                    let previous = &self.views[focused as usize];
                    previous.borrow().set_focused(false);
                    v.borrow().set_focused(true);
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn focus_prev(&self) -> bool {
        let mut focused = -1;
        for i in (0..self.views.len()).rev() {
            let v = &self.views[i];
            if v.borrow().is_focused() {
                focused = i as i32;
                continue;
            }
            if let Some(state) = v.borrow().get_state() {
                if state.focusable && focused >= 0 {
                    let previous = &self.views[focused as usize];
                    previous.borrow().set_focused(false);
                    v.borrow().set_focused(true);
                    return true;
                }
            }
        }
        false
    }
}

impl Frame {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> Frame {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.state.focusable = false;
        Frame {
            state: RefCell::new(main),
            direction: Direction::default(),
            views: Vec::new(),
            breaking: false
        }
    }

    fn set_font(&mut self, font_name: &str) {
        self.state.borrow_mut().font_manager.set_font(font_name);
    }

    fn set_font_style(&mut self, style: &str) {
        self.state.borrow_mut().font_manager.set_font_style(style);
    }

    fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }
}

impl Container for Frame {
    fn add_view(&mut self, view: Element) {
        self.views.push(view);
    }

    fn get_view(&self, id: &str) -> Option<Element> {
        //println!("Searching {} in Frame {}", &id, &self.get_id());
        if let Some(found) = self.views.iter().find(|&view| view.borrow().get_id() == id) {
            return Some(Rc::clone(found));
        }

        for v in self.views.iter() {
            if let Some(found) = v.borrow().as_container() {
                let view = found.get_view(id);
                if view.is_some() {
                    return view;
                }
            }
        }
        None
    }

    fn get_view_count(&self) -> usize {
        self.views.len()
    }

    fn get_views(&self) -> Vec<Element> {
        self.views.clone()
    }
}

impl View for Frame {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "direction" => { self.set_direction(value.parse().unwrap()) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "breaking" => { self.breaking = value.parse().unwrap_or(false) }
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
        self.base_set_scale(scale);
        //println!("Laying out for {},{} - {},{}", x, y, width, height);
        let (new_width, new_height) = self.calculate_size(width, height, scale);
        //println!("New width {}, new height {}", new_width, new_height);

        let padding = self.get_padding(scale);
        let mut xx = padding.left;
        let mut yy = padding.top;
        let max_x = new_width - padding.right;
        let mut max_height = 0;
        let typeface = match self.state.borrow().font_manager.get() {
            None => typeface.clone(),
            Some(t) => t
        };
        for v in self.views.iter() {
            let mut v = v.try_borrow_mut().unwrap();
            let margins = v.get_margin(scale);
            v.layout_content(xx + margins.left, yy + margins.top, new_width - xx - padding.right, new_height - yy - padding.bottom, &typeface, scale);
            // Get maximum occupied area
            let (w, h) = v.calculate_full_size(scale);
            match self.direction {
                Direction::Horizontal => xx = xx + w + margins.left + margins.right,
                Direction::Vertical => yy = yy + h + margins.top + margins.bottom
            }
            if self.breaking && self.direction == Direction::Horizontal {
                if xx > max_x {
                    yy += max_height + margins.top;
                    xx = padding.left + margins.left;
                    v.layout_content(xx, yy + margins.top, new_width - xx - padding.right, new_height - yy - padding.bottom, &typeface, scale);
                    // Get maximum occupied area
                    let (w, h) = v.calculate_full_size(scale);
                    xx += w;
                    max_height = h + margins.bottom;
                }
                if v.is_break() {
                    let (_, h) = v.calculate_full_size(scale);
                    xx = padding.left;
                    yy += h + margins.bottom;
                }
            }
            if h > max_height {
                max_height = h;
            }
            //println!("View {} is at rect {:?}", &v.get_id(), &v.get_rect());
        }

        let (w, h) = self.calculate_full_size(scale);
        let (width, height) = {
            let state = self.state.borrow_mut();
            let ww;
            let hh;
            match &state.width {
                Dimension::Min => ww = w,
                Dimension::Max => ww = new_width,
                Dimension::Dip(dip) => ww = (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => ww = (width as f32 * p / 100f32).round() as i32
            }
            match &state.height {
                Dimension::Min => hh = h,
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

    fn fits_in_rect(&self, width: i32, height: i32, scale: f64) -> bool {
        let size = self.calculate_full_size(scale);
        size.0 <= width && size.1 <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let mut rect = self.state.borrow().rect;
        let start = rect.min + origin;
        rect.move_by(origin);
        //println!("Drawing frame {} in rect: {:?}", self.get_id(), &rect);
        theme.push_clip();
        theme.clip_rect(rect);
        let state = self.state.borrow();
        if let Some(bg) = state.background.as_ref() {
            if let Some(crate::styles::selector::DrawState::Color(c)) = bg.get_state(&state.state) {
                theme.draw_rect(rect, *c);
            } else {
                theme.draw_panel_back(rect, state.state);
            }
        } else {
            theme.draw_panel_back(rect, state.state);
        }
        if let Some(border_color) = state.border_color {
            let r = rect;
            theme.draw_rect(super::types::rect((r.min.x, r.min.y), (r.max.x, r.min.y + 1)), border_color);
            theme.draw_rect(super::types::rect((r.min.x, r.max.y - 1), (r.max.x, r.max.y)), border_color);
            theme.draw_rect(super::types::rect((r.min.x, r.min.y), (r.min.x + 1, r.max.y)), border_color);
            theme.draw_rect(super::types::rect((r.max.x - 1, r.min.y), (r.max.x, r.max.y)), border_color);
        }
        drop(state);
        for v in self.views.iter() {
            let v = v.try_borrow().unwrap();
            v.paint(start, theme);
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
        let scale = self.state.borrow().scale;
        let mut rect = rect((-1, -1), (0, 0));
        for v in self.views.iter() {
            let v = v.borrow();
            // Get maximum occupied area
            let view_rect = v.get_rect();
            let margins = v.get_margin(scale);
            if rect.min.x == -1 || view_rect.min.x < rect.min.x {
                rect.min.x = view_rect.min.x;
                if margins.left != 0 {
                    rect.min.x -= margins.left;
                }
            }
            if rect.min.y == -1 || view_rect.min.y < rect.min.y {
                rect.min.y = view_rect.min.y;
                if margins.top != 0 {
                    rect.min.y -= margins.top;
                }
            }
            if view_rect.max.x + margins.right > rect.max.x {
                rect.max.x = view_rect.max.x + margins.right;
            }
            if view_rect.max.y + margins.bottom > rect.max.y {
                rect.max.y = view_rect.max.y + margins.bottom;
            }
        }
        (rect.width(), rect.height())
    }

    fn is_focused(&self) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused() {
                return true;
            }
        }
        false
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn set_focused(&self, focused: bool) {
        if focused {
            return;
        }
        for v in self.views.iter() {
            v.borrow().set_focused(false);
        }
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

    fn as_container(&self) -> Option<&dyn Container> {
        Some(self as &dyn Container)
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn Container> {
        Some(self as &mut dyn Container)
    }

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        // No op for now
    }

    fn click(&self, _ui: &mut UI) -> bool {
        // No op
        false
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        for v in self.views.iter() {
            if v.borrow_mut().update(ui) {
                return true;
            }
        }
        false
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        let position = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        let mut processed = false;
        for v in self.views.iter().rev() {
            processed |= v.borrow().on_mouse_move(ui, Vector2::from(position));
        }
        processed
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        println!("Mouse down in {}", &self.state.borrow().id);
        let position = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        let focused;
        for v in self.views.iter().rev() {
            let f = v.borrow().is_focused();
            if v.borrow().on_mouse_button_down(ui, Vector2::from(position), button) {
                // If focused changed to true
                focused = !f && v.borrow().is_focused();
                if focused {
                    for vv in self.views.iter() {
                        if vv.borrow().get_id() != v.borrow().get_id() {
                            vv.borrow_mut().set_focused(!focused);
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let position = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        for v in self.views.iter().rev() {
            if v.borrow().on_mouse_button_up(ui, Vector2::from(position), button) {
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Vector2<i32>, distance: speedy2d::window::MouseScrollDistance) -> bool {
        let position = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        for v in self.views.iter().rev() {
            if v.borrow().on_mouse_wheel_scroll(ui, Vector2::from(position), distance) {
                return true;
            }
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused() {
                println!("Found focused view {}", v.borrow().get_id());
                if v.borrow().on_key_down(ui, virtual_key_code, scancode, state.clone()) {
                    return true;
                }
            }
        }
        if let Some(code) = virtual_key_code {
            if code == VirtualKeyCode::Right && self.direction == Direction::Horizontal {
                if self.focus_next() {
                    return true;
                }
            }
            if code == VirtualKeyCode::Left && self.direction == Direction::Horizontal {
                if self.focus_prev() {
                    return true;
                }
            }
            if code == VirtualKeyCode::Up && self.direction == Direction::Vertical {
                if self.focus_prev() {
                    return true;
                }
            }
            if code == VirtualKeyCode::Down && self.direction == Direction::Vertical {
                if self.focus_next() {
                    return true;
                }
            }
        }
        println!("KD finished in {}", self.get_id());
        false
    }

    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused() {
                if v.borrow().on_key_up(ui, virtual_key_code, scancode, state.clone()) {
                    return true;
                }
            }
        }
        false
    }

    fn on_key_char(&self, ui: &mut UI, unicode_codepoint: char, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused() {
                if v.borrow().on_key_char(ui, unicode_codepoint, state.clone()) {
                    return true;
                }
            }
        }
        false
    }
}

impl Default for Frame {
    fn default() -> Self {
        let rect = rect((0, 0), (400, 300));
        Frame::new(rect, Dimension::Max, Dimension::Min)
    }
}