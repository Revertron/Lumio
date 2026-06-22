//! Horizontal slider: a flat rounded track with an accent-filled portion and a
//! draggable round thumb. A deliberate step away from the Win95/sunken chrome
//! toward a modern, flat control.
//!
//! Float-valued (`min`/`max`/`value`); an optional `step` snaps the thumb and
//! drives keyboard/wheel increments and the "steps" labels. Fires
//! [`EventType::ValueChanged`] with [`EventData::Value`] on every change
//! (drag, click-to-position, keyboard, wheel).
//!
//! XML: `<Slider min="0" max="100" value="42" step="1" label_style="ends|current"/>`.

use std::cell::{Cell, RefCell};
use std::str::FromStr;

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};
use crate::text::{TextBlock, TextOptions};
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};

/// Minimum intrinsic width (dip) when the slider is given `Min` width.
const DEFAULT_MIN_WIDTH: i32 = 120;
/// Cap on the number of step labels/ticks drawn, to avoid clutter on wide ranges.
const STEP_LABEL_MAX: i32 = 20;
/// Above this many steps, draw ticks but not numeric labels.
const STEP_TEXT_MAX: i32 = 10;

/// Which value labels a [`Slider`] draws. Combinable flags, `|`-separated in XML
/// (`label_style="ends|current"`). Modeled on the [`Gravity`] newtype.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LabelStyle(u8);

impl LabelStyle {
    /// No value labels.
    pub const NONE: LabelStyle = LabelStyle(0);
    /// `min` at the left end, `max` at the right end (below the track).
    pub const ENDS: LabelStyle = LabelStyle(0b0001);
    /// A numeric label (and tick) at each `step` position.
    pub const STEPS: LabelStyle = LabelStyle(0b0010);
    /// The current value, in a bubble floating above the thumb.
    pub const CURRENT: LabelStyle = LabelStyle(0b0100);

    /// Whether every bit in `other` is set in `self` (false for `NONE`).
    pub fn contains(self, other: LabelStyle) -> bool {
        other.0 != 0 && self.0 & other.0 == other.0
    }

    /// Whether no label flags are set.
    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl Default for LabelStyle {
    fn default() -> Self {
        LabelStyle::NONE
    }
}

impl std::ops::BitOr for LabelStyle {
    type Output = LabelStyle;
    fn bitor(self, rhs: Self) -> Self::Output {
        LabelStyle(self.0 | rhs.0)
    }
}

impl FromStr for LabelStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bits: u8 = 0;
        for token in s.split('|') {
            match token.trim() {
                "ends" => bits |= LabelStyle::ENDS.0,
                "steps" => bits |= LabelStyle::STEPS.0,
                "current" => bits |= LabelStyle::CURRENT.0,
                _ => {}
            }
        }
        Ok(LabelStyle(bits))
    }
}

/// Resolved pixel geometry for the current rect/scale/style. Offsets are
/// relative to the rect's top-left; paint and hit-testing add their own base.
struct Metrics {
    thumb_d: i32,
    track_h: i32,
    /// Focus-ring width (px) drawn around the thumb when focused.
    fr: i32,
    /// Reserved halo (px) around the thumb on every side, so the focus ring at
    /// the track ends / band edges is never clipped by the widget rect.
    pad: i32,
    top_band: i32,
    /// Left edge of the usable track region, relative to the rect's left.
    track_left: i32,
    /// Width the thumb centre can travel across.
    usable: i32,
    /// Track centre Y, relative to the rect's top.
    cy: i32,
    /// Total intrinsic height (top band + thumb halo + bottom band).
    total_h: i32,
}

/// Cache key for the shaped value labels. Rebuilt only when one of these changes.
#[derive(Clone, PartialEq)]
struct LabelKey {
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    scale_milli: i32,
    width: i32,
    style: LabelStyle,
}

struct LabelCache {
    key: LabelKey,
    min_lbl: Option<TextBlock>,
    max_lbl: Option<TextBlock>,
    cur_lbl: Option<TextBlock>,
    step_lbls: Vec<(f32, TextBlock)>,
}

pub struct Slider {
    state: RefCell<FieldsMain>,
    min: Cell<f32>,
    max: Cell<f32>,
    value: Cell<f32>,
    /// 0 = continuous; otherwise the thumb snaps to multiples of `step`.
    step: Cell<f32>,
    label_style: Cell<LabelStyle>,
    track_color: RefCell<Option<u32>>,
    fill_color: RefCell<Option<u32>>,
    thumb_color: RefCell<Option<u32>>,
    dragging: Cell<bool>,
    thumb_hovered: Cell<bool>,
    cached: RefCell<Option<LabelCache>>,
}

impl HasMainFields for Slider {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for Slider {}

#[allow(dead_code)]
impl Slider {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> Slider {
        Slider {
            state: RefCell::new(FieldsMain::with_rect(rect, width, height)),
            min: Cell::new(0.0),
            max: Cell::new(100.0),
            value: Cell::new(0.0),
            step: Cell::new(0.0),
            label_style: Cell::new(LabelStyle::NONE),
            track_color: RefCell::new(None),
            fill_color: RefCell::new(None),
            thumb_color: RefCell::new(None),
            dragging: Cell::new(false),
            thumb_hovered: Cell::new(false),
            cached: RefCell::new(None),
        }
    }

    pub fn get_value(&self) -> f32 {
        self.value.get()
    }

    /// Set the value, clamping to `[min, max]` and snapping to `step`. Returns
    /// whether the stored value actually changed. Does not fire an event.
    pub fn set_value(&self, v: f32) -> bool {
        let min = self.min.get();
        let max = self.max.get();
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        let mut v = v.clamp(lo, hi);
        let step = self.step.get();
        if step > 0.0 {
            v = (lo + ((v - lo) / step).round() * step).clamp(lo, hi);
        }
        let changed = (v - self.value.get()).abs() > f32::EPSILON;
        self.value.set(v);
        if changed {
            self.invalidate();
        }
        changed
    }

    pub fn set_range(&self, min: f32, max: f32) {
        self.min.set(min);
        self.max.set(max);
        self.set_value(self.value.get());
        self.invalidate();
    }

    pub fn set_step(&self, step: f32) {
        self.step.set(step.max(0.0));
        self.set_value(self.value.get());
        self.invalidate();
    }

    pub fn set_label_style(&self, style: LabelStyle) {
        self.label_style.set(style);
        self.invalidate();
    }

    fn invalidate(&self) {
        *self.cached.borrow_mut() = None;
    }

    /// Normalised position 0..1 of the current value within the range.
    fn norm(&self) -> f32 {
        let (mn, mx) = (self.min.get(), self.max.get());
        if (mx - mn).abs() < f32::EPSILON {
            return 0.0;
        }
        ((self.value.get() - mn) / (mx - mn)).clamp(0.0, 1.0)
    }

    /// The per-keypress / per-wheel-notch increment.
    fn increment(&self) -> f32 {
        let step = self.step.get();
        if step > 0.0 {
            return step;
        }
        let range = (self.max.get() - self.min.get()).abs();
        if range > 0.0 { range / 100.0 } else { 1.0 }
    }

    /// Reserved height (px) for one band of labels at the current scale.
    fn label_band_px(&self, scale: f64) -> i32 {
        (crate::drawing::current_text_size("text") * scale as f32 * 1.35).ceil() as i32
    }

    fn metrics(&self, scale: f64, width: i32) -> Metrics {
        let thumb_d = ((crate::drawing::current_dimension("slider.thumb_size") * scale as f32).round() as i32).max(8);
        let track_h = ((crate::drawing::current_dimension("slider.track_height") * scale as f32).round() as i32).max(2);
        let gap = (crate::drawing::current_dimension("slider.label_gap") * scale as f32).round() as i32;
        let band = self.label_band_px(scale);
        let fr = ((2.0 * scale).round() as i32).max(2);
        let pad = fr + 1;
        let style = self.label_style.get();
        let top_band = if style.contains(LabelStyle::CURRENT) { band + gap } else { 0 };
        let bottom_band = if style.contains(LabelStyle::ENDS) || style.contains(LabelStyle::STEPS) {
            band + gap
        } else {
            0
        };
        let track_left = thumb_d / 2 + pad;
        let usable = (width - thumb_d - 2 * pad).max(0);
        let cy = top_band + pad + thumb_d / 2;
        let total_h = top_band + thumb_d + 2 * pad + bottom_band;
        Metrics { thumb_d, track_h, fr, pad, top_band, track_left, usable, cy, total_h }
    }

    /// Map a window/parent-local X coordinate to a value.
    fn value_from_x(&self, x_abs: i32, r: Rect<i32>) -> f32 {
        let scale = self.state.borrow().scale;
        let m = self.metrics(scale, r.width());
        if m.usable <= 0 {
            return self.min.get();
        }
        let x_local = x_abs - r.min.x - m.track_left;
        let t = (x_local as f32 / m.usable as f32).clamp(0.0, 1.0);
        let (mn, mx) = (self.min.get(), self.max.get());
        mn + t * (mx - mn)
    }

    /// The thumb rect in the same coordinate space as the view's rect.
    fn thumb_rect(&self, r: Rect<i32>, m: &Metrics) -> Rect<i32> {
        let t = self.norm();
        let cx = r.min.x + m.track_left + (t * m.usable as f32).round() as i32;
        let cy = r.min.y + m.cy;
        rect(
            (cx - m.thumb_d / 2, cy - m.thumb_d / 2),
            (cx + m.thumb_d / 2, cy + m.thumb_d / 2),
        )
    }

    fn fire_changed(&self, ui: &mut UI) {
        let v = self.value.get();
        self.base_fire_event(ui, EventType::ValueChanged, &EventData::Value(v));
    }

    /// Format a value for a label: integer when it has no fractional part,
    /// otherwise with decimals inferred from `step` (trailing zeros stripped).
    fn format_value(&self, v: f32) -> String {
        let step = self.step.get();
        let integral = v.fract().abs() < 1e-4 && (step == 0.0 || step.fract().abs() < 1e-4);
        if integral {
            return format!("{}", v.round() as i64);
        }
        let decimals = if step > 0.0 { decimals_for(step) } else { 2 };
        let s = format!("{:.*}", decimals, v);
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    }

    /// Reshape the value labels if the cache key changed.
    fn ensure_labels(&self, scale: f64, width: i32) {
        let style = self.label_style.get();
        let key = LabelKey {
            value: self.value.get(),
            min: self.min.get(),
            max: self.max.get(),
            step: self.step.get(),
            scale_milli: (scale * 1000.0) as i32,
            width,
            style,
        };
        if let Some(c) = self.cached.borrow().as_ref()
            && c.key == key
        {
            return;
        }

        let typeface = self.state.borrow().font_manager.get();
        let font = typeface
            .as_ref()
            .and_then(|tf| get_font_family(&tf.font_name, tf.font_style));
        let base_size = typeface
            .as_ref()
            .and_then(|tf| tf.font_size)
            .unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let size_px = base_size * scale as f32;
        let shape = |text: String| -> Option<TextBlock> {
            font.as_ref().map(|f| f.layout_text(&text, size_px, TextOptions::new()))
        };

        let (min_lbl, max_lbl) = if style.contains(LabelStyle::ENDS) {
            (
                shape(self.format_value(self.min.get())),
                shape(self.format_value(self.max.get())),
            )
        } else {
            (None, None)
        };
        let cur_lbl = if style.contains(LabelStyle::CURRENT) {
            shape(self.format_value(self.value.get()))
        } else {
            None
        };
        let mut step_lbls = Vec::new();
        if style.contains(LabelStyle::STEPS) && self.step.get() > 0.0 {
            let (mn, mx, st) = (self.min.get(), self.max.get(), self.step.get());
            let n = ((mx - mn) / st).round() as i32;
            if (0..=STEP_LABEL_MAX).contains(&n) {
                let with_text = n <= STEP_TEXT_MAX;
                // Ticks are drawn straight from the range in paint; here we
                // only shape numeric labels, and only when they won't crowd.
                // When `ends` is also on, the min/max labels already own the
                // endpoints, so skip the duplicate step labels there.
                let skip_ends = style.contains(LabelStyle::ENDS);
                if with_text {
                    for i in 0..=n {
                        if skip_ends && (i == 0 || i == n) {
                            continue;
                        }
                        let v = (mn + i as f32 * st).min(mx);
                        if let Some(b) = shape(self.format_value(v)) {
                            step_lbls.push((v, b));
                        }
                    }
                }
            }
        }

        *self.cached.borrow_mut() = Some(LabelCache { key, min_lbl, max_lbl, cur_lbl, step_lbls });
    }
}

fn decimals_for(step: f32) -> usize {
    let mut d = 0usize;
    let mut s = step;
    while d < 4 && (s - s.round()).abs() > 1e-4 {
        s *= 10.0;
        d += 1;
    }
    d
}

impl View for Slider {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        match name {
            "min" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.min.set(v);
                    self.set_value(self.value.get());
                    self.invalidate();
                }
            }
            "max" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.max.set(v);
                    self.set_value(self.value.get());
                    self.invalidate();
                }
            }
            "value" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.set_value(v);
                }
            }
            "step" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.step.set(v.max(0.0));
                    self.set_value(self.value.get());
                    self.invalidate();
                }
            }
            "label_style" => {
                self.label_style.set(value.parse().unwrap_or_default());
                self.invalidate();
            }
            "track_color" => {
                *self.track_color.borrow_mut() = crate::view_base::parse_color_value(value);
            }
            "fill_color" => {
                *self.fill_color.borrow_mut() = crate::view_base::parse_color_value(value);
            }
            "thumb_color" => {
                *self.thumb_color.borrow_mut() = crate::view_base::parse_color_value(value);
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
        let (w, h) = self.calculate_bounded_size(width, height, scale);
        let r = rect((x, y), (x + w, y + h));
        self.set_rect(r);
        self.invalidate();
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let r = self.state.borrow().rect;
        r.width() <= width && r.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let (mut r, scale, vstate, enabled) = {
            let s = self.state.borrow();
            (s.rect, s.scale, s.state, s.state.enabled)
        };
        r.move_by(origin);

        let m = self.metrics(scale, r.width());
        self.ensure_labels(scale, r.width());
        let style = self.label_style.get();

        theme.push_clip();
        theme.clip_rect(r);
        if !enabled {
            theme.push_opacity(0.5);
        }

        let base_x = r.min.x;
        let base_y = r.min.y;
        let track_left = base_x + m.track_left;
        let track_right = base_x + m.track_left + m.usable;
        let cy = base_y + m.cy;
        let (mn, mx) = (self.min.get(), self.max.get());
        let t = self.norm();
        let thumb_cx = track_left + (t * m.usable as f32).round() as i32;
        let radius = m.track_h / 2;

        // Track (unfilled).
        let track_color = self.track_color.borrow().unwrap_or_else(|| theme.color("outline"));
        let track_rect = rect((track_left, cy - m.track_h / 2), (track_right, cy + m.track_h / 2));
        theme.draw_rounded_rect(track_rect, track_color, radius);

        // Accent fill (left of the thumb).
        let fill_color = self.fill_color.borrow().unwrap_or_else(|| theme.color("progress_fill"));
        if thumb_cx > track_left {
            let fill_rect = rect((track_left, cy - m.track_h / 2), (thumb_cx, cy + m.track_h / 2));
            theme.draw_rounded_rect(fill_rect, fill_color, radius);
        }

        // Step ticks (drawn straight from the range; labels come from the cache).
        if style.contains(LabelStyle::STEPS) && self.step.get() > 0.0 && (mx - mn).abs() > f32::EPSILON {
            let n = ((mx - mn) / self.step.get()).round() as i32;
            if (0..=STEP_LABEL_MAX).contains(&n) {
                let tick_w = (scale.round() as i32).max(1);
                let tick_color = theme.color("text_hint");
                for i in 0..=n {
                    let v = (mn + i as f32 * self.step.get()).min(mx);
                    let tt = (v - mn) / (mx - mn);
                    let x = track_left + (tt * m.usable as f32).round() as i32;
                    let tick = rect((x - tick_w / 2, cy - m.track_h), (x - tick_w / 2 + tick_w, cy + m.track_h));
                    theme.draw_rect(tick, tick_color);
                }
            }
        }

        // Bottom-band labels (ends + steps).
        let label_color = theme.color("text_hint");
        let label_y = (base_y + m.top_band + m.thumb_d + 2 * m.pad) as f32;
        {
            let cache = self.cached.borrow();
            if let Some(c) = cache.as_ref() {
                if style.contains(LabelStyle::ENDS) {
                    if let Some(b) = &c.min_lbl {
                        theme.draw_text(track_left as f32, label_y, label_color, b);
                    }
                    if let Some(b) = &c.max_lbl {
                        theme.draw_text(track_right as f32 - b.width(), label_y, label_color, b);
                    }
                }
                if style.contains(LabelStyle::STEPS) && (mx - mn).abs() > f32::EPSILON {
                    for (v, b) in &c.step_lbls {
                        let tt = ((*v - mn) / (mx - mn)).clamp(0.0, 1.0);
                        let x = track_left as f32 + tt * m.usable as f32;
                        theme.draw_text(x - b.width() / 2.0, label_y, label_color, b);
                    }
                }
            }
        }

        // Thumb: optional focus ring, then a bordered circle.
        let thumb_rect = self.thumb_rect(r, &m);
        if vstate.focused {
            let fr = m.fr;
            let ring = rect(
                (thumb_rect.min.x - fr, thumb_rect.min.y - fr),
                (thumb_rect.max.x + fr, thumb_rect.max.y + fr),
            );
            let focus = theme.color("focus");
            theme.draw_rounded_rect(ring, focus, m.thumb_d / 2 + fr);
        }
        let border_w = (scale.round() as i32).max(1);
        let border_color = theme.color("outline");
        theme.draw_rounded_rect(thumb_rect, border_color, m.thumb_d / 2);
        let inner = rect(
            (thumb_rect.min.x + border_w, thumb_rect.min.y + border_w),
            (thumb_rect.max.x - border_w, thumb_rect.max.y - border_w),
        );
        let thumb_fill = self.thumb_color.borrow().unwrap_or_else(|| theme.color("surface"));
        theme.draw_rounded_rect(inner, thumb_fill, (m.thumb_d / 2 - border_w).max(1));

        // Current-value bubble, floating above the thumb.
        if style.contains(LabelStyle::CURRENT) {
            let cache = self.cached.borrow();
            if let Some(b) = cache.as_ref().and_then(|c| c.cur_lbl.as_ref()) {
                let pad_h = (6.0 * scale).round() as i32;
                let pad_v = (3.0 * scale).round() as i32;
                let bw = b.width().ceil() as i32 + pad_h * 2;
                let bh = b.height().ceil() as i32 + pad_v * 2;
                let mut bx = thumb_cx - bw / 2;
                if bw <= r.width() {
                    bx = bx.clamp(r.min.x, r.max.x - bw);
                } else {
                    bx = r.min.x;
                }
                let by = base_y;
                let brect = rect((bx, by), (bx + bw, by + bh));
                let corner = (4.0 * scale).round() as i32;
                let bub_border = theme.color("outline");
                theme.draw_rounded_rect(brect, bub_border, corner);
                let bub_inner = rect(
                    (bx + border_w, by + border_w),
                    (bx + bw - border_w, by + bh - border_w),
                );
                let bub_bg = theme.color("surface");
                theme.draw_rounded_rect(bub_inner, bub_bg, (corner - border_w).max(1));
                let tx = bx + (bw - b.width().ceil() as i32) / 2;
                let ty = by + (bh - b.height().ceil() as i32) / 2;
                let text_color = theme.color("text");
                theme.draw_text(tx as f32, ty as f32, text_color, b);
            }
        }

        if !enabled {
            theme.pop_opacity();
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

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let scale = self.state.borrow().scale;
        let m = self.metrics(scale, 0);
        let min_w = (DEFAULT_MIN_WIDTH as f64 * scale).round() as i32;
        (min_w, m.total_h)
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
        false
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() || !matches!(button, MouseButton::Left) {
            return false;
        }
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) {
            return false;
        }
        {
            let mut s = self.state.borrow_mut();
            s.state.focused = true;
            s.state.pressed = true;
        }
        let scale = self.state.borrow().scale;
        let m = self.metrics(scale, r.width());
        let on_thumb = self.thumb_rect(r, &m).hit((position.x, position.y));
        self.dragging.set(true);
        let mut changed = false;
        if !on_thumb {
            let v = self.value_from_x(position.x, r);
            changed = self.set_value(v);
        }
        if changed {
            self.fire_changed(ui);
        }
        true
    }

    fn on_mouse_button_up(&self, _ui: &mut UI, _position: Point<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let was = self.dragging.get();
        self.dragging.set(false);
        self.state.borrow_mut().state.pressed = false;
        was
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        if self.dragging.get() {
            let r = self.state.borrow().rect;
            let v = self.value_from_x(position.x, r);
            if self.set_value(v) {
                self.fire_changed(ui);
            }
            return true;
        }
        let r = self.state.borrow().rect;
        let scale = self.state.borrow().scale;
        let m = self.metrics(scale, r.width());
        let hov = self.thumb_rect(r, &m).hit((position.x, position.y));
        let changed = self.thumb_hovered.get() != hov;
        self.thumb_hovered.set(hov);
        changed
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        if !self.base_is_enabled() {
            return false;
        }
        let r = self.state.borrow().rect;
        if !r.hit((position.x, position.y)) {
            return false;
        }
        let dy = match distance {
            MouseScrollDistance::Lines { y, .. }
            | MouseScrollDistance::Pixels { y, .. }
            | MouseScrollDistance::Pages { y, .. } => y,
        };
        if dy == 0.0 {
            return false;
        }
        let inc = self.increment();
        let delta = if dy > 0.0 { inc } else { -inc };
        let changed = self.set_value(self.value.get() + delta);
        if changed {
            self.fire_changed(ui);
        }
        changed
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_enabled() || !self.base_is_focused() {
            return false;
        }
        let inc = self.increment();
        let big = inc * 10.0;
        let cur = self.value.get();
        let (mn, mx) = (self.min.get(), self.max.get());
        let new = match virtual_key_code {
            Some(VirtualKeyCode::Left) | Some(VirtualKeyCode::Down) => cur - inc,
            Some(VirtualKeyCode::Right) | Some(VirtualKeyCode::Up) => cur + inc,
            Some(VirtualKeyCode::Home) => mn.min(mx),
            Some(VirtualKeyCode::End) => mn.max(mx),
            Some(VirtualKeyCode::PageUp) => cur + big,
            Some(VirtualKeyCode::PageDown) => cur - big,
            _ => return false,
        };
        if self.set_value(new) {
            self.fire_changed(ui);
        }
        true
    }
}

impl Default for Slider {
    fn default() -> Self {
        let r = rect((0, 0), (200, 24));
        Slider::new(r, Dimension::Max, Dimension::Min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_slider() -> Slider {
        let s = Slider::new(rect((0, 0), (216, 24)), Dimension::Max, Dimension::Min);
        s.base_set_scale(1.0);
        s
    }

    #[test]
    fn clamps_and_snaps() {
        let s = make_slider();
        s.set_range(0.0, 10.0);
        s.set_step(2.0);
        s.set_value(5.3);
        assert_eq!(s.get_value(), 6.0); // 5.3 -> nearest multiple of 2
        s.set_value(100.0);
        assert_eq!(s.get_value(), 10.0); // clamped to max
        s.set_value(-5.0);
        assert_eq!(s.get_value(), 0.0); // clamped to min
    }

    #[test]
    fn value_from_x_midpoint() {
        let s = make_slider();
        s.set_range(0.0, 100.0);
        // width 216, thumb 16 -> usable 200, track_left 8. x=108 -> t=0.5 -> 50.
        let r = rect((0, 0), (216, 24));
        let v = s.value_from_x(108, r);
        assert!((v - 50.0).abs() < 0.01, "got {v}");
    }

    #[test]
    fn label_style_parses_combined_flags() {
        let style: LabelStyle = "ends|current".parse().unwrap();
        assert!(style.contains(LabelStyle::ENDS));
        assert!(style.contains(LabelStyle::CURRENT));
        assert!(!style.contains(LabelStyle::STEPS));
        assert!(LabelStyle::NONE.is_none());
    }

    #[test]
    fn formats_int_and_float() {
        let s = make_slider();
        s.set_step(1.0);
        assert_eq!(s.format_value(42.0), "42");
        s.set_step(0.1);
        assert_eq!(s.format_value(0.5), "0.5");
    }
}
