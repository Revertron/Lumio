use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::str::FromStr;

use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};

use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Container, Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, Direction, FieldsMain};

const DEFAULT_DIVIDER_SIZE: i32 = 4;
const DEFAULT_MIN_PANE_SIZE: i32 = 50;

#[derive(Copy, Clone, Debug)]
enum SplitPos {
    Dip(u32),
    Percent(f32),
}

impl Default for SplitPos {
    fn default() -> Self {
        SplitPos::Percent(50.0)
    }
}

impl FromStr for SplitPos {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(stripped) = s.strip_suffix('%') {
            let float = stripped.parse::<f32>().map_err(|_| ())?;
            Ok(SplitPos::Percent(float))
        } else {
            let int = s.parse::<u32>().map_err(|_| ())?;
            Ok(SplitPos::Dip(int))
        }
    }
}

pub struct SplitPanel {
    state: RefCell<FieldsMain>,
    direction: Direction,
    views: Vec<Element>,
    split_pos: Cell<SplitPos>,
    split_pos_px: Cell<i32>,
    divider_size: Cell<i32>,
    divider_dragging: Cell<bool>,
    drag_start_mouse: Cell<i32>,
    drag_start_split: Cell<i32>,
    divider_hovered: Cell<bool>,
    min_pane_size: Cell<i32>,
    needs_relayout: Cell<bool>,
    last_typeface: RefCell<Option<Typeface>>,
    last_layout: Cell<(i32, i32, i32, i32)>,
}

impl HasMainFields for SplitPanel {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for SplitPanel {}

impl SplitPanel {
    fn set_font(&mut self, font_name: &str) {
        self.state.borrow_mut().font_manager.set_font(font_name);
    }

    fn set_font_style(&mut self, style: &str) {
        self.state.borrow_mut().font_manager.set_font_style(style);
    }

    /// Get the divider rect in local coordinates (relative to the panel's own rect)
    fn divider_rect_local(&self) -> Rect<i32> {
        let my_rect = self.state.borrow().rect;
        let split = self.split_pos_px.get();
        let scale = self.state.borrow().scale;
        let div_size = (self.divider_size.get() as f64 * scale).round() as i32;

        match self.direction {
            Direction::Horizontal => {
                rect(
                    (split, 0),
                    (split + div_size, my_rect.height()),
                )
            }
            Direction::Vertical => {
                rect(
                    (0, split),
                    (my_rect.width(), split + div_size),
                )
            }
        }
    }

    fn relayout_children(&mut self) {
        let (_x, _y, width, height) = self.last_layout.get();
        let scale = self.state.borrow().scale;
        let typeface = self.last_typeface.borrow().clone().unwrap_or_default();

        let div_size = (self.divider_size.get() as f64 * scale).round() as i32;
        let split = self.split_pos_px.get();

        if !self.views.is_empty() {
            let mut v = self.views[0].try_borrow_mut().unwrap();
            match self.direction {
                Direction::Horizontal => {
                    v.layout_content(0, 0, split, height, &typeface, scale);
                }
                Direction::Vertical => {
                    v.layout_content(0, 0, width, split, &typeface, scale);
                }
            }
        }

        if self.views.len() >= 2 {
            let mut v = self.views[1].try_borrow_mut().unwrap();
            match self.direction {
                Direction::Horizontal => {
                    let second_x = split + div_size;
                    let second_w = width - second_x;
                    v.layout_content(second_x, 0, second_w, height, &typeface, scale);
                }
                Direction::Vertical => {
                    let second_y = split + div_size;
                    let second_h = height - second_y;
                    v.layout_content(0, second_y, width, second_h, &typeface, scale);
                }
            }
        }
    }
}

impl Container for SplitPanel {
    fn add_view(&mut self, view: Element) {
        if self.views.len() < 2 {
            self.views.push(view);
        }
    }

    fn get_view(&self, id: &str) -> Option<Element> {
        if let Some(found) = self.views.iter().find(|v| v.borrow().get_id() == id) {
            return Some(Rc::clone(found));
        }
        for v in self.views.iter() {
            if let Some(container) = v.borrow().as_container() {
                if let Some(found) = container.get_view(id) {
                    return Some(found);
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

impl View for SplitPanel {
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
            "split_pos" => {
                if let Ok(sp) = SplitPos::from_str(value) {
                    self.split_pos.set(sp);
                }
            }
            "divider_size" => {
                if let Ok(s) = value.parse::<i32>() {
                    self.divider_size.set(s);
                }
            }
            "min_pane_size" => {
                if let Ok(s) = value.parse::<i32>() {
                    self.min_pane_size.set(s);
                }
            }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            _ => {}
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
        let (new_width, new_height) = self.calculate_size(width, height, scale);

        let typeface = match self.state.borrow().font_manager.get() {
            None => typeface.clone(),
            Some(t) => t,
        };

        let div_size = (self.divider_size.get() as f64 * scale).round() as i32;
        let min_pane = (self.min_pane_size.get() as f64 * scale).round() as i32;

        // Resolve split_pos to pixels
        let total = match self.direction {
            Direction::Horizontal => new_width,
            Direction::Vertical => new_height,
        };
        let available = total - div_size;

        let split = match self.split_pos.get() {
            SplitPos::Dip(dip) => (dip as f64 * scale).round() as i32,
            SplitPos::Percent(pct) => (available as f32 * pct / 100.0).round() as i32,
        };
        let split = split.max(min_pane).min(available - min_pane);
        self.split_pos_px.set(split);

        // Store for relayout during drag
        *self.last_typeface.borrow_mut() = Some(typeface.clone());
        self.last_layout.set((x, y, new_width, new_height));

        // Layout first child
        if !self.views.is_empty() {
            let mut v = self.views[0].try_borrow_mut().unwrap();
            match self.direction {
                Direction::Horizontal => {
                    v.layout_content(0, 0, split, new_height, &typeface, scale);
                }
                Direction::Vertical => {
                    v.layout_content(0, 0, new_width, split, &typeface, scale);
                }
            }
        }

        // Layout second child
        if self.views.len() >= 2 {
            let mut v = self.views[1].try_borrow_mut().unwrap();
            match self.direction {
                Direction::Horizontal => {
                    let second_x = split + div_size;
                    let second_w = new_width - second_x;
                    v.layout_content(second_x, 0, second_w, new_height, &typeface, scale);
                }
                Direction::Vertical => {
                    let second_y = split + div_size;
                    let second_h = new_height - second_y;
                    v.layout_content(0, second_y, new_width, second_h, &typeface, scale);
                }
            }
        }

        let r = rect((x, y), (x + new_width, y + new_height));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let r = self.state.borrow().rect;
        r.width() <= width && r.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let mut my_rect = self.state.borrow().rect;
        let start = my_rect.min + origin;
        my_rect.move_by(origin);

        theme.push_clip();
        theme.clip_rect(my_rect);

        // Draw background
        let state = self.state.borrow();
        if let Some(bg) = state.background.as_ref() {
            if let Some(crate::styles::selector::DrawState::Color(c)) = bg.get_state(&state.state) {
                theme.draw_rect(my_rect, *c);
            } else {
                theme.draw_panel_back(my_rect, state.state);
            }
        } else {
            theme.draw_panel_back(my_rect, state.state);
        }
        if let Some(border_color) = state.border_color {
            let r = my_rect;
            theme.draw_rect(rect((r.min.x, r.min.y), (r.max.x, r.min.y + 1)), border_color);
            theme.draw_rect(rect((r.min.x, r.max.y - 1), (r.max.x, r.max.y)), border_color);
            theme.draw_rect(rect((r.min.x, r.min.y), (r.min.x + 1, r.max.y)), border_color);
            theme.draw_rect(rect((r.max.x - 1, r.min.y), (r.max.x, r.max.y)), border_color);
        }
        drop(state);

        // Paint first child
        if !self.views.is_empty() {
            let v = self.views[0].try_borrow().unwrap();
            theme.push_clip();
            let split = self.split_pos_px.get();
            match self.direction {
                Direction::Horizontal => {
                    theme.clip_rect(rect(
                        (start.x, start.y),
                        (start.x + split, start.y + my_rect.height()),
                    ));
                }
                Direction::Vertical => {
                    theme.clip_rect(rect(
                        (start.x, start.y),
                        (start.x + my_rect.width(), start.y + split),
                    ));
                }
            }
            v.paint(start, theme);
            theme.pop_clip();
        }

        // Paint second child
        if self.views.len() >= 2 {
            let v = self.views[1].try_borrow().unwrap();
            let scale = self.state.borrow().scale;
            let div_size = (self.divider_size.get() as f64 * scale).round() as i32;
            let split = self.split_pos_px.get();
            theme.push_clip();
            match self.direction {
                Direction::Horizontal => {
                    theme.clip_rect(rect(
                        (start.x + split + div_size, start.y),
                        (start.x + my_rect.width(), start.y + my_rect.height()),
                    ));
                }
                Direction::Vertical => {
                    theme.clip_rect(rect(
                        (start.x, start.y + split + div_size),
                        (start.x + my_rect.width(), start.y + my_rect.height()),
                    ));
                }
            }
            v.paint(start, theme);
            theme.pop_clip();
        }

        // Draw divider
        let div_local = self.divider_rect_local();
        let div_rect = rect(
            (div_local.min.x + start.x, div_local.min.y + start.y),
            (div_local.max.x + start.x, div_local.max.y + start.y),
        );
        let view_state = self.state.borrow().state;
        theme.draw_separator(div_rect, view_state);

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
        let mut w = 0i32;
        let mut h = 0i32;
        for v in self.views.iter() {
            let v = v.borrow();
            let vr = v.get_rect();
            w = w.max(vr.max.x);
            h = h.max(vr.max.y);
        }
        (w, h)
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
        // No events for SplitPanel itself
    }

    fn click(&self, _ui: &mut UI) -> bool {
        false
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        let mut redraw = false;

        if self.needs_relayout.get() {
            self.needs_relayout.set(false);
            self.relayout_children();
            redraw = true;
        }

        for v in self.views.iter() {
            redraw |= v.borrow_mut().update(ui);
        }

        redraw
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        let local = Vector2::new(
            position.x - self.state.borrow().rect.min.x,
            position.y - self.state.borrow().rect.min.y,
        );

        if self.divider_dragging.get() {
            let mouse_pos = match self.direction {
                Direction::Horizontal => local.x,
                Direction::Vertical => local.y,
            };
            let delta = mouse_pos - self.drag_start_mouse.get();
            let new_split = self.drag_start_split.get() + delta;

            let scale = self.state.borrow().scale;
            let my_rect = self.state.borrow().rect;
            let div_size = (self.divider_size.get() as f64 * scale).round() as i32;
            let min_pane = (self.min_pane_size.get() as f64 * scale).round() as i32;
            let total = match self.direction {
                Direction::Horizontal => my_rect.width(),
                Direction::Vertical => my_rect.height(),
            };
            let available = total - div_size;
            let clamped = new_split.max(min_pane).min(available - min_pane);

            if clamped != self.split_pos_px.get() {
                self.split_pos_px.set(clamped);
                // Update split_pos so window resize preserves the dragged position
                let unscaled = (clamped as f64 / scale).round() as u32;
                self.split_pos.set(SplitPos::Dip(unscaled));
                self.needs_relayout.set(true);
            }
            return true;
        }

        // Hit-test divider for hover state
        let div_rect = self.divider_rect_local();
        let hit = div_rect.hit((local.x, local.y));
        self.divider_hovered.set(hit);

        // Forward to children
        let mut processed = false;
        for v in self.views.iter().rev() {
            processed |= v.borrow().on_mouse_move(ui, local);
        }
        processed
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let local = Vector2::new(
            position.x - self.state.borrow().rect.min.x,
            position.y - self.state.borrow().rect.min.y,
        );

        // Hit-test divider
        let div_rect = self.divider_rect_local();
        if div_rect.hit((local.x, local.y)) {
            self.divider_dragging.set(true);
            let mouse_pos = match self.direction {
                Direction::Horizontal => local.x,
                Direction::Vertical => local.y,
            };
            self.drag_start_mouse.set(mouse_pos);
            self.drag_start_split.set(self.split_pos_px.get());
            return true;
        }

        // Forward to children
        for v in self.views.iter().rev() {
            if v.borrow().on_mouse_button_down(ui, local, button) {
                return true;
            }
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if self.divider_dragging.get() {
            self.divider_dragging.set(false);
            return true;
        }

        let local = Vector2::new(
            position.x - self.state.borrow().rect.min.x,
            position.y - self.state.borrow().rect.min.y,
        );
        for v in self.views.iter().rev() {
            if v.borrow().on_mouse_button_up(ui, local, button) {
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Vector2<i32>, distance: speedy2d::window::MouseScrollDistance) -> bool {
        let local = Vector2::new(
            position.x - self.state.borrow().rect.min.x,
            position.y - self.state.borrow().rect.min.y,
        );
        for v in self.views.iter().rev() {
            if v.borrow().on_mouse_wheel_scroll(ui, local, distance) {
                return true;
            }
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused()
                && v.borrow().on_key_down(ui, virtual_key_code, scancode, state.clone()) {
                return true;
            }
        }
        false
    }

    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused()
                && v.borrow().on_key_up(ui, virtual_key_code, scancode, state.clone()) {
                return true;
            }
        }
        false
    }

    fn on_key_char(&self, ui: &mut UI, unicode_codepoint: char, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            if v.borrow().is_focused()
                && v.borrow().on_key_char(ui, unicode_codepoint, state.clone()) {
                return true;
            }
        }
        false
    }
}

impl Default for SplitPanel {
    fn default() -> Self {
        let r = rect((0, 0), (400, 300));
        let mut main = FieldsMain::with_rect(r, Dimension::Max, Dimension::Max);
        main.state.focusable = false;
        SplitPanel {
            state: RefCell::new(main),
            direction: Direction::Horizontal,
            views: Vec::new(),
            split_pos: Cell::new(SplitPos::default()),
            split_pos_px: Cell::new(0),
            divider_size: Cell::new(DEFAULT_DIVIDER_SIZE),
            divider_dragging: Cell::new(false),
            drag_start_mouse: Cell::new(0),
            drag_start_split: Cell::new(0),
            divider_hovered: Cell::new(false),
            min_pane_size: Cell::new(DEFAULT_MIN_PANE_SIZE),
            needs_relayout: Cell::new(false),
            last_typeface: RefCell::new(None),
            last_layout: Cell::new((0, 0, 0, 0)),
        }
    }
}
