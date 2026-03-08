use std::cell::{Cell, RefCell};
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::view_base::{HasMainFields, ViewBasics};
use crate::traits::{Container, Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Direction, FieldsMain};

const SCROLLBAR_WIDTH: i32 = 16;
const SCROLL_LINE: i32 = 20;
const MIN_THUMB_SIZE: i32 = 16;

pub struct ScrollView {
    state: RefCell<FieldsMain>,
    direction: Direction,
    child: RefCell<Option<Element>>,
    scroll_offset: Cell<i32>,
    content_size: Cell<i32>,
    thumb_dragging: Cell<bool>,
    drag_start_offset: Cell<i32>,
    drag_start_mouse: Cell<i32>,
    arrow_start_pressed: Cell<bool>,
    arrow_end_pressed: Cell<bool>,
    thumb_hovered: Cell<bool>,
}

impl HasMainFields for ScrollView {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ScrollView {}

impl ScrollView {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> ScrollView {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.state.focusable = false;
        ScrollView {
            state: RefCell::new(main),
            direction: Direction::Vertical,
            child: RefCell::new(None),
            scroll_offset: Cell::new(0),
            content_size: Cell::new(0),
            thumb_dragging: Cell::new(false),
            drag_start_offset: Cell::new(0),
            drag_start_mouse: Cell::new(0),
            arrow_start_pressed: Cell::new(false),
            arrow_end_pressed: Cell::new(false),
            thumb_hovered: Cell::new(false),
        }
    }

    pub fn get_scroll_offset(&self) -> i32 {
        self.scroll_offset.get()
    }

    pub fn set_scroll_offset(&self, offset: i32) {
        self.scroll_offset.set(offset);
        self.clamp_scroll();
    }

    pub fn scroll_to_start(&self) {
        self.scroll_offset.set(0);
    }

    pub fn scroll_to_end(&self) {
        self.scroll_offset.set(self.max_scroll());
    }

    fn scaled_scrollbar_width(&self, scale: f64) -> i32 {
        (SCROLLBAR_WIDTH as f64 * scale).round() as i32
    }

    fn needs_scrollbar(&self) -> bool {
        let r = self.state.borrow().rect;
        let vp = match self.direction {
            Direction::Vertical => r.height(),
            Direction::Horizontal => r.width(),
        };
        self.content_size.get() > vp
    }

    fn max_scroll(&self) -> i32 {
        let r = self.state.borrow().rect;
        let vp = match self.direction {
            Direction::Vertical => r.height(),
            Direction::Horizontal => r.width(),
        };
        -(self.content_size.get() - vp).max(0)
    }

    fn clamp_scroll(&self) {
        let offset = self.scroll_offset.get();
        let max = self.max_scroll();
        self.scroll_offset.set(offset.min(0).max(max));
    }

    /// Returns scrollbar rect in local coordinates (relative to own rect origin)
    fn scrollbar_rect(&self) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let w = r.width();
        let h = r.height();
        let scale = self.state.borrow().scale;
        let sb = self.scaled_scrollbar_width(scale);
        match self.direction {
            Direction::Vertical => rect((w - sb, 0), (w, h)),
            Direction::Horizontal => rect((0, h - sb), (w, h)),
        }
    }

    fn arrow_start_rect(&self) -> Rect<i32> {
        let sb = self.scrollbar_rect();
        let scale = self.state.borrow().scale;
        let size = self.scaled_scrollbar_width(scale);
        match self.direction {
            Direction::Vertical => rect((sb.min.x, sb.min.y), (sb.max.x, sb.min.y + size)),
            Direction::Horizontal => rect((sb.min.x, sb.min.y), (sb.min.x + size, sb.max.y)),
        }
    }

    fn arrow_end_rect(&self) -> Rect<i32> {
        let sb = self.scrollbar_rect();
        let scale = self.state.borrow().scale;
        let size = self.scaled_scrollbar_width(scale);
        match self.direction {
            Direction::Vertical => rect((sb.min.x, sb.max.y - size), (sb.max.x, sb.max.y)),
            Direction::Horizontal => rect((sb.max.x - size, sb.min.y), (sb.max.x, sb.max.y)),
        }
    }

    fn track_rect(&self) -> Rect<i32> {
        let sb = self.scrollbar_rect();
        let scale = self.state.borrow().scale;
        let size = self.scaled_scrollbar_width(scale);
        match self.direction {
            Direction::Vertical => rect((sb.min.x, sb.min.y + size), (sb.max.x, sb.max.y - size)),
            Direction::Horizontal => rect((sb.min.x + size, sb.min.y), (sb.max.x - size, sb.max.y)),
        }
    }

    fn thumb_rect(&self) -> Rect<i32> {
        let track = self.track_rect();
        let scale = self.state.borrow().scale;
        let min_thumb = (MIN_THUMB_SIZE as f64 * scale).round() as i32;
        let content = self.content_size.get();
        if content <= 0 {
            return track;
        }

        let track_len = match self.direction {
            Direction::Vertical => track.height(),
            Direction::Horizontal => track.width(),
        };

        let r = self.state.borrow().rect;
        let vp = match self.direction {
            Direction::Vertical => r.height(),
            Direction::Horizontal => r.width(),
        };

        let thumb_len = ((vp as f64 / content as f64) * track_len as f64).round() as i32;
        let thumb_len = thumb_len.max(min_thumb).min(track_len);

        let scroll_range = content - vp;
        let thumb_range = track_len - thumb_len;
        let offset = self.scroll_offset.get(); // negative or 0
        let thumb_pos = if scroll_range > 0 {
            ((-offset as f64 / scroll_range as f64) * thumb_range as f64).round() as i32
        } else {
            0
        };

        match self.direction {
            Direction::Vertical => rect(
                (track.min.x, track.min.y + thumb_pos),
                (track.max.x, track.min.y + thumb_pos + thumb_len),
            ),
            Direction::Horizontal => rect(
                (track.min.x + thumb_pos, track.min.y),
                (track.min.x + thumb_pos + thumb_len, track.max.y),
            ),
        }
    }

    /// Viewport rect in local coordinates
    fn viewport_rect(&self) -> Rect<i32> {
        let r = self.state.borrow().rect;
        let w = r.width();
        let h = r.height();
        let scale = self.state.borrow().scale;
        let sb = self.scaled_scrollbar_width(scale);
        if self.needs_scrollbar() {
            match self.direction {
                Direction::Vertical => rect((0, 0), (w - sb, h)),
                Direction::Horizontal => rect((0, 0), (w, h - sb)),
            }
        } else {
            rect((0, 0), (w, h))
        }
    }

    fn scroll_by(&self, delta: i32, scale: f64) {
        let scaled_delta = (delta as f64 * scale).round() as i32;
        let offset = self.scroll_offset.get() + scaled_delta;
        self.scroll_offset.set(offset);
        self.clamp_scroll();
    }

    fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }
}

impl Container for ScrollView {
    fn add_view(&mut self, view: Element) {
        self.child.replace(Some(view));
    }

    fn get_view(&self, id: &str) -> Option<Element> {
        if let Some(child) = &*self.child.borrow() {
            if child.borrow().get_id() == id {
                return Some(Rc::clone(child));
            }
            if let Some(container) = child.borrow().as_container() {
                return container.get_view(id);
            }
        }
        None
    }

    fn get_view_count(&self) -> usize {
        if self.child.borrow().is_some() { 1 } else { 0 }
    }

    fn get_views(&self) -> Vec<Element> {
        match &*self.child.borrow() {
            Some(child) => vec![Rc::clone(child)],
            None => Vec::new(),
        }
    }
}

impl View for ScrollView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        if name == "direction" {
            self.set_direction(value.parse().unwrap());
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

        // Set own rect first so viewport calculations work
        let my_rect = rect((x, y), (x + new_width, y + new_height));
        self.set_rect(my_rect);

        let typeface = match self.state.borrow().font_manager.get() {
            None => typeface.clone(),
            Some(t) => t,
        };

        if let Some(child) = &*self.child.borrow() {
            let mut child = child.borrow_mut();
            // Give the child unlimited space in the scroll direction
            let sb = self.scaled_scrollbar_width(scale);
            match self.direction {
                Direction::Vertical => {
                    // First pass: layout with full width (no scrollbar) and unlimited height
                    child.layout_content(0, 0, new_width, i32::MAX / 2, &typeface, scale);
                    let (_, ch) = child.calculate_full_size(scale);
                    self.content_size.set(ch);

                    // If scrollbar needed, relayout with reduced width
                    if ch > new_height {
                        child.layout_content(0, 0, new_width - sb, i32::MAX / 2, &typeface, scale);
                        let (_, ch) = child.calculate_full_size(scale);
                        self.content_size.set(ch);
                    }
                }
                Direction::Horizontal => {
                    child.layout_content(0, 0, i32::MAX / 2, new_height, &typeface, scale);
                    let (cw, _) = child.calculate_full_size(scale);
                    self.content_size.set(cw);

                    if cw > new_width {
                        child.layout_content(0, 0, i32::MAX / 2, new_height - sb, &typeface, scale);
                        let (cw, _) = child.calculate_full_size(scale);
                        self.content_size.set(cw);
                    }
                }
            }
        }

        self.clamp_scroll();
        my_rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, scale: f64) -> bool {
        let size = self.calculate_full_size(scale);
        size.0 <= width && size.1 <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let my_rect = self.state.borrow().rect;
        let abs_origin = Point::from((origin.x + my_rect.min.x, origin.y + my_rect.min.y));
        let scale = self.state.borrow().scale;

        // Paint viewport content with clipping
        if let Some(child) = &*self.child.borrow() {
            let vp = self.viewport_rect();
            let clip = rect(
                (abs_origin.x + vp.min.x, abs_origin.y + vp.min.y),
                (abs_origin.x + vp.max.x, abs_origin.y + vp.max.y),
            );
            theme.push_clip();
            theme.clip_rect(clip);

            let child_origin = match self.direction {
                Direction::Vertical => Point::from((abs_origin.x, abs_origin.y + self.scroll_offset.get())),
                Direction::Horizontal => Point::from((abs_origin.x + self.scroll_offset.get(), abs_origin.y)),
            };
            child.borrow().paint(child_origin, theme);

            theme.pop_clip();
        }

        // Paint scrollbar if needed
        if self.needs_scrollbar() {
            self.paint_scrollbar(abs_origin, theme, scale);
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
        let r = self.state.borrow().rect;
        (r.width(), r.height())
    }

    fn is_focused(&self) -> bool {
        if let Some(child) = &*self.child.borrow() {
            if child.borrow().is_focused() {
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
        if let Some(child) = &*self.child.borrow() {
            child.borrow().set_focused(false);
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
        false
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        if let Some(child) = &*self.child.borrow() {
            if child.borrow_mut().update(ui) {
                return true;
            }
        }
        false
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        let my_rect = self.state.borrow().rect;
        let local = Vector2::new(position.x - my_rect.min.x, position.y - my_rect.min.y);

        // Handle thumb dragging
        if self.thumb_dragging.get() {
            let mouse_pos = match self.direction {
                Direction::Vertical => local.y,
                Direction::Horizontal => local.x,
            };
            let delta_mouse = mouse_pos - self.drag_start_mouse.get();
            let track = self.track_rect();
            let track_len = match self.direction {
                Direction::Vertical => track.height(),
                Direction::Horizontal => track.width(),
            };
            let r = self.state.borrow().rect;
            let vp = match self.direction {
                Direction::Vertical => r.height(),
                Direction::Horizontal => r.width(),
            };
            let content = self.content_size.get();
            let thumb_rect = self.thumb_rect();
            let thumb_len = match self.direction {
                Direction::Vertical => thumb_rect.height(),
                Direction::Horizontal => thumb_rect.width(),
            };
            let thumb_range = track_len - thumb_len;
            if thumb_range > 0 {
                let scroll_range = content - vp;
                let new_offset = self.drag_start_offset.get() - (delta_mouse as f64 * scroll_range as f64 / thumb_range as f64).round() as i32;
                self.scroll_offset.set(new_offset);
                self.clamp_scroll();
            }
            return true;
        }

        // Check thumb hover state
        if self.needs_scrollbar() {
            let thumb = self.thumb_rect();
            let was_hovered = self.thumb_hovered.get();
            self.thumb_hovered.set(thumb.hit((local.x, local.y)));
            if was_hovered != self.thumb_hovered.get() {
                return true;
            }
        }

        // Forward to child
        if let Some(child) = &*self.child.borrow() {
            let vp = self.viewport_rect();
            let child_pos = match self.direction {
                Direction::Vertical => Vector2::new(local.x - vp.min.x, local.y - vp.min.y - self.scroll_offset.get()),
                Direction::Horizontal => Vector2::new(local.x - vp.min.x - self.scroll_offset.get(), local.y - vp.min.y),
            };
            if child.borrow().on_mouse_move(ui, child_pos) {
                return true;
            }
        }
        false
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let my_rect = self.state.borrow().rect;
        let local = Vector2::new(position.x - my_rect.min.x, position.y - my_rect.min.y);

        // Check if click is within our bounds
        let bounds = rect((0, 0), (my_rect.width(), my_rect.height()));
        if !bounds.hit((local.x, local.y)) {
            return false;
        }

        if !matches!(button, MouseButton::Left) {
            // Forward non-left clicks to child
            if let Some(child) = &*self.child.borrow() {
                let vp = self.viewport_rect();
                let child_pos = match self.direction {
                    Direction::Vertical => Vector2::new(local.x - vp.min.x, local.y - vp.min.y - self.scroll_offset.get()),
                    Direction::Horizontal => Vector2::new(local.x - vp.min.x - self.scroll_offset.get(), local.y - vp.min.y),
                };
                return child.borrow().on_mouse_button_down(ui, child_pos, button);
            }
            return false;
        }

        let scale = self.state.borrow().scale;

        // Check scrollbar interactions
        if self.needs_scrollbar() {
            let sb = self.scrollbar_rect();
            if sb.hit((local.x, local.y)) {
                // Arrow start
                let arrow_start = self.arrow_start_rect();
                if arrow_start.hit((local.x, local.y)) {
                    self.arrow_start_pressed.set(true);
                    self.scroll_by(SCROLL_LINE, scale);
                    return true;
                }

                // Arrow end
                let arrow_end = self.arrow_end_rect();
                if arrow_end.hit((local.x, local.y)) {
                    self.arrow_end_pressed.set(true);
                    self.scroll_by(-SCROLL_LINE, scale);
                    return true;
                }

                // Thumb
                let thumb = self.thumb_rect();
                if thumb.hit((local.x, local.y)) {
                    self.thumb_dragging.set(true);
                    self.drag_start_offset.set(self.scroll_offset.get());
                    let mouse_pos = match self.direction {
                        Direction::Vertical => local.y,
                        Direction::Horizontal => local.x,
                    };
                    self.drag_start_mouse.set(mouse_pos);
                    return true;
                }

                // Track (page scroll)
                let track = self.track_rect();
                if track.hit((local.x, local.y)) {
                    let r = self.state.borrow().rect;
                    let vp_size = match self.direction {
                        Direction::Vertical => r.height(),
                        Direction::Horizontal => r.width(),
                    };
                    let click_pos = match self.direction {
                        Direction::Vertical => local.y,
                        Direction::Horizontal => local.x,
                    };
                    let thumb_start = match self.direction {
                        Direction::Vertical => thumb.min.y,
                        Direction::Horizontal => thumb.min.x,
                    };
                    if click_pos < thumb_start {
                        // Page scroll toward start
                        let offset = self.scroll_offset.get() + vp_size;
                        self.scroll_offset.set(offset);
                        self.clamp_scroll();
                    } else {
                        // Page scroll toward end
                        let offset = self.scroll_offset.get() - vp_size;
                        self.scroll_offset.set(offset);
                        self.clamp_scroll();
                    }
                    return true;
                }

                return true;
            }
        }

        // Forward to child
        if let Some(child) = &*self.child.borrow() {
            let vp = self.viewport_rect();
            if vp.hit((local.x, local.y)) {
                let child_pos = match self.direction {
                    Direction::Vertical => Vector2::new(local.x - vp.min.x, local.y - vp.min.y - self.scroll_offset.get()),
                    Direction::Horizontal => Vector2::new(local.x - vp.min.x - self.scroll_offset.get(), local.y - vp.min.y),
                };
                return child.borrow().on_mouse_button_down(ui, child_pos, button);
            }
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let my_rect = self.state.borrow().rect;
        let local = Vector2::new(position.x - my_rect.min.x, position.y - my_rect.min.y);

        let was_dragging = self.thumb_dragging.get();
        let was_arrow = self.arrow_start_pressed.get() || self.arrow_end_pressed.get();
        self.thumb_dragging.set(false);
        self.arrow_start_pressed.set(false);
        self.arrow_end_pressed.set(false);

        if was_dragging || was_arrow {
            return true;
        }

        // Forward to child
        if let Some(child) = &*self.child.borrow() {
            let vp = self.viewport_rect();
            let child_pos = match self.direction {
                Direction::Vertical => Vector2::new(local.x - vp.min.x, local.y - vp.min.y - self.scroll_offset.get()),
                Direction::Horizontal => Vector2::new(local.x - vp.min.x - self.scroll_offset.get(), local.y - vp.min.y),
            };
            if child.borrow().on_mouse_button_up(ui, child_pos, button) {
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Vector2<i32>, distance: MouseScrollDistance) -> bool {
        let my_rect = self.state.borrow().rect;
        let local = Vector2::new(position.x - my_rect.min.x, position.y - my_rect.min.y);
        let bounds = rect((0, 0), (my_rect.width(), my_rect.height()));
        if !bounds.hit((local.x, local.y)) {
            return false;
        }

        // First, let child handle it (e.g. nested ScrollView)
        if let Some(child) = &*self.child.borrow() {
            let vp = self.viewport_rect();
            let child_pos = match self.direction {
                Direction::Vertical => Vector2::new(local.x - vp.min.x, local.y - vp.min.y - self.scroll_offset.get()),
                Direction::Horizontal => Vector2::new(local.x - vp.min.x - self.scroll_offset.get(), local.y - vp.min.y),
            };
            if child.borrow().on_mouse_wheel_scroll(ui, child_pos, distance) {
                return true;
            }
        }

        if !self.needs_scrollbar() {
            return false;
        }

        let scale = self.state.borrow().scale;
        let r = self.state.borrow().rect;
        let vp_size = match self.direction {
            Direction::Vertical => r.height(),
            Direction::Horizontal => r.width(),
        };

        let delta = match distance {
            MouseScrollDistance::Lines { x, y, z: _ } => {
                match self.direction {
                    Direction::Vertical => (y * SCROLL_LINE as f64 * scale).round() as i32,
                    Direction::Horizontal => (x * SCROLL_LINE as f64 * scale).round() as i32,
                }
            }
            MouseScrollDistance::Pixels { x, y, z: _ } => {
                match self.direction {
                    Direction::Vertical => y as i32,
                    Direction::Horizontal => x as i32,
                }
            }
            MouseScrollDistance::Pages { x, y, z: _ } => {
                match self.direction {
                    Direction::Vertical => (y * vp_size as f64).round() as i32,
                    Direction::Horizontal => (x * vp_size as f64).round() as i32,
                }
            }
        };

        let offset = self.scroll_offset.get() + delta;
        self.scroll_offset.set(offset);
        self.clamp_scroll();
        true
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        if let Some(child) = &*self.child.borrow() {
            if child.borrow().on_key_down(ui, virtual_key_code, scancode, state) {
                return true;
            }
        }
        false
    }

    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        if let Some(child) = &*self.child.borrow() {
            if child.borrow().on_key_up(ui, virtual_key_code, scancode, state) {
                return true;
            }
        }
        false
    }

    fn on_key_char(&self, ui: &mut UI, unicode_codepoint: char, state: ModifiersState) -> bool {
        if let Some(child) = &*self.child.borrow() {
            if child.borrow().on_key_char(ui, unicode_codepoint, state) {
                return true;
            }
        }
        false
    }
}

impl ScrollView {
    fn paint_scrollbar(&self, abs_origin: Point<i32>, theme: &mut dyn Theme, _scale: f64) {
        // Track background
        let track = self.track_rect();
        let abs_track = rect(
            (abs_origin.x + track.min.x, abs_origin.y + track.min.y),
            (abs_origin.x + track.max.x, abs_origin.y + track.max.y),
        );
        theme.draw_scrollbar_track(abs_track, self.direction);

        // Arrow start button
        let arrow_start = self.arrow_start_rect();
        let abs_arrow_start = rect(
            (abs_origin.x + arrow_start.min.x, abs_origin.y + arrow_start.min.y),
            (abs_origin.x + arrow_start.max.x, abs_origin.y + arrow_start.max.y),
        );
        let arrow_start_state = ViewState {
            pressed: self.arrow_start_pressed.get(),
            hovered: self.arrow_start_pressed.get(),
            ..ViewState::no_focus()
        };
        theme.draw_scrollbar_arrow_button(abs_arrow_start, arrow_start_state, true, self.direction);

        // Arrow end button
        let arrow_end = self.arrow_end_rect();
        let abs_arrow_end = rect(
            (abs_origin.x + arrow_end.min.x, abs_origin.y + arrow_end.min.y),
            (abs_origin.x + arrow_end.max.x, abs_origin.y + arrow_end.max.y),
        );
        let arrow_end_state = ViewState {
            pressed: self.arrow_end_pressed.get(),
            hovered: self.arrow_end_pressed.get(),
            ..ViewState::no_focus()
        };
        theme.draw_scrollbar_arrow_button(abs_arrow_end, arrow_end_state, false, self.direction);

        // Thumb
        let thumb = self.thumb_rect();
        let abs_thumb = rect(
            (abs_origin.x + thumb.min.x, abs_origin.y + thumb.min.y),
            (abs_origin.x + thumb.max.x, abs_origin.y + thumb.max.y),
        );
        let thumb_state = ViewState {
            pressed: false,
            hovered: false,
            ..ViewState::no_focus()
        };
        theme.draw_scrollbar_thumb(abs_thumb, thumb_state, self.direction);
    }
}

impl Default for ScrollView {
    fn default() -> Self {
        let r = rect((0, 0), (200, 200));
        ScrollView::new(r, Dimension::Max, Dimension::Max)
    }
}
