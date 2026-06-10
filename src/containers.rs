use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};
use super::background::{self, BackgroundImage};
use super::events::EventType;
use super::views::Borders;

use super::themes::{Theme, Typeface, ViewState};
use super::traits::{Container, Element, View, WeakElement};
use super::types::{Point, Rect, rect};
use super::ui::UI;
use super::views::{Dimension, Direction, FieldsMain, Gravity, HAlign, VAlign, Visibility};
use super::view_base::{HasMainFields, ViewBasics};

pub struct Frame {
    state: RefCell<FieldsMain>,
    direction: Direction,
    views: Vec<Element>,
    breaking: bool,
    background_image: RefCell<Option<BackgroundImage>>
}

/// Returns how far to shift a child along its parent's cross axis based on gravity.
/// In a vertical layout the cross axis is horizontal; in horizontal the cross axis is vertical.
#[allow(clippy::too_many_arguments)]
fn cross_axis_offset(
    gravity: Gravity,
    is_vertical: bool,
    parent_width: i32, parent_height: i32,
    parent_padding: &Borders, child_margin: &Borders,
    child_width: i32, child_height: i32
) -> i32 {
    if is_vertical {
        let band = (parent_width - parent_padding.left - parent_padding.right
            - child_margin.left - child_margin.right - child_width).max(0);
        match gravity.horizontal() {
            HAlign::Left => 0,
            HAlign::Center => band / 2,
            HAlign::Right => band,
        }
    } else {
        let band = (parent_height - parent_padding.top - parent_padding.bottom
            - child_margin.top - child_margin.bottom - child_height).max(0);
        match gravity.vertical() {
            VAlign::Top => 0,
            VAlign::Center => band / 2,
            VAlign::Bottom => band,
        }
    }
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
            let vb = v.borrow();
            if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                continue;
            }
            if vb.is_focused() {
                focused = i as i32;
                continue;
            }
            if let Some(state) = vb.get_state() {
                if state.focusable && focused >= 0 {
                    drop(vb);
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
            let vb = v.borrow();
            if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                continue;
            }
            if vb.is_focused() {
                focused = i as i32;
                continue;
            }
            if let Some(state) = vb.get_state() {
                if state.focusable && focused >= 0 {
                    drop(vb);
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
            breaking: false,
            background_image: RefCell::new(None)
        }
    }

    /// Sets the background image asset path, or clears the background image with `None`.
    pub fn set_background_image(&mut self, path: Option<&str>) {
        match path {
            Some(p) => self.background_image_mut().set_path(p),
            None => *self.background_image.borrow_mut() = None,
        }
    }

    /// Access to the background image config (created with defaults if not set yet).
    /// Style fields (`opacity`, `repeat`, `position`, `size`, `origin`) are public.
    pub fn background_image_mut(&mut self) -> RefMut<'_, BackgroundImage> {
        RefMut::map(self.background_image.borrow_mut(), |o| o.get_or_insert_with(BackgroundImage::default))
    }

    fn set_font(&mut self, font_name: &str) {
        self.state.borrow_mut().font_manager.set_font(font_name);
    }

    fn set_font_style(&mut self, style: &str) {
        self.state.borrow_mut().font_manager.set_font_style(style);
    }

    fn set_font_size(&mut self, size: f32) {
        self.state.borrow_mut().font_manager.set_font_size(size);
    }

    fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    /// Single-pass layout for breaking horizontal layouts (original algorithm).
    fn layout_single_pass(&self, new_width: i32, new_height: i32, padding: &Borders, typeface: &Typeface, scale: f64) {
        let mut xx = padding.left;
        let mut yy = padding.top;
        let max_x = new_width - padding.right;
        let mut max_height = 0;
        for v in self.views.iter() {
            let mut v = v.try_borrow_mut().unwrap();
            if v.get_visibility() == Visibility::Gone {
                continue;
            }
            let margins = v.get_margin(scale);
            v.layout_content(xx + margins.left, yy + margins.top, new_width - xx - padding.right, new_height - yy - padding.bottom, typeface, scale);
            let (w, h) = v.calculate_full_size(scale);
            match self.direction {
                Direction::Horizontal => xx = xx + w + margins.left + margins.right,
                Direction::Vertical => yy = yy + h + margins.top + margins.bottom
            }
            if xx > max_x {
                yy += max_height + margins.top;
                xx = padding.left + margins.left;
                v.layout_content(xx, yy + margins.top, new_width - xx - padding.right, new_height - yy - padding.bottom, typeface, scale);
                let (w, h) = v.calculate_full_size(scale);
                xx += w;
                max_height = h + margins.bottom;
            }
            if v.is_break() {
                let (_, h) = v.calculate_full_size(scale);
                xx = padding.left;
                yy += h + margins.bottom;
            }
            if h > max_height {
                max_height = h;
            }
        }
    }

    /// Two-pass layout: measures non-Max children first, then distributes remaining space to Max children.
    fn layout_two_pass(&self, new_width: i32, new_height: i32, padding: &Borders, typeface: &Typeface, scale: f64) {
        let is_vertical = self.direction == Direction::Vertical;

        let total_available = if is_vertical {
            new_height - padding.top - padding.bottom
        } else {
            new_width - padding.left - padding.right
        };

        // Pass 1: Measure non-Max children, count Max children
        let mut fixed_consumed: i32 = 0;
        let mut max_count: i32 = 0;
        let mut child_is_max: Vec<bool> = Vec::with_capacity(self.views.len());

        for v in self.views.iter() {
            let mut v = v.try_borrow_mut().unwrap();
            if v.get_visibility() == Visibility::Gone {
                child_is_max.push(false);
                continue;
            }
            let margins = v.get_margin(scale);
            let bounds = v.get_bounds();

            let is_max = if is_vertical {
                matches!(bounds.1, Dimension::Max)
            } else {
                matches!(bounds.0, Dimension::Max)
            };

            let (margin_before, margin_after) = if is_vertical {
                (margins.top, margins.bottom)
            } else {
                (margins.left, margins.right)
            };

            if is_max {
                max_count += 1;
                // Reserve space for margins only; the child's content space is computed later
                fixed_consumed += margin_before + margin_after;
            } else {
                // Layout at temporary position to measure size. Subtract the
                // child's own margins from the available area so wrapping
                // content (e.g. Labels) sizes itself within its content box,
                // not into the margin space — otherwise a long wrapped Label
                // can eat its own margin_right.
                v.layout_content(
                    padding.left + margins.left,
                    padding.top + margins.top,
                    new_width - padding.left - padding.right - margins.left - margins.right,
                    new_height - padding.top - padding.bottom - margins.top - margins.bottom,
                    typeface, scale
                );
                // Use the rect just set by layout_content — it honors configured
                // Dimensions (Dip/Percent), unlike calculate_full_size which
                // re-derives from raw content. Pass 2 advances cursor using
                // child_rect.height() too, so this keeps both passes consistent.
                let measured = v.get_rect();
                let size = if is_vertical { measured.height() } else { measured.width() };
                fixed_consumed += size + margin_before + margin_after;
            }
            child_is_max.push(is_max);
        }

        // Compute space for Max children (per_max excludes Max children's margins)
        let remaining = (total_available - fixed_consumed).max(0);
        let per_max = if max_count > 0 { remaining / max_count } else { 0 };
        let mut extra = if max_count > 0 { remaining % max_count } else { 0 };

        // When the parent shrinks to its content on the cross axis (Min), gravity
        // should align children inside the resolved content width — not the full
        // available width — otherwise a right-gravity child would expand the
        // parent to the available edge instead of sitting flush against the
        // longest sibling.
        let bounds = self.get_bounds();
        let cross_is_min = if is_vertical {
            matches!(bounds.0, Dimension::Min)
        } else {
            matches!(bounds.1, Dimension::Min)
        };
        let (effective_pw, effective_ph) = if cross_is_min {
            let mut max_extent = 0i32;
            for v in self.views.iter() {
                let v = v.try_borrow().unwrap();
                if v.get_visibility() == Visibility::Gone { continue; }
                let r = v.get_rect();
                let m = v.get_margin(scale);
                let extent = if is_vertical {
                    r.width() + m.left + m.right
                } else {
                    r.height() + m.top + m.bottom
                };
                if extent > max_extent { max_extent = extent; }
            }
            if is_vertical {
                let resolved = (padding.left + max_extent + padding.right).min(new_width);
                (resolved, new_height)
            } else {
                let resolved = (padding.top + max_extent + padding.bottom).min(new_height);
                (new_width, resolved)
            }
        } else {
            (new_width, new_height)
        };

        // When there are no Max children, leftover main-axis space goes before the
        // first child whose main-axis gravity points to the end (right in horizontal,
        // bottom in vertical), pushing it and following siblings against the end edge.
        let main_end_gap_at = if max_count == 0 && remaining > 0 {
            self.views.iter().enumerate().find_map(|(i, v)| {
                let vb = v.try_borrow().unwrap();
                if vb.get_visibility() == Visibility::Gone { return None; }
                let g = vb.get_gravity();
                let at_end = if is_vertical {
                    g.vertical() == VAlign::Bottom
                } else {
                    g.horizontal() == HAlign::Right
                };
                if at_end { Some(i) } else { None }
            })
        } else {
            None
        };

        // Pass 2: Layout Max children at final positions, move non-Max children
        let mut cursor = if is_vertical { padding.top } else { padding.left };

        for (i, v) in self.views.iter().enumerate() {
            let mut v = v.try_borrow_mut().unwrap();
            if v.get_visibility() == Visibility::Gone {
                continue;
            }
            if main_end_gap_at == Some(i) {
                cursor += remaining;
            }
            let margins = v.get_margin(scale);
            let is_max = child_is_max[i];

            let (margin_before, margin_after) = if is_vertical {
                (margins.top, margins.bottom)
            } else {
                (margins.left, margins.right)
            };

            if is_max {
                // per_max is the content space (margins already reserved in fixed_consumed).
                // layout_content's width/height param is "available space" — calculate_size
                // for Max subtracts margins internally, so pass per_max + margins.
                let mut slot = per_max;
                if extra > 0 {
                    slot += 1;
                    extra -= 1;
                }
                let avail = slot + margin_before + margin_after;

                if is_vertical {
                    v.layout_content(
                        padding.left + margins.left,
                        cursor + margins.top,
                        new_width - padding.left - padding.right,
                        avail,
                        typeface, scale
                    );
                } else {
                    v.layout_content(
                        cursor + margins.left,
                        padding.top + margins.top,
                        avail,
                        new_height - padding.top - padding.bottom,
                        typeface, scale
                    );
                }
                // Apply cross-axis gravity. The child's cross-axis size may be
                // smaller than the parent's (e.g. Label height=Min inside a
                // tall horizontal Frame); without this, gravity="center_vertical"
                // / "right" / "bottom" on a Max child has no effect. Recompute
                // the absolute target from the canonical anchor (cursor/padding)
                // each pass — some views (Label) cache layout and re-return their
                // last rect on subsequent layout_content calls, so reading the
                // current rect and adding an offset would compound on every relayout.
                let child_rect_now = v.get_rect();
                let cross_offset = cross_axis_offset(
                    v.get_gravity(),
                    is_vertical,
                    effective_pw, effective_ph,
                    padding, &margins,
                    child_rect_now.width(), child_rect_now.height()
                );
                let (anchor_x, anchor_y) = if is_vertical {
                    (padding.left + margins.left + cross_offset, cursor + margins.top)
                } else {
                    (cursor + margins.left, padding.top + margins.top + cross_offset)
                };
                if child_rect_now.min.x != anchor_x || child_rect_now.min.y != anchor_y {
                    let moved = rect(
                        (anchor_x, anchor_y),
                        (anchor_x + child_rect_now.width(), anchor_y + child_rect_now.height()),
                    );
                    v.set_rect(moved);
                }
                // Advance cursor by the child's actual rect size + margins
                let child_rect = v.get_rect();
                let size = if is_vertical { child_rect.height() } else { child_rect.width() };
                cursor += size + margin_before + margin_after;
            } else {
                // Move to correct final position (don't re-call layout_content,
                // as some views like Label cache their layout and skip re-layout)
                let old_rect = v.get_rect();
                let cross_offset = cross_axis_offset(
                    v.get_gravity(),
                    is_vertical,
                    effective_pw, effective_ph,
                    padding, &margins,
                    old_rect.width(), old_rect.height()
                );
                let (new_x, new_y) = if is_vertical {
                    (padding.left + margins.left + cross_offset, cursor + margins.top)
                } else {
                    (cursor + margins.left, padding.top + margins.top + cross_offset)
                };
                if old_rect.min.x != new_x || old_rect.min.y != new_y {
                    let moved = rect(
                        (new_x, new_y),
                        (new_x + old_rect.width(), new_y + old_rect.height())
                    );
                    v.set_rect(moved);
                }
                let size = if is_vertical { old_rect.height() } else { old_rect.width() };
                cursor += size + margin_before + margin_after;
            }
        }
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

    fn remove_view(&mut self, id: &str) -> bool {
        if let Some(pos) = self.views.iter().position(|v| v.borrow().get_id() == id) {
            self.views.remove(pos);
            return true;
        }
        for v in &self.views {
            let removed = if let Some(container) = v.borrow_mut().as_container_mut() {
                container.remove_view(id)
            } else {
                false
            };
            if removed {
                return true;
            }
        }
        false
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
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
            }
            "breaking" => { self.breaking = value.parse().unwrap_or(false) }
            "background_image" => { self.background_image_mut().set_path(value) }
            "background_image_opacity" => {
                if let Ok(o) = value.parse::<f32>() {
                    self.background_image_mut().opacity = o.clamp(0.0, 1.0);
                }
            }
            "background_repeat" => { self.background_image_mut().repeat = background::parse_repeat(value) }
            "background_position" => { self.background_image_mut().position = background::parse_position(value) }
            "background_size" => { self.background_image_mut().size = background::parse_size(value) }
            "background_origin" => { self.background_image_mut().origin = background::parse_origin(value) }
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
        let (new_width, new_height) = self.calculate_size(width, height, scale);

        let padding = self.get_padding(scale);
        let typeface = self.state.borrow().font_manager.get_typeface(typeface);

        if self.breaking && self.direction == Direction::Horizontal {
            self.layout_single_pass(new_width, new_height, &padding, &typeface, scale);
        } else {
            self.layout_two_pass(new_width, new_height, &padding, &typeface, scale);
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
        if let Some(bg) = self.background_image.borrow_mut().as_mut() {
            bg.paint(theme, rect, &state.padding.scaled(state.scale), state.scale);
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
            if v.get_visibility() != Visibility::Visible {
                continue;
            }
            let disabled = !v.is_enabled();
            if disabled {
                theme.push_opacity(0.5);
            }
            v.paint(start, theme);
            if disabled {
                theme.pop_opacity();
            }
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

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let scale = self.state.borrow().scale;
        let mut rect = rect((-1, -1), (0, 0));
        for v in self.views.iter() {
            let v = v.borrow();
            if v.get_visibility() == Visibility::Gone {
                continue;
            }
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
            if v.borrow().get_visibility() != Visibility::Visible {
                continue;
            }
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
            let vb = v.borrow();
            if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                continue;
            }
            processed |= vb.on_mouse_move(ui, Vector2::from(position));
        }
        processed
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        println!("Mouse down in {}", &self.state.borrow().id);
        let position = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        let focused;
        for v in self.views.iter().rev() {
            {
                let vb = v.borrow();
                if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                    continue;
                }
            }
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
            {
                let vb = v.borrow();
                if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                    continue;
                }
            }
            if v.borrow().on_mouse_button_up(ui, Vector2::from(position), button) {
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Vector2<i32>, distance: speedy2d::window::MouseScrollDistance) -> bool {
        let position = (position.x - self.state.borrow().rect.min.x, position.y - self.state.borrow().rect.min.y);
        for v in self.views.iter().rev() {
            {
                let vb = v.borrow();
                if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                    continue;
                }
            }
            if v.borrow().on_mouse_wheel_scroll(ui, Vector2::from(position), distance) {
                return true;
            }
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool {
        for v in self.views.iter() {
            {
                let vb = v.borrow();
                if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                    continue;
                }
            }
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
            {
                let vb = v.borrow();
                if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                    continue;
                }
            }
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
            {
                let vb = v.borrow();
                if vb.get_visibility() != Visibility::Visible || !vb.is_enabled() {
                    continue;
                }
            }
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