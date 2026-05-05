use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};

const DEFAULT_SPACING: i32 = 8;
const DEFAULT_ITEM_MAX_WIDTH: i32 = 360;
const DEFAULT_EDGE_INSET_RIGHT: i32 = 16;
const DEFAULT_EDGE_INSET_BOTTOM: i32 = 16;
const ENTER_MS: u128 = 180;
const LEAVE_MS: u128 = 160;
/// How fast `current_y` chases `target_y` when slots shift (per ms, in pixels).
/// Tuned to look like a ~140ms tween across a typical item height.
const FALL_SPEED_PER_MS: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Entering,
    Visible,
    Leaving,
}

struct NotificationItem {
    id: String,
    element: Element,
    expires_at: Option<Instant>,
    phase: Phase,
    phase_started: Instant,
    /// X anchor (target after slide-in completes), in window pixels.
    target_x: i32,
    /// Y target the item is tweening toward, in window pixels.
    target_y: i32,
    /// Currently rendered Y position, tweens toward `target_y`.
    current_y: f32,
    width: i32,
    height: i32,
    /// `false` until the item has been laid out and current_y placed.
    placed: bool,
}

pub struct NotificationStack {
    state: RefCell<FieldsMain>,
    items: RefCell<Vec<NotificationItem>>,
    spacing: i32,
    item_max_width: i32,
    edge_inset: (i32, i32),
    cached_typeface: RefCell<Option<Typeface>>,
}

impl HasMainFields for NotificationStack {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for NotificationStack {}

#[allow(dead_code)]
impl NotificationStack {
    pub fn new() -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (0, 0)), Dimension::Max, Dimension::Max);
        main.state.focusable = false;
        NotificationStack {
            state: RefCell::new(main),
            items: RefCell::new(Vec::new()),
            spacing: DEFAULT_SPACING,
            item_max_width: DEFAULT_ITEM_MAX_WIDTH,
            edge_inset: (DEFAULT_EDGE_INSET_RIGHT, DEFAULT_EDGE_INSET_BOTTOM),
            cached_typeface: RefCell::new(None),
        }
    }

    pub fn set_spacing(&mut self, dip: i32) { self.spacing = dip; }
    pub fn set_item_max_width(&mut self, dip: i32) { self.item_max_width = dip; }
    pub fn set_edge_inset(&mut self, right: i32, bottom: i32) { self.edge_inset = (right, bottom); }

    /// Add a new item. If `id` is already present it is dismissed immediately first.
    pub fn push_item(
        &self,
        id: &str,
        element: Element,
        timeout: Option<Duration>,
        typeface: &Typeface,
        scale: f64,
    ) {
        // Replace existing
        self.dismiss_immediate(id);

        // Tag the element with the id so callers can look it up
        element.borrow_mut().set_id(id);

        // Size the element. Width: clamped to item_max_width (scaled). Height: Min content.
        let max_w_px = (self.item_max_width as f64 * scale).round() as i32;
        // Lay out at (0, 0) — we move it ourselves later.
        let mut e = element.borrow_mut();
        e.set_scale(scale);
        e.layout_content(0, 0, max_w_px, i32::MAX / 4, typeface, scale);
        let r = e.get_rect();
        let width = r.width().min(max_w_px);
        let height = r.height();
        drop(e);

        let now = Instant::now();
        let item = NotificationItem {
            id: id.to_owned(),
            element,
            expires_at: timeout.map(|d| now + d),
            phase: Phase::Entering,
            phase_started: now,
            target_x: 0,
            target_y: 0,
            current_y: 0.0,
            width,
            height,
            placed: false,
        };

        // Insert at the front (newest on top of the visual stack — bottom-anchored,
        // newer items reserve a slot above the current uppermost).
        self.items.borrow_mut().insert(0, item);
        *self.cached_typeface.borrow_mut() = Some(typeface.clone());
    }

    /// Transition an item to the Leaving phase. No-op if id is unknown or already leaving.
    pub fn dismiss(&self, id: &str) {
        let mut items = self.items.borrow_mut();
        for it in items.iter_mut() {
            if it.id == id && it.phase != Phase::Leaving {
                it.phase = Phase::Leaving;
                it.phase_started = Instant::now();
                break;
            }
        }
    }

    /// Remove an item without animation.
    pub fn dismiss_immediate(&self, id: &str) {
        self.items.borrow_mut().retain(|it| it.id != id);
    }

    /// Transition every visible/entering item to the Leaving phase.
    pub fn dismiss_all(&self) {
        let now = Instant::now();
        let mut items = self.items.borrow_mut();
        for it in items.iter_mut() {
            if it.phase != Phase::Leaving {
                it.phase = Phase::Leaving;
                it.phase_started = now;
            }
        }
    }

    pub fn has(&self, id: &str) -> bool {
        self.items.borrow().iter().any(|it| it.id == id && it.phase != Phase::Leaving)
    }

    /// Walks parents of `view` until it finds a view whose id matches a current
    /// notification, then returns that id. Used so close-X callbacks don't have to
    /// capture the id by closure.
    pub fn id_for_descendant(&self, view: &dyn View) -> Option<String> {
        let items = self.items.borrow();
        // Check the view itself
        let id = view.get_id();
        if items.iter().any(|it| it.id == id) {
            return Some(id);
        }
        // Walk up via parent chain
        let mut current = view.get_parent();
        while let Some(p) = current {
            let pid = p.borrow().get_id();
            if items.iter().any(|it| it.id == pid) {
                return Some(pid);
            }
            current = p.borrow().get_parent();
        }
        None
    }

    fn ease_out_cubic(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    /// Compute the slide_offset_x (positive = pushed right, off-screen) for an item.
    fn slide_offset_x(item: &NotificationItem) -> i32 {
        let elapsed_ms = item.phase_started.elapsed().as_millis();
        match item.phase {
            Phase::Entering => {
                let t = (elapsed_ms as f32) / (ENTER_MS as f32);
                let off = (item.width as f32) * (1.0 - Self::ease_out_cubic(t));
                off.round() as i32
            }
            Phase::Visible => 0,
            Phase::Leaving => {
                let t = (elapsed_ms as f32) / (LEAVE_MS as f32);
                let off = (item.width as f32) * Self::ease_out_cubic(t);
                off.round() as i32
            }
        }
    }

    /// Snapshot (Element, current-rect) for items in the Visible phase.
    /// Releases the items borrow before returning so callbacks driven by the
    /// forwarded events can mutate the items list without panicking.
    fn snapshot_visible(&self) -> Vec<(Element, Rect<i32>)> {
        let items = self.items.borrow();
        items.iter()
            .filter(|it| it.phase == Phase::Visible)
            .map(|it| {
                let r = rect(
                    (it.target_x, it.current_y.round() as i32),
                    (it.target_x + it.width, it.current_y.round() as i32 + it.height),
                );
                (Rc::clone(&it.element), r)
            })
            .collect()
    }

    fn alpha(item: &NotificationItem) -> f32 {
        let elapsed_ms = item.phase_started.elapsed().as_millis();
        match item.phase {
            Phase::Entering => {
                let t = (elapsed_ms as f32) / (ENTER_MS as f32);
                Self::ease_out_cubic(t)
            }
            Phase::Visible => 1.0,
            Phase::Leaving => {
                let t = (elapsed_ms as f32) / (LEAVE_MS as f32);
                (1.0 - Self::ease_out_cubic(t)).max(0.0)
            }
        }
    }

    /// Recompute target_x/target_y for every item and prime current_y for newly
    /// placed items so they don't fall down from above.
    fn recompute_targets(&self, win_w: i32, win_h: i32, scale: f64) {
        let inset_r = (self.edge_inset.0 as f64 * scale).round() as i32;
        let inset_b = (self.edge_inset.1 as f64 * scale).round() as i32;
        let spacing = (self.spacing as f64 * scale).round() as i32;

        let mut items = self.items.borrow_mut();
        // Items are stored newest-first. Bottom-anchored: the LAST item in the
        // vector (oldest) sits closest to the bottom edge; earlier items stack
        // upward above it.
        let mut bottom_cursor = win_h - inset_b;
        for it in items.iter_mut().rev() {
            it.target_y = bottom_cursor - it.height;
            it.target_x = win_w - inset_r - it.width;
            if !it.placed {
                it.current_y = it.target_y as f32;
                it.placed = true;
            }
            bottom_cursor = it.target_y - spacing;
        }
    }
}

impl View for NotificationStack {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "spacing" => { if let Ok(v) = value.parse() { self.spacing = v; } }
            "item_max_width" => { if let Ok(v) = value.parse() { self.item_max_width = v; } }
            "inset_right" => { if let Ok(v) = value.parse() { self.edge_inset.0 = v; } }
            "inset_bottom" => { if let Ok(v) = value.parse() { self.edge_inset.1 = v; } }
            _ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) { self.base_set_parent(parent); }
    fn get_parent(&self) -> Option<Element> { self.base_get_parent() }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.base_set_scale(scale);
        let r = rect((x, y), (x + width, y + height));
        self.set_rect(r);
        *self.cached_typeface.borrow_mut() = Some(typeface.clone());
        // Re-target items for the new window size. Items keep their measured
        // width/height; only their slot positions shift.
        self.recompute_targets(width, height, scale);
        r
    }

    fn fits_in_rect(&self, _width: i32, _height: i32, _scale: f64) -> bool { true }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        // Snapshot enough state to release the items borrow before painting,
        // so callbacks driven by deeper paint paths can't deadlock the RefCell.
        struct PaintItem {
            element: Element,
            draw_x: i32,
            draw_y: i32,
            width: i32,
            height: i32,
            alpha: f32,
        }
        let snapshot: Vec<PaintItem> = {
            let items = self.items.borrow();
            items.iter().map(|it| {
                let off = Self::slide_offset_x(it);
                PaintItem {
                    element: Rc::clone(&it.element),
                    draw_x: it.target_x + off,
                    draw_y: it.current_y.round() as i32,
                    width: it.width,
                    height: it.height,
                    alpha: Self::alpha(it),
                }
            }).collect()
        };

        for it in snapshot {
            // Move the element to its current draw position so hit-testing
            // matches the on-screen location.
            {
                let mut el = it.element.borrow_mut();
                let r = rect(
                    (it.draw_x, it.draw_y),
                    (it.draw_x + it.width, it.draw_y + it.height),
                );
                el.set_rect(r);
            }
            theme.push_opacity(it.alpha);
            it.element.borrow().paint(origin, theme);
            theme.pop_opacity();
        }
    }

    fn get_state(&self) -> Option<ViewState> { Some(self.state.borrow().state) }
    fn get_rect(&self) -> Rect<i32> { self.base_get_rect() }
    fn set_rect(&mut self, rect: Rect<i32>) { self.base_set_rect(rect); }
    fn get_padding(&self, scale: f64) -> Borders { self.base_get_padding(scale) }
    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) { self.base_set_padding(top, left, right, bottom); }
    fn get_margin(&self, scale: f64) -> Borders { self.base_get_margin(scale) }
    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) { self.base_set_margin(top, left, right, bottom); }
    fn get_gravity(&self) -> Gravity { self.base_get_gravity() }
    fn set_gravity(&self, gravity: Gravity) { self.base_set_gravity(gravity); }
    fn get_bounds(&self) -> (Dimension, Dimension) { self.base_get_bounds() }
    fn get_content_size(&self) -> (i32, i32) { (0, 0) }
    fn is_break(&self) -> bool { self.base_is_break() }
    fn set_focusable(&self, focusable: bool) { self.base_set_focusable(focusable); }
    fn set_width(&mut self, width: Dimension) { self.base_set_width(width); }
    fn set_height(&mut self, height: Dimension) { self.base_set_height(height); }
    fn set_scale(&mut self, scale: f64) { self.base_set_scale(scale); }
    fn set_id(&mut self, id: &str) { self.base_set_id(id); }
    fn get_id(&self) -> String { self.base_get_id() }
    fn get_tooltip(&self) -> Option<String> { self.base_get_tooltip() }
    fn set_tooltip(&mut self, tooltip: Option<String>) { self.base_set_tooltip(tooltip); }
    fn get_background(&self) -> Option<u32> { self.base_get_background() }
    fn set_background(&mut self, color: Option<u32>) { self.base_set_background(color); }
    fn get_border_color(&self) -> Option<u32> { self.base_get_border_color() }
    fn set_border_color(&mut self, color: Option<u32>) { self.base_set_border_color(color); }
    fn is_enabled(&self) -> bool { self.base_is_enabled() }
    fn set_enabled(&mut self, enabled: bool) { self.base_set_enabled(enabled); }
    fn get_visibility(&self) -> Visibility { self.base_get_visibility() }
    fn set_visibility(&mut self, visibility: Visibility) { self.base_set_visibility(visibility); }

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {}
    fn click(&self, _ui: &mut UI) -> bool { false }

    fn update(&mut self, _ui: &mut UI) -> bool {
        let now = Instant::now();
        let mut redraw = false;

        let win_rect = self.state.borrow().rect;
        let win_w = win_rect.width();
        let win_h = win_rect.height();
        let scale = self.state.borrow().scale;

        // Phase advancement and auto-dismiss
        {
            let mut items = self.items.borrow_mut();
            for it in items.iter_mut() {
                match it.phase {
                    Phase::Entering => {
                        if it.phase_started.elapsed().as_millis() >= ENTER_MS {
                            it.phase = Phase::Visible;
                            it.phase_started = now;
                        }
                        redraw = true;
                    }
                    Phase::Visible => {
                        if let Some(deadline) = it.expires_at {
                            if now >= deadline {
                                it.phase = Phase::Leaving;
                                it.phase_started = now;
                                redraw = true;
                            }
                        }
                    }
                    Phase::Leaving => {
                        redraw = true;
                    }
                }
            }
            // Drop items whose Leaving phase finished
            let before = items.len();
            items.retain(|it| !(it.phase == Phase::Leaving && it.phase_started.elapsed().as_millis() >= LEAVE_MS));
            if items.len() != before {
                redraw = true;
            }
        }

        // Always recompute targets — items above a leaving one should fall down
        // smoothly while the leaver is still animating out.
        self.recompute_targets(win_w, win_h, scale);

        // Tween current_y toward target_y
        // Use ~16ms / frame, but compute from real elapsed since prev tick is
        // not tracked; FALL_SPEED_PER_MS is calibrated for a ~16ms cadence.
        let frame_step = (FALL_SPEED_PER_MS * 16.0).max(1.0);
        {
            let mut items = self.items.borrow_mut();
            for it in items.iter_mut() {
                let dy = it.target_y as f32 - it.current_y;
                if dy.abs() <= frame_step {
                    if dy.abs() > 0.5 {
                        redraw = true;
                    }
                    it.current_y = it.target_y as f32;
                } else {
                    it.current_y += dy.signum() * frame_step;
                    redraw = true;
                }
            }
        }

        redraw
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        let snapshot = self.snapshot_visible();
        let pt = Point::from((position.x, position.y));
        let mut over_item = false;
        for (el, r) in snapshot {
            if r.hit(pt) {
                over_item = true;
                el.borrow().on_mouse_move(ui, position);
            }
        }
        // Consume the move so the underlying UI doesn't get hover state for
        // anything covered by a notification.
        over_item
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let snapshot = self.snapshot_visible();
        let pt = Point::from((position.x, position.y));
        for (el, r) in snapshot {
            if r.hit(pt) {
                // Forward so inner children (like the close-X button) handle the
                // click. Whether they handle it or not, we *consume* the click —
                // notifications are opaque to input within their bounds.
                el.borrow().on_mouse_button_down(ui, position, button);
                return true;
            }
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        let snapshot = self.snapshot_visible();
        let pt = Point::from((position.x, position.y));
        for (el, r) in snapshot {
            if r.hit(pt) {
                el.borrow().on_mouse_button_up(ui, position, button);
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Vector2<i32>, distance: MouseScrollDistance) -> bool {
        let snapshot = self.snapshot_visible();
        let pt = Point::from((position.x, position.y));
        for (el, r) in snapshot {
            if r.hit(pt) {
                el.borrow().on_mouse_wheel_scroll(ui, position, distance);
                return true;
            }
        }
        false
    }

    fn on_key_down(&self, _ui: &mut UI, _vkc: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool { false }
    fn on_key_up(&self, _ui: &mut UI, _vkc: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool { false }
    fn on_key_char(&self, _ui: &mut UI, _ch: char, _state: ModifiersState) -> bool { false }
    fn on_key_mod_changed(&self, _ui: &mut UI, _state: ModifiersState) -> bool { false }
}

impl Default for NotificationStack {
    fn default() -> Self { NotificationStack::new() }
}
