use std::cell::RefCell;
use speedy2d::dimen::Vector2;
use speedy2d::window::MouseButton;
use super::super::events::EventType;
use super::super::themes::{Theme, Typeface, ViewState};
use super::super::traits::{Element, View, WeakElement};
use super::super::types::{Point, Rect, rect};
use super::super::ui::UI;
use super::super::views::{Borders, Dimension, FieldsMain};
use super::super::view_base::{HasMainFields, ViewBasics};

pub trait ListItem {
    fn get_view(&self) -> Element;
}

pub struct ListView {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<Box<dyn ListItem>>>,
    views: RefCell<Vec<Element>>,
    items_focusable: bool,
    scroll_y: i32,
    selected: RefCell<Option<usize>>
}

impl HasMainFields for ListView {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ListView {}

impl ListView {
    pub fn new(rect: Rect<i32>) -> ListView {
        ListView {
            state: RefCell::new(FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min)),
            items: RefCell::new(vec![]),
            views: RefCell::new(vec![]),
            items_focusable: true,
            scroll_y: 0,
            selected: RefCell::new(None)
        }
    }

    pub fn set_items(&mut self, items: Vec<Box<dyn ListItem>>) {
        self.items = RefCell::new(items); //TODO don't hold two copies of entities (items & views)
        self.views.borrow_mut().clear();
        let mut y = 0;
        let width = self.get_rect().width();
        let max_height = 20000; //TODO make it infinite
        let typeface = self.state.borrow().font_manager.get().unwrap();
        let scale = self.state.borrow().scale;
        for i in self.items.borrow().iter() {
            let view = i.get_view();
            view.borrow_mut().set_focusable(self.items_focusable);
            view.borrow_mut().layout_content(0, y, width, max_height, &typeface, scale);
            y += view.borrow().get_rect().height();
            //TODO set view parent
            self.views.borrow_mut().push(view);
        }
    }

    fn get_hit_item(&self, x: i32, y: i32) -> Option<usize> {
        let mut index = 0;
        for v in self.views.borrow().iter() {
            let mut rect = v.borrow().get_rect();
            rect.move_by((0, self.scroll_y));
            if rect.hit((x, y)) {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    pub fn select_item(&self, index: usize) -> bool {
        if index > self.views.borrow().len() {
            return false;
        }
        if let Some(selected) = *self.selected.borrow() {
            self.views.borrow_mut()[selected].borrow_mut().set_focused(false);
        }
        self.views.borrow_mut()[index].borrow_mut().set_focused(true);
        *self.selected.borrow_mut() = Some(index);
        true
    }
}

impl View for ListView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        // No ListView-specific properties
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
        let start = rect.min + origin;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        theme.draw_list_back(rect, self.get_state().unwrap());
        theme.draw_list_body(rect, self.get_state().unwrap());
        for v in self.views.borrow().iter() {
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
        (100, 200)
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
            let mut state = self.state.borrow_mut();
            if matches!(button, MouseButton::Left) {
                state.state.pressed = true;
            }
            state.state.focused = true;
            let rect = state.rect;
            if let Some(index) = self.get_hit_item(position.x - rect.min.x, position.y - rect.min.y) {
                self.select_item(index);
                println!("Selected item {:?}", *self.selected.borrow());
            }
            return true;
        }
        false
    }
}

impl Default for ListView {
    fn default() -> Self {
        let rect = rect((0, 0), (100, 200));
        ListView::new(rect)
    }
}