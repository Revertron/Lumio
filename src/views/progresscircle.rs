//! Round progress indicator — the circular counterpart of [`ProgressBar`].
//!
//! Two modes:
//! - **determinate** (default): a full track ring with a round-capped arc that
//!   grows clockwise from 12 o'clock. Setting a new value doesn't jump — the
//!   drawn arc eases toward it, so a progress feed looks smooth even when the
//!   app reports in coarse steps.
//! - **indeterminate** (`indeterminate="true"`): a ring of dots orbiting the
//!   circle, each one a little smaller and fainter than the one ahead of it, so
//!   the trail reads as motion; the whole ring breathes gently in and out. The
//!   dots are spaced edge to edge, not centre to centre, so the gaps stay even
//!   as the dots shrink along the trail.
//!
//! Both animations are driven off a wall clock, not a frame counter, so they run
//! at the same speed whatever the tick rate.
//!
//! XML: `<ProgressCircle value="0.4" show_value="true" thickness="4"/>`.
//!
//! [`ProgressBar`]: super::ProgressBar

use std::cell::{Cell, RefCell};
use std::f32::consts::{PI, TAU};
use std::time::Instant;

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::text::{TextBlock, TextOptions};
use crate::themes::{Renderer, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};

/// Dots in the indeterminate ring.
const DOT_COUNT: usize = 10;
/// Fraction of the circle the trail spans; the rest is the gap behind its tail.
const TRAIL_SPAN: f32 = 0.9;
/// Smallest gap between two neighbouring dots, as a multiple of the dot radius.
/// Only binds when the ring is so thick that the dots can't all fit at that
/// spacing — then they shrink to keep the gap.
const MIN_GAP_RATIO: f32 = 0.5;
/// Seconds per revolution of the indeterminate ring.
const SPIN_PERIOD: f32 = 1.4;
/// Seconds per cycle of the gentle size pulse applied to every dot.
const PULSE_PERIOD: f32 = 2.2;
/// Amplitude of that pulse, as a fraction of the dot radius.
const PULSE_DEPTH: f32 = 0.08;
/// How much smaller the last dot of the trail is than the leading one.
const TRAIL_SHRINK: f32 = 0.55;
/// How much fainter the last dot of the trail is than the leading one.
const TRAIL_FADE: f32 = 0.72;
/// Dot radius as a multiple of the ring thickness.
const DOT_RADIUS_RATIO: f32 = 0.9;
/// Exponential rate (per second) at which the drawn value approaches the set
/// one: ~99% of the way there in half a second.
const EASE_RATE: f32 = 9.0;
/// Below this difference the eased value snaps to its target.
const EASE_EPSILON: f32 = 0.0005;
/// Centre label size, as a fraction of the diameter inside the ring.
const LABEL_FRACTION: f32 = 0.42;

pub struct ProgressCircle {
    state: RefCell<FieldsMain>,
    /// Target progress, 0.0..=1.0 (ignored in indeterminate mode).
    value: Cell<f32>,
    /// The value actually drawn; eases toward `value` in `update`.
    display: Cell<f32>,
    indeterminate: Cell<bool>,
    /// Draw the percentage in the middle of the ring (determinate only).
    show_value: Cell<bool>,
    /// Ring thickness in dips; `None` uses the palette's
    /// `progress_circle.thickness`.
    thickness: Cell<Option<f32>>,
    track_color: RefCell<Option<u32>>,
    fill_color: RefCell<Option<u32>>,
    /// Animation clock. Phases come from elapsed time, so a dropped tick
    /// doesn't slow the animation down and two circles never drift apart.
    started: Cell<Instant>,
    /// Previous tick, for the value easing step.
    last_tick: Cell<Instant>,
    /// Shaped centre label, keyed by `(percent, font size bits)`.
    label: RefCell<Option<(i32, u32, TextBlock)>>,
}

impl HasMainFields for ProgressCircle {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ProgressCircle {}

#[allow(dead_code)]
impl ProgressCircle {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> ProgressCircle {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.state.focusable = false;
        let now = Instant::now();
        ProgressCircle {
            state: RefCell::new(main),
            value: Cell::new(0.0),
            display: Cell::new(0.0),
            indeterminate: Cell::new(false),
            show_value: Cell::new(false),
            thickness: Cell::new(None),
            track_color: RefCell::new(None),
            fill_color: RefCell::new(None),
            started: Cell::new(now),
            last_tick: Cell::new(now),
            label: RefCell::new(None),
        }
    }

    pub fn get_value(&self) -> f32 {
        self.value.get()
    }

    /// Set the target progress (0.0..=1.0). The ring animates toward it; use
    /// [`set_value_now`](Self::set_value_now) to jump straight there.
    pub fn set_value(&self, value: f32) {
        self.value.set(value.clamp(0.0, 1.0));
    }

    /// Set the progress and draw it immediately, with no easing.
    pub fn set_value_now(&self, value: f32) {
        let value = value.clamp(0.0, 1.0);
        self.value.set(value);
        self.display.set(value);
    }

    pub fn is_indeterminate(&self) -> bool {
        self.indeterminate.get()
    }

    pub fn set_indeterminate(&self, indeterminate: bool) {
        if self.indeterminate.get() != indeterminate {
            self.indeterminate.set(indeterminate);
            // Restart the orbit from the top, so a spinner that gets switched
            // on always begins at 12 o'clock.
            self.started.set(Instant::now());
        }
    }

    pub fn shows_value(&self) -> bool {
        self.show_value.get()
    }

    pub fn set_show_value(&self, show: bool) {
        self.show_value.set(show);
    }

    /// Ring thickness in dips. `None` restores the palette default.
    pub fn set_thickness(&self, thickness: Option<f32>) {
        self.thickness.set(thickness.map(|t| t.max(0.5)));
    }

    pub fn set_track_color(&self, color: Option<u32>) {
        *self.track_color.borrow_mut() = color;
    }

    pub fn set_fill_color(&self, color: Option<u32>) {
        *self.fill_color.borrow_mut() = color;
    }

    /// Intrinsic side length in physical pixels.
    fn intrinsic_size(scale: f64) -> i32 {
        (crate::drawing::current_dimension("progress_circle.size") as f64 * scale).round() as i32
    }

    /// Ring geometry inside `content`: centre, centre-line radius and band
    /// thickness, all in physical pixels.
    fn geometry(&self, content: Rect<i32>, scale: f64) -> (f32, f32, f32, f32) {
        let diameter = content.width().min(content.height()).max(0) as f32;
        let dip = self
            .thickness
            .get()
            .unwrap_or_else(|| crate::drawing::current_dimension("progress_circle.thickness"));
        // Keep the band inside the circle even on a tiny widget.
        let thickness = (dip * scale as f32).clamp(1.0, (diameter / 2.0).max(1.0));
        let cx = (content.min.x + content.max.x) as f32 / 2.0;
        let cy = (content.min.y + content.max.y) as f32 / 2.0;
        ((diameter - thickness) / 2.0, thickness, cx, cy)
    }

    /// `color` with its alpha channel scaled by `factor`.
    fn fade(color: u32, factor: f32) -> u32 {
        let alpha = ((color >> 24) & 0xff) as f32 * factor.clamp(0.0, 1.0);
        (color & 0x00ff_ffff) | ((alpha.round() as u32) << 24)
    }

    /// Orbiting-dot ring: a head dot with a trail of ever smaller, fainter dots
    /// behind it, the whole ring breathing in and out as it turns.
    fn paint_dots(&self, theme: &mut dyn Renderer, cx: f32, cy: f32, radius: f32, thickness: f32, color: u32) {
        let elapsed = self.started.get().elapsed().as_secs_f32();
        let head = -PI / 2.0 + TAU * (elapsed / SPIN_PERIOD).fract();
        let pulse = 1.0 + PULSE_DEPTH * (TAU * elapsed / PULSE_PERIOD).sin();

        // Dots shrink along the trail, so equal centre-to-centre steps would
        // leave visibly uneven gaps. Instead the *edges* are equally spaced:
        // the step between neighbours is `r[i] + gap + r[i+1]`, and `gap` is
        // solved from the arc the trail is supposed to cover.
        let size_of = |i: usize| 1.0 - TRAIL_SHRINK * (i as f32 / (DOT_COUNT - 1) as f32);
        let pair_sum: f32 = (0..DOT_COUNT - 1).map(|i| size_of(i) + size_of(i + 1)).sum();
        let span = TAU * radius * TRAIL_SPAN;
        // A ring thick enough that the dots wouldn't fit shrinks them instead
        // of eating into the gap.
        let room = span / (pair_sum + MIN_GAP_RATIO * (DOT_COUNT - 1) as f32);
        let dot_radius = (thickness * DOT_RADIUS_RATIO * pulse).min(room);
        let gap = (span - dot_radius * pair_sum) / (DOT_COUNT - 1) as f32;

        let mut angle = head;
        for i in 0..DOT_COUNT {
            // 0 = the leading dot, 1 = the tail end of the trail.
            let t = i as f32 / (DOT_COUNT - 1) as f32;
            let (sin, cos) = angle.sin_cos();
            theme.draw_circle(
                cx + radius * cos,
                cy + radius * sin,
                dot_radius * size_of(i),
                Self::fade(color, 1.0 - TRAIL_FADE * t),
            );
            if i + 1 < DOT_COUNT {
                // Step back by the chord that leaves exactly `gap` between the
                // two dots' edges (chord, not arc — that is the gap the eye sees).
                let chord = dot_radius * (size_of(i) + size_of(i + 1)) + gap;
                angle -= 2.0 * (chord / (2.0 * radius)).clamp(-1.0, 1.0).asin();
            }
        }
    }

    /// Shape (and cache) the centre percentage label for the drawn value.
    fn ensure_label(&self, percent: i32, size_px: f32) {
        if let Some((cached_percent, cached_size, _)) = self.label.borrow().as_ref()
            && *cached_percent == percent
            && *cached_size == size_px.to_bits()
        {
            return;
        }
        let typeface = self.state.borrow().font_manager.get();
        let block = typeface
            .as_ref()
            .and_then(|tf| get_font_family(&tf.font_name, tf.font_style))
            .map(|font| font.layout_text(&format!("{percent}%"), size_px, TextOptions::new()));
        *self.label.borrow_mut() = block.map(|b| (percent, size_px.to_bits(), b));
    }
}

impl View for ProgressCircle {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            // The XML value is the initial state, so it is drawn as-is rather
            // than animated up from zero.
            "value" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.set_value_now(v);
                }
            }
            "indeterminate" => self.set_indeterminate(value == "true"),
            "show_value" => self.show_value.set(value == "true"),
            "thickness" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.set_thickness(Some(v));
                }
            }
            "track_color" => {
                *self.track_color.borrow_mut() = crate::view_base::parse_color_value(value);
            }
            "fill_color" => {
                *self.fill_color.borrow_mut() = crate::view_base::parse_color_value(value);
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

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        let effective = self.state.borrow().font_manager.get_typeface(typeface);
        self.state.borrow_mut().font_manager.set(Some(effective));
        self.base_set_scale(scale);
        let (new_width, new_height) = self.calculate_bounded_size(width, height, scale);
        let r = rect((x, y), (x + new_width, y + new_height));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let r = self.state.borrow().rect;
        r.width() <= width && r.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Renderer) {
        let (mut r, scale, enabled) = {
            let s = self.state.borrow();
            (s.rect, s.scale, s.state.enabled)
        };
        r.move_by(origin);
        self.base_draw_ninepatch(theme, r);

        let padding = self.get_padding(scale);
        let content = rect(
            (r.min.x + padding.left, r.min.y + padding.top),
            (r.max.x - padding.right, r.max.y - padding.bottom),
        );
        let (radius, thickness, cx, cy) = self.geometry(content, scale);
        if radius <= 0.0 {
            return;
        }

        if !enabled {
            theme.push_opacity(0.5);
        }
        let fill = self.fill_color.borrow().unwrap_or_else(|| theme.color("progress_fill"));

        if self.indeterminate.get() {
            self.paint_dots(theme, cx, cy, radius, thickness, fill);
        } else {
            let track = self.track_color.borrow().unwrap_or_else(|| theme.color("outline"));
            theme.draw_arc(cx, cy, radius, 0.0, TAU, thickness, Self::fade(track, 0.45));

            let progress = self.display.get();
            if progress > 0.0 {
                theme.draw_arc(cx, cy, radius, -PI / 2.0, TAU * progress, thickness, fill);
            }

            if self.show_value.get() {
                // The label lives inside the ring, and never outgrows it.
                let inner = (radius * 2.0 - thickness * 2.0).max(0.0);
                let base = self
                    .state
                    .borrow()
                    .font_manager
                    .get()
                    .and_then(|tf| tf.font_size)
                    .unwrap_or_else(|| crate::drawing::current_text_size("text"));
                let size_px = (base * scale as f32).min(inner * LABEL_FRACTION);
                if size_px >= 1.0 {
                    self.ensure_label((progress * 100.0).round() as i32, size_px);
                    if let Some((_, _, block)) = self.label.borrow().as_ref() {
                        let color = theme.color("text");
                        theme.draw_text(cx - block.width() / 2.0, cy - block.height() / 2.0, color, block);
                    }
                }
            }
        }
        if !enabled {
            theme.pop_opacity();
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

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let scale = self.state.borrow().scale;
        let size = Self::intrinsic_size(scale);
        (size, size)
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

    fn get_content_description(&self) -> Option<String> {
        self.base_get_content_description()
    }

    fn set_content_description(&mut self, description: Option<String>) {
        self.base_set_content_description(description);
    }

    fn get_labelled_by(&self) -> Option<String> {
        self.base_get_labelled_by()
    }

    fn set_labelled_by(&mut self, view_id: Option<String>) {
        self.base_set_labelled_by(view_id);
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

    fn on_event(&mut self, event: EventType, func: EventCallback) {
        self.base_on_event(event, func);
    }

    fn has_listener(&self, event: EventType) -> bool {
        self.base_has_listener(event)
    }

    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool {
        self.base_fire_event(ui, event, data)
    }

    fn click(&self, _ui: &mut UI) -> bool {
        // A progress indicator is not interactive.
        false
    }

    fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::ProgressIndicator);
        // An indeterminate circle has no meaningful value to report.
        if !self.is_indeterminate() {
            node.set_numeric_value(f64::from(self.get_value()));
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(1.0);
        }
        node
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        let now = Instant::now();
        // Cap the step so a stalled window (dragged, minimized) doesn't make the
        // ring jump when it resumes.
        let dt = (now - self.last_tick.get()).as_secs_f32().min(0.1);
        self.last_tick.set(now);

        if self.indeterminate.get() {
            return true; // the orbit is always moving
        }
        let delta = self.value.get() - self.display.get();
        if delta.abs() < EASE_EPSILON {
            if delta != 0.0 {
                self.display.set(self.value.get());
                return true;
            }
            return false;
        }
        self.display.set(self.display.get() + delta * (1.0 - (-EASE_RATE * dt).exp()));
        true
    }
}

impl Default for ProgressCircle {
    fn default() -> Self {
        let size = crate::drawing::current_dimension("progress_circle.size") as i32;
        ProgressCircle::new(rect((0, 0), (size, size)), Dimension::Min, Dimension::Min)
    }
}
