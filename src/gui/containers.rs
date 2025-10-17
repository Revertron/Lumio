use std::cell::RefCell;
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};
use super::events::EventType;
use super::views::Borders;

use super::themes::{FontStyle, Theme, Typeface, ViewState};
use super::traits::{Container, Element, View, WeakElement};
use super::types::{Point, Rect, rect};
use super::ui::UI;
use super::views::{Dimension, Direction, FieldsMain};

pub struct Frame {
    state: RefCell<FieldsMain>,
    direction: Direction,
    views: Vec<Element>,
    breaking: bool
}

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
        let typeface = match self.state.borrow_mut().typeface.take() {
            None => Typeface { font_name: font_name.to_owned(), font_style: FontStyle::Regular },
            Some(mut t) => {
                t.font_name = font_name.to_owned();
                t
            }
        };
        self.state.borrow_mut().typeface = Some(typeface)
    }

    fn set_font_style(&mut self, style: &str) {
        let font_style = FontStyle::from(style);
        let typeface = match self.state.borrow_mut().typeface.take() {
            None => Typeface { font_name: String::new(), font_style },
            Some(t) => Typeface { font_name: t.font_name, font_style },
        };
        self.state.borrow_mut().typeface = Some(typeface)
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
        println!("Searching {} in Frame {}", &id, &self.get_id());
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
}

impl View for Frame {
    fn set_any(&mut self, name: &str, value: &str) {
        match name {
            "left" => { self.set_x(value.parse().unwrap()) }
            "top" => { self.set_y(value.parse().unwrap()) }
            "width" => { self.set_width(value.parse().unwrap()) }
            "height" => { self.set_height(value.parse().unwrap()) }
            "padding" => { self.state.borrow_mut().padding.set_all(value.parse().unwrap_or(0)) }
            "padding_top" => { self.state.borrow_mut().padding.top = value.parse().unwrap_or(0) }
            "padding_left" => { self.state.borrow_mut().padding.left = value.parse().unwrap_or(0) }
            "padding_right" => { self.state.borrow_mut().padding.right = value.parse().unwrap_or(0) }
            "padding_bottom" => { self.state.borrow_mut().padding.bottom = value.parse().unwrap_or(0) }
            "margin" => { self.state.borrow_mut().margin.set_all(value.parse().unwrap_or(0)) }
            "margin_top" => { self.state.borrow_mut().margin.top = value.parse().unwrap_or(0) }
            "margin_left" => { self.state.borrow_mut().margin.left = value.parse().unwrap_or(0) }
            "margin_right" => { self.state.borrow_mut().margin.right = value.parse().unwrap_or(0) }
            "margin_bottom" => { self.state.borrow_mut().margin.bottom = value.parse().unwrap_or(0) }
            "direction" => { self.set_direction(value.parse().unwrap()) }
            "id" => { self.set_id(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "breaking" => { self.breaking = value.parse().unwrap_or(false) }
            "break" => { self.state.borrow_mut().break_line = value.parse().unwrap_or(false) }
            &_ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.state.borrow_mut().parent = parent;
    }

    fn get_parent(&self) -> Option<Element> {
        match &self.state.borrow().parent {
            None => { None }
            Some(weak) => {
                match weak.upgrade() {
                    None => { None }
                    Some(rc) => { Some(rc) }
                }
            }
        }
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.state.borrow_mut().scale = scale;
        //println!("Laying out for {},{} - {},{}", x, y, width, height);
        let (new_width, new_height) = self.calculate_size(width, height, scale);
        //println!("New width {}, new height {}", new_width, new_height);

        let padding = self.get_padding(scale);
        let mut xx = padding.left;
        let mut yy = padding.top;
        let max_x = new_width - padding.right;
        let mut max_height = 0;
        let typeface = match self.state.borrow().typeface.clone() {
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
                Dimension::Dip(dip) => ww = *dip as i32,
                Dimension::Percent(p) => ww = (width as f32 * p / 100f32).round() as i32
            }
            match &state.height {
                Dimension::Min => hh = h,
                Dimension::Max => hh = new_height,
                Dimension::Dip(dip) => hh = *dip as i32,
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
        theme.draw_panel_back(rect, self.state.borrow().state);
        theme.draw_panel_body(rect, self.state.borrow().state);
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
        self.state.borrow().rect
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.state.borrow_mut().rect = rect;
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.state.borrow().padding.scaled(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut state = self.state.borrow_mut();
        state.padding.top = top;
        state.padding.left = left;
        state.padding.right = right;
        state.padding.bottom = bottom;
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.state.borrow().margin.scaled(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut state = self.state.borrow_mut();
        state.margin.top = top;
        state.margin.left = left;
        state.margin.right = right;
        state.margin.bottom = bottom;
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        let state = self.state.borrow();
        (state.width, state.height)
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
        self.state.borrow().break_line
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
        self.state.borrow_mut().state.focusable = focusable;
    }

    fn set_width(&mut self, width: Dimension) {
        self.state.borrow_mut().width = width;
    }

    fn set_height(&mut self, height: Dimension) {
        self.state.borrow_mut().height = height;
    }

    fn set_scale(&mut self, scale: f64) {
        self.state.borrow_mut().scale = scale;
    }

    fn set_id(&mut self, id: &str) {
        self.state.borrow_mut().id = id.to_owned();
    }

    fn get_id(&self) -> String {
        self.state.borrow().id.clone()
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