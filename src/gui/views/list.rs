use std::cell::RefCell;
use speedy2d::dimen::Vector2;
use speedy2d::font::{FormattedTextBlock, TextLayout, TextOptions};
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};
use gui::assets::get_font;
use gui::common::DEFAULT_TEXT_SIZE;
use gui::events::EventType;
use gui::themes::{Theme, Typeface, ViewState};
use gui::traits::{Element, View, WeakElement};
use gui::types::{Point, Rect, rect};
use gui::ui::UI;
use gui::views::{Borders, Dimension, FieldsMain};

pub struct List {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<String>>,
    texts: RefCell<Vec<Option<FormattedTextBlock>>>,
    text_size: f32,
    scroll_y: RefCell<i32>,
    selected: RefCell<Option<usize>>
}

impl List {
    pub fn new(rect: Rect<i32>) -> List {
        List {
            state: RefCell::new(FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min)),
            items: RefCell::new(vec![]),
            texts: RefCell::new(vec![]),
            text_size: DEFAULT_TEXT_SIZE,
            scroll_y: RefCell::new(0),
            selected: RefCell::new(None)
        }
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.items = RefCell::new(items);
        self.texts.borrow_mut().clear();
        let typeface = self.state.borrow().typeface.clone().unwrap();
        let scale = self.state.borrow().scale as f32;
        for i in self.items.borrow().iter() {
            if let Some(font) = get_font(&typeface.font_name, &typeface.font_style.to_string()) {
                let options = TextOptions::new();
                let text = font.layout_text(&i, self.text_size * scale, options);
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
            "margin_left" => { self.state.borrow_mut().margin.left = value.parse().unwrap_or(0) }
            "margin_right" => { self.state.borrow_mut().margin.right = value.parse().unwrap_or(0) }
            "margin_top" => { self.state.borrow_mut().margin.top = value.parse().unwrap_or(0) }
            "margin_bottom" => { self.state.borrow_mut().margin.bottom = value.parse().unwrap_or(0) }
            "id" => { self.set_id(value) }
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
                    Some(parent) => { Some(parent) }
                }
            }
        }
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.state.borrow_mut().typeface = Some(typeface.clone());
        self.state.borrow_mut().scale = scale;
        let (width, height) = {
            let state = self.state.borrow_mut();
            let ww;
            let hh;
            match &state.width {
                Dimension::Min => ww = 0,
                Dimension::Max => ww = width,
                Dimension::Dip(dip) => ww = *dip as i32,
                Dimension::Percent(p) => ww = (width as f32 * p / 100f32).round() as i32
            }
            match &state.height {
                Dimension::Min => hh = 0,
                Dimension::Max => hh = height,
                Dimension::Dip(dip) => hh = *dip as i32,
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
        theme.draw_list_back(rect, state);
        //let color = theme.get_text_color(self.state.borrow().state, &self.state.borrow().foreground);
        let mut y = rect.min.y;
        let mut index = 0usize;
        let selected = *self.selected.borrow();
        let scroll_y = *self.scroll_y.borrow();
        for v in self.texts.borrow().iter() {
            if let Some(text) = v {
                let text_height = text.height().ceil() as i32;
                let mut text_color: u32 = 0xff000000;
                if let Some(s) = selected {
                    if s == index {
                        let rect = super::super::types::rect((rect.min.x + 2, (y + scroll_y)), (rect.max.x - 2, (y + scroll_y) + text_height));
                        theme.draw_rect(rect, 0xff0000C0);
                        text_color = 0xffffffff;
                    }
                }

                theme.draw_text((rect.min.x + 10) as f32, (y + scroll_y) as f32, text_color, text);
                y += text_height;
            }
            index += 1;
        }
        theme.draw_list_body(rect, state);
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
        (100, 200)
    }

    fn is_focused(&self) -> bool {
        self.state.borrow().state.focused
    }

    fn is_break(&self) -> bool {
        self.state.borrow().break_line
    }

    fn set_focused(&self, focused: bool) {
        self.state.borrow_mut().state.focused = focused;
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

    fn set_id(&mut self, id: &str) {
        self.state.borrow_mut().id = id.to_owned();
    }

    fn get_id(&self) -> String {
        self.state.borrow().id.clone()
    }

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        todo!()
    }

    fn click(&self, _ui: &mut UI) -> bool {
        todo!()
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
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