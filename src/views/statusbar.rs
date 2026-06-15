use std::cell::RefCell;

use crate::text::{TextBlock, TextOptions};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
use crate::view_base::{HasMainFields, ViewBasics, FontManager};

const DEFAULT_HEIGHT: u32 = 22;
const SECTION_PADDING_H: i32 = 4;
const SECTION_INSET: i32 = 2;

/// A section within the StatusBar, holding its text and cached layout.
struct Section {
    id: String,
    text: String,
    cached_text: Option<TextBlock>,
    width: Dimension,
}

pub struct StatusBar {
    state: RefCell<FieldsMain>,
    sections: Vec<Section>,
    font_manager: FontManager,
}

impl HasMainFields for StatusBar {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for StatusBar {}

#[allow(dead_code)]
impl StatusBar {
    pub fn new(rect: Rect<i32>, width: Dimension, height: Dimension) -> StatusBar {
        let mut main = FieldsMain::with_rect(rect, width, height);
        main.state.focusable = false;
        StatusBar {
            state: RefCell::new(main),
            sections: Vec::new(),
            font_manager: FontManager::new(),
        }
    }

    /// Add a new text section and return its index.
    pub fn add_section(&mut self, id: &str, text: &str) -> usize {
        let idx = self.sections.len();
        self.sections.push(Section {
            id: id.to_owned(),
            text: text.to_owned(),
            cached_text: None,
            width: Dimension::Min,
        });
        idx
    }

    /// Set the text of a section by its id.
    pub fn set_section_text(&mut self, id: &str, text: &str) {
        if let Some(section) = self.sections.iter_mut().find(|s| s.id == id) {
            section.text = text.to_owned();
            section.cached_text = None;
        }
    }

    /// Set the text of a section by index.
    pub fn set_section_text_by_index(&mut self, index: usize, text: &str) {
        if let Some(section) = self.sections.get_mut(index) {
            section.text = text.to_owned();
            section.cached_text = None;
        }
    }

    /// Set the width of a section by its id.
    pub fn set_section_width(&mut self, id: &str, width: Dimension) {
        if let Some(section) = self.sections.iter_mut().find(|s| s.id == id) {
            section.width = width;
        }
    }

    /// Get the number of sections.
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    fn set_font(&mut self, font_name: &str) {
        self.font_manager.set_font(font_name);
    }

    fn set_font_style(&mut self, style: &str) {
        self.font_manager.set_font_style(style);
    }

    fn set_font_size(&mut self, size: f32) {
        self.font_manager.set_font_size(size);
        self.invalidate_cache();
    }

    fn invalidate_cache(&mut self) {
        for section in &mut self.sections {
            section.cached_text = None;
        }
    }
}

impl View for StatusBar {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "font" => self.set_font(value),
            "font_style" => self.set_font_style(value),
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
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
        self.base_set_scale(scale);
        let (new_width, new_height) = self.calculate_size(width, height, scale);

        let typeface = self.font_manager.get_typeface(typeface);

        // Cache text for all sections
        let font = get_font_family(&typeface.font_name, typeface.font_style);
        let base_size = typeface.font_size.unwrap_or(14.0);
        let text_size = (base_size * scale as f32).round();
        for section in &mut self.sections {
            if let Some(ref f) = font {
                let text = f.layout_text(&section.text, text_size, TextOptions::new());
                section.cached_text = Some(text);
            }
        }

        let r = rect((x, y), (x + new_width, y + new_height));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, scale: f64) -> bool {
        let size = self.calculate_full_size(scale);
        size.0 <= width && size.1 <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);

        theme.push_clip();
        theme.clip_rect(r);

        // Draw background
        match state.background.as_ref().and_then(|bg| bg.get_state(&state.state)) {
            Some(crate::styles::selector::DrawState::Color(c)) => theme.draw_rect(r, *c),
            Some(crate::styles::selector::DrawState::Token(t)) => {
                let c = theme.color(t);
                theme.draw_rect(r, c);
            }
            _ => theme.draw_component("panel.back", r, state.state),
        }

        let view_state = state.state;
        drop(state);

        let scale = self.state.borrow().scale;
        let pad_h = (SECTION_PADDING_H as f64 * scale).round() as i32;
        let inset = (SECTION_INSET as f64 * scale).round() as i32;
        let gap = (2.0 * scale).round() as i32;
        let new_width = r.width();

        // Compute section widths (including inset borders)
        let inset_overhead = inset * 2; // left + right inset border
        let total_gaps = gap * (self.sections.len() as i32 + 1); // gaps between and around sections
        let mut fixed_consumed = total_gaps;
        let mut max_count = 0i32;

        let mut section_widths: Vec<i32> = Vec::with_capacity(self.sections.len());
        for section in &self.sections {
            match section.width {
                Dimension::Max => {
                    max_count += 1;
                    fixed_consumed += inset_overhead;
                    section_widths.push(-1); // placeholder
                }
                Dimension::Dip(dip) => {
                    let w = (dip as f64 * scale).round() as i32;
                    fixed_consumed += w + inset_overhead;
                    section_widths.push(w);
                }
                Dimension::Percent(p) => {
                    let w = (new_width as f32 * p / 100.0).round() as i32;
                    fixed_consumed += w + inset_overhead;
                    section_widths.push(w);
                }
                Dimension::Min => {
                    let text_w = section.cached_text.as_ref().map(|t| t.width().ceil() as i32).unwrap_or(0);
                    let w = text_w + pad_h * 2;
                    fixed_consumed += w + inset_overhead;
                    section_widths.push(w);
                }
            }
        }

        let remaining = (new_width - fixed_consumed).max(0);
        let per_max = if max_count > 0 { remaining / max_count } else { 0 };
        let mut extra = if max_count > 0 { remaining % max_count } else { 0 };

        // Fill in Max widths
        for (i, section) in self.sections.iter().enumerate() {
            if matches!(section.width, Dimension::Max) {
                let mut w = per_max + pad_h * 2;
                if extra > 0 {
                    w += 1;
                    extra -= 1;
                }
                section_widths[i] = w;
            }
        }

        // Draw sections with inset borders
        let section_top = r.min.y + inset;
        let section_bottom = r.max.y - inset;
        let last_index = self.sections.len().saturating_sub(1);
        let mut xx = r.min.x + gap;
        for (i, section) in self.sections.iter().enumerate() {
            let sx = xx;
            // Last section always extends to the right edge
            let sx_end = if i == last_index {
                r.max.x - gap
            } else {
                xx + section_widths[i] + inset_overhead
            };

            // Sunken inset: shadow on top & left, highlight on bottom & right
            // Top shadow
            theme.draw_rect(rect((sx, section_top), (sx_end, section_top + 1)), theme.color("border_light"));
            // Left shadow
            theme.draw_rect(rect((sx, section_top), (sx + 1, section_bottom)), theme.color("border_light"));
            // Bottom highlight
            theme.draw_rect(rect((sx, section_bottom - 1), (sx_end, section_bottom)), theme.color("highlight"));
            // Right highlight
            theme.draw_rect(rect((sx_end - 1, section_top), (sx_end, section_bottom)), theme.color("highlight"));

            // Draw text centered vertically within the inset area
            if let Some(ref cached) = section.cached_text {
                let text_color = theme.get_text_color(view_state, None);
                let inner_top = (section_top + 1) as f32;
                let inner_height = (section_bottom - section_top - 2) as f32;
                let text_y = inner_top + (inner_height - cached.height()) / 2.0;
                let text_x = (sx + 1 + pad_h) as f32;
                theme.draw_text(text_x, text_y.round(), text_color, cached);
            }

            xx = sx_end + gap;
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
        let state = self.state.borrow();
        let scale = state.scale;
        let pad_h = (SECTION_PADDING_H as f64 * scale).round() as i32;

        let configured_w = match state.width {
            Dimension::Dip(d) => Some((d as f64 * scale).round() as i32),
            _ => None,
        };
        let configured_h = match state.height {
            Dimension::Dip(d) => Some((d as f64 * scale).round() as i32),
            _ => None,
        };
        drop(state);

        let mut total_width = 0i32;
        let mut max_height = 0i32;
        for (i, section) in self.sections.iter().enumerate() {
            if i > 0 {
                total_width += 1; // separator
            }
            let text_w = section.cached_text.as_ref().map(|t| t.width().ceil() as i32).unwrap_or(0);
            let text_h = section.cached_text.as_ref().map(|t| t.height().ceil() as i32).unwrap_or(0);
            total_width += text_w + pad_h * 2;
            if text_h > max_height {
                max_height = text_h;
            }
        }

        let width = configured_w.unwrap_or(total_width);
        let height = configured_h.unwrap_or_else(|| {
            if max_height > 0 { max_height } else { (DEFAULT_HEIGHT as f64 * self.state.borrow().scale).round() as i32 }
        });
        (width, height)
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
}

impl Default for StatusBar {
    fn default() -> Self {
        let rect = rect((0, 0), (0, DEFAULT_HEIGHT as i32));
        StatusBar::new(rect, Dimension::Max, Dimension::Dip(DEFAULT_HEIGHT))
    }
}
