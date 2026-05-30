use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use speedy2d::dimen::Vector2;
use speedy2d::font::{TextAlignment, TextLayout, TextOptions};
use speedy2d::window::MouseButton;
use crate::assets::{get_asset, get_font_family};
use crate::events::EventType;
use crate::svg;

use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Gravity, Visibility};
use crate::styles::selector::FontSelector;
use crate::views::{FieldsMain, FieldsTexted};
use crate::view_base::{HasMainFields, ViewBasics, parse_hex_color};

const DEFAULT_LINK_COLOR: u32 = 0xFF3273DC;
const DEFAULT_ICON_TINT: u32 = 0xFFFFFFFF;
const ICON_GAP_DIP: i32 = 2;

pub struct Label {
    state: RefCell<FieldsTexted>,
    /// When true, render as a hyperlink: link-coloured text + underline; the
    /// view becomes focusable and dispatches `EventType::Click`.
    link: RefCell<bool>,
    link_color: RefCell<u32>,
    /// Tracks press-down on the label so `on_mouse_button_up` only fires the
    /// click when release lands on the label too (drag-off cancels).
    pressed: RefCell<bool>,
    /// Optional rounded-rectangle background fill drawn before the text.
    background_color: RefCell<Option<u32>>,
    /// Optional override for the text colour. Wins over both the link colour
    /// (when `link=true`) and the theme default.
    text_color: RefCell<Option<u32>>,
    /// Corner radius (dip) for the background fill. 0 = square.
    corner_radius: RefCell<i32>,
    // Leading/trailing icons + tint. Same loader/draw pattern as `Edit`.
    left_icon_path: RefCell<String>,
    left_icon_bytes: RefCell<Option<Vec<u8>>>,
    left_icon_is_svg: RefCell<bool>,
    left_icon_rasterized: RefCell<Option<(u32, u32, Vec<u8>)>>,
    right_icon_path: RefCell<String>,
    right_icon_bytes: RefCell<Option<Vec<u8>>>,
    right_icon_is_svg: RefCell<bool>,
    right_icon_rasterized: RefCell<Option<(u32, u32, Vec<u8>)>>,
    icon_tint: RefCell<u32>,
    /// Track which icon (if any) absorbed the most recent mouse-down, so the
    /// click only fires on mouse-up if the release lands over the same icon.
    pressed_icon: RefCell<Option<bool>>, // Some(true)=left, Some(false)=right
    /// Width / height / scale params used the last time `layout_content` ran.
    /// `layout_content` returns the cached rect when these match (skipping the
    /// expensive font shaping); when they differ we re-layout. Resolves the
    /// "Label doesn't reflow on parent resize" bug.
    last_layout_params: std::cell::Cell<Option<(i32, i32, f64)>>,
}

impl HasMainFields for Label {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for Label {}

#[allow(dead_code)]
impl Label {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> Label {
        let mut main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        main.state.focusable = false;
        Label {
            state: RefCell::new(FieldsTexted {
                main,
                text: text.to_owned(),
                text_size,
                line_height: 0f32,
                single_line: false,
                cached_text: None,
                font: FontSelector::new(),
                listeners: HashMap::new()
            }),
            link: RefCell::new(false),
            link_color: RefCell::new(DEFAULT_LINK_COLOR),
            pressed: RefCell::new(false),
            background_color: RefCell::new(None),
            text_color: RefCell::new(None),
            corner_radius: RefCell::new(0),
            left_icon_path: RefCell::new(String::new()),
            left_icon_bytes: RefCell::new(None),
            left_icon_is_svg: RefCell::new(false),
            left_icon_rasterized: RefCell::new(None),
            right_icon_path: RefCell::new(String::new()),
            right_icon_bytes: RefCell::new(None),
            right_icon_is_svg: RefCell::new(false),
            right_icon_rasterized: RefCell::new(None),
            icon_tint: RefCell::new(DEFAULT_ICON_TINT),
            pressed_icon: RefCell::new(None),
            last_layout_params: std::cell::Cell::new(None),
        }
    }

    pub fn set_background_color(&self, color: Option<u32>) {
        *self.background_color.borrow_mut() = color;
    }

    pub fn set_text_color(&self, color: Option<u32>) {
        *self.text_color.borrow_mut() = color;
    }

    pub fn set_corner_radius(&self, radius: i32) {
        *self.corner_radius.borrow_mut() = radius;
    }

    pub fn set_left_icon(&self, path: &str) {
        *self.left_icon_path.borrow_mut() = path.to_owned();
        *self.left_icon_bytes.borrow_mut() = None;
        *self.left_icon_is_svg.borrow_mut() = false;
        *self.left_icon_rasterized.borrow_mut() = None;
        self.state.borrow_mut().cached_text = None;
    }

    pub fn set_right_icon(&self, path: &str) {
        *self.right_icon_path.borrow_mut() = path.to_owned();
        *self.right_icon_bytes.borrow_mut() = None;
        *self.right_icon_is_svg.borrow_mut() = false;
        *self.right_icon_rasterized.borrow_mut() = None;
        self.state.borrow_mut().cached_text = None;
    }

    pub fn set_icon_tint(&self, tint: u32) {
        *self.icon_tint.borrow_mut() = tint;
    }

    /// `&self` visibility setter. Safe to call from inside an event handler
    /// firing from this same view — the trait-level `set_visibility(&mut self)`
    /// would deadlock because the dispatcher already holds `element.borrow()`.
    pub fn hide(&self) {
        self.base_set_visibility(Visibility::Gone);
    }

    pub fn show(&self) {
        self.base_set_visibility(Visibility::Visible);
    }

    fn load_icon(path: &RefCell<String>, bytes: &RefCell<Option<Vec<u8>>>, is_svg: &RefCell<bool>) {
        if bytes.borrow().is_some() {
            return;
        }
        let p = path.borrow().clone();
        if p.is_empty() {
            return;
        }
        if let Some(data) = get_asset(&p) {
            let svg_flag = p.to_ascii_lowercase().ends_with(".svg") || svg::looks_like_svg(&data);
            *is_svg.borrow_mut() = svg_flag;
            *bytes.borrow_mut() = Some(data);
        }
    }

    fn load_icons(&self) {
        Self::load_icon(&self.left_icon_path, &self.left_icon_bytes, &self.left_icon_is_svg);
        Self::load_icon(&self.right_icon_path, &self.right_icon_bytes, &self.right_icon_is_svg);
    }

    /// Width in pixels reserved by an icon side; 0 when no icon is set.
    fn icon_side_width(has_icon: bool, inner_height: i32, scale: f64) -> i32 {
        if !has_icon || inner_height <= 0 {
            return 0;
        }
        inner_height + (ICON_GAP_DIP as f64 * scale).round() as i32
    }

    /// (left_inset, right_inset) in pixels, given inner (post-padding) height.
    fn icon_insets(&self, inner_height: i32, scale: f64) -> (i32, i32) {
        let has_left = !self.left_icon_path.borrow().is_empty();
        let has_right = !self.right_icon_path.borrow().is_empty();
        (
            Self::icon_side_width(has_left, inner_height, scale),
            Self::icon_side_width(has_right, inner_height, scale),
        )
    }

    /// Icon hit rectangles in the same coord system as `state.main.rect` (pre-origin).
    fn icon_hit_rects(&self) -> (Option<Rect<i32>>, Option<Rect<i32>>) {
        let scale = self.state.borrow().main.scale;
        let padding = self.get_padding(scale);
        let my_rect = self.state.borrow().main.rect;
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        if inner_h <= 0 {
            return (None, None);
        }
        let icon_size = inner_h;
        let inner_top = my_rect.min.y + padding.top;
        let has_left = !self.left_icon_path.borrow().is_empty();
        let has_right = !self.right_icon_path.borrow().is_empty();
        let left = if has_left {
            let x = my_rect.min.x + padding.left;
            Some(crate::types::rect((x, inner_top), (x + icon_size, inner_top + icon_size)))
        } else {
            None
        };
        let right = if has_right {
            let x = my_rect.max.x - padding.right - icon_size;
            Some(crate::types::rect((x, inner_top), (x + icon_size, inner_top + icon_size)))
        } else {
            None
        };
        (left, right)
    }

    fn fire_icon_event(&self, ui: &mut UI, event: EventType) {
        let handler = self.state.borrow_mut().listeners.remove(&event);
        if let Some(mut handler) = handler {
            handler(ui, self as &dyn View);
            self.state.borrow_mut().listeners.insert(event, handler);
        }
    }

    fn draw_icon(&self, theme: &mut dyn Theme, icon_rect: Rect<i32>, is_left: bool, tint: u32) {
        let (path_cell, bytes_cell, is_svg_cell, raster_cell) = if is_left {
            (&self.left_icon_path, &self.left_icon_bytes, &self.left_icon_is_svg, &self.left_icon_rasterized)
        } else {
            (&self.right_icon_path, &self.right_icon_bytes, &self.right_icon_is_svg, &self.right_icon_rasterized)
        };
        if bytes_cell.borrow().is_none() {
            return;
        }
        let w = icon_rect.width().max(0) as u32;
        let h = icon_rect.height().max(0) as u32;
        if w == 0 || h == 0 {
            return;
        }
        if *is_svg_cell.borrow() {
            let needs_render = match &*raster_cell.borrow() {
                Some((cw, ch, _)) => *cw != w || *ch != h,
                None => true,
            };
            if needs_render {
                let src_opt = bytes_cell.borrow().clone();
                if let Some(src) = src_opt {
                    if let Some(rgba) = svg::rasterize(&src, w, h) {
                        *raster_cell.borrow_mut() = Some((w, h, rgba));
                    }
                }
            }
            if let Some((cw, ch, rgba)) = &*raster_cell.borrow() {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                path_cell.borrow().hash(&mut hasher);
                cw.hash(&mut hasher);
                ch.hash(&mut hasher);
                let key = hasher.finish();
                theme.draw_raw_image_tinted(icon_rect, rgba, (*cw, *ch), key, tint);
            }
        } else {
            let bytes_opt = bytes_cell.borrow().clone();
            if let Some(bytes) = bytes_opt {
                theme.draw_image_tinted(icon_rect, &bytes, tint);
            }
        }
    }

    pub fn set_link(&self, link: bool) {
        *self.link.borrow_mut() = link;
        // Links accept hover/press input.
        self.state.borrow_mut().main.state.focusable = link;
        // Resizing depends on link state — invalidate the cache so the next
        // paint/layout recomputes with the underline reservation included.
        self.state.borrow_mut().cached_text = None;
    }

    pub fn is_link(&self) -> bool {
        *self.link.borrow()
    }

    /// Extra vertical pixels reserved below the text for the link underline.
    /// Currently zero — the underline is drawn inside the text bounding box
    /// (sits in the descender row), so no extra space is needed.
    fn link_extra_v(&self, _scale: f64) -> i32 {
        0
    }

    fn rebuild_text(&self) {
        let state = self.state.borrow();
        let typeface = state.main.font_manager.get_typeface(&Typeface::default());
        let font = match get_font_family(&typeface.font_name, typeface.font_style) {
            Some(f) => f,
            None => return,
        };
        let scale = state.main.scale;
        let padding = &state.main.padding;
        let pad_h = (padding.left as f64 * scale).round() as i32 + (padding.right as f64 * scale).round() as i32;
        let base_size = typeface.font_size
            .map(|dip| dip * scale as f32)
            .unwrap_or(state.text_size);
        // Reserve icon space using a font-height estimate so wrap-to-width
        // accounts for icons before the text is laid out.
        let has_left = !self.left_icon_path.borrow().is_empty();
        let has_right = !self.right_icon_path.borrow().is_empty();
        let est_icon = base_size.ceil() as i32;
        let gap = (ICON_GAP_DIP as f64 * scale).round() as i32;
        let icon_reserve = (if has_left { est_icon + gap } else { 0 })
            + (if has_right { est_icon + gap } else { 0 });
        // Use parent width for wrapping when label is width=Min
        let available_width = if matches!(state.main.width, Dimension::Min) {
            if let Some(parent) = state.main.parent.as_ref().and_then(|w| w.upgrade()) {
                let parent_rect = parent.borrow().get_rect();
                let label_x = state.main.rect.min.x;
                (parent_rect.max.x - label_x - pad_h - icon_reserve).max(0)
            } else {
                (state.main.rect.width() - pad_h - icon_reserve).max(0)
            }
        } else {
            (state.main.rect.width() - pad_h - icon_reserve).max(0)
        };
        let options = match state.single_line {
            true => TextOptions::new(),
            false => TextOptions::new().with_wrap_to_width(available_width as f32, TextAlignment::Left),
        };
        let text = font.layout_text(&state.text, base_size, options);
        // After layout, recompute icon reservation using actual text height so
        // the final rect snaps tight; insets used by paint/hit-test derive
        // from this same text height via `icon_insets`.
        let actual_icon = if has_left || has_right { text.height().ceil() as i32 } else { 0 };
        let actual_reserve = (if has_left { actual_icon + gap } else { 0 })
            + (if has_right { actual_icon + gap } else { 0 });
        // Update rect to fit new text
        let new_width = text.width().ceil() as i32 + pad_h + actual_reserve;
        let pad_v = (padding.top as f64 * scale).round() as i32 + (padding.bottom as f64 * scale).round() as i32;
        let new_height = text.height().ceil() as i32 + pad_v + self.link_extra_v(scale);
        drop(state);
        let mut state = self.state.borrow_mut();
        if matches!(state.main.width, Dimension::Min) {
            state.main.rect.max.x = state.main.rect.min.x + new_width;
        }
        if matches!(state.main.height, Dimension::Min) {
            state.main.rect.max.y = state.main.rect.min.y + new_height;
        }
        state.cached_text = Some(text);
    }

    pub fn set_text(&mut self, text: &str) {
        let mut state = self.state.borrow_mut();
        state.text.clear();
        state.text.push_str(text);
        let _ = state.cached_text.take();
    }

    pub fn set_single_line(&self, single_line: bool) {
        let mut state = self.state.borrow_mut();
        state.single_line = single_line;
        state.cached_text = None;
    }

    fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        self.state.borrow().main.font_manager.get_typeface(parent_typeface)
    }

    fn set_font(&self, font_name: &str) {
        self.state.borrow_mut().main.font_manager.set_font(font_name);
    }

    fn set_font_style(&self, style: &str) {
        self.state.borrow_mut().main.font_manager.set_font_style(style);
    }

    fn set_font_size(&self, size: f32) {
        let mut state = self.state.borrow_mut();
        state.main.font_manager.set_font_size(size);
        state.cached_text = None;
    }
}

impl View for Label {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "text" => { self.set_text(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
            }
            "single_line" => { self.state.borrow_mut().single_line = value.parse().unwrap_or(false) }
            "link" => { self.set_link(value == "true") }
            "link_color" => {
                if let Some(c) = parse_hex_color(value) {
                    *self.link_color.borrow_mut() = c;
                }
            }
            "background_color" => {
                self.set_background_color(parse_hex_color(value));
            }
            "text_color" => {
                self.set_text_color(parse_hex_color(value));
            }
            "corner_radius" => {
                if let Ok(r) = value.parse::<i32>() {
                    self.set_corner_radius(r);
                }
            }
            "left_icon" => { self.set_left_icon(value); }
            "right_icon" => { self.set_right_icon(value); }
            "icon_tint" => {
                if let Some(c) = parse_hex_color(value) {
                    self.set_icon_tint(c);
                }
            }
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
        // Skip the (expensive) font shaping if nothing relevant has changed.
        // We re-layout when (width, height, scale) differ from the previous
        // call OR when the text was invalidated (cached_text is None).
        let params = (width, height, scale);
        if self.state.borrow().cached_text.is_some()
            && self.last_layout_params.get() == Some(params)
        {
            return self.get_rect();
        }
        self.last_layout_params.set(Some(params));
        self.state.borrow_mut().cached_text = None;

        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let (new_width, new_height) = self.calculate_size(width - horizontal, height - vertical, scale);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface.clone()));
        let base_size = typeface.font_size
            .map(|dip| dip * scale as f32)
            .unwrap_or(self.state.borrow().text_size);
        let has_left = !self.left_icon_path.borrow().is_empty();
        let has_right = !self.right_icon_path.borrow().is_empty();
        let est_icon = base_size.ceil() as i32;
        let gap = (ICON_GAP_DIP as f64 * scale).round() as i32;
        let icon_reserve = (if has_left { est_icon + gap } else { 0 })
            + (if has_right { est_icon + gap } else { 0 });
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            let single_line = self.state.borrow().single_line;
            let wrap_w = (new_width - icon_reserve).max(0);
            let options = match single_line {
                true => TextOptions::new(),
                false => TextOptions::new().with_wrap_to_width(wrap_w as f32, TextAlignment::Left),
            };
            let text = font.layout_text(&self.state.borrow().text, base_size, options);
            self.state.borrow_mut().cached_text = Some(text);
        }
        let (content_width, content_height) = self.calculate_full_size(scale);
        let (b_width, b_height) = self.get_bounds();
        let final_width = match b_width {
            Dimension::Min => content_width,
            _ => new_width + horizontal,
        };
        let final_height = match b_height {
            Dimension::Min => content_height,
            _ => new_height + vertical,
        };
        let rect = rect((x, y), (x + final_width, y + final_height));
        self.set_rect(rect.clone());
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let state = self.state.borrow();
        match &state.cached_text {
            Some(text) => text.width() <= width as f32 && text.height() <= height as f32,
            None => true
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        // Rebuild cached text if it was invalidated (e.g. by set_text)
        if self.state.borrow().cached_text.is_none() {
            self.rebuild_text();
        }
        // Lazy-load icon assets on first paint.
        self.load_icons();
        let state = self.state.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);
        let scale = state.main.scale;
        theme.push_clip();
        theme.clip_rect(rect);

        // Background fill (rounded if corner_radius > 0).
        if let Some(bg) = *self.background_color.borrow() {
            let radius = ((*self.corner_radius.borrow() as f64) * scale).round() as i32;
            theme.draw_rounded_rect(rect, bg, radius);
        }

        // Icon insets — same coordinate convention as Edit.
        let padding = state.main.padding.scaled(scale);
        let inner_h = rect.height() - padding.top - padding.bottom;
        let (left_inset, right_inset) = self.icon_insets(inner_h, scale);

        if let Some(text) = &state.cached_text {
            let is_link = *self.link.borrow();
            let has_bg = self.background_color.borrow().is_some();
            // Colour precedence: explicit text_color > link_color (if link) > theme default.
            let color = if let Some(c) = *self.text_color.borrow() {
                c
            } else if is_link {
                *self.link_color.borrow()
            } else {
                theme.get_text_color(state.main.state, state.main.foreground.as_ref())
            };
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let text_x = (rect.min.x + padding.left + left_inset) as f32;
            let text_y = (rect.min.y as f32 + y).round();
            theme.draw_text(text_x, text_y, color, text);
            // Underline: only when link mode is on AND there's no background
            // (a filled chip with an underlined word looks busy).
            if is_link && !has_bg {
                let line_h = ((1.0 * scale).round() as i32).max(1);
                let underline_bottom = (text_y + text.height()).round() as i32;
                let text_w = text.width().ceil() as i32;
                let underline = crate::types::rect(
                    (text_x.round() as i32, underline_bottom - line_h),
                    (text_x.round() as i32 + text_w, underline_bottom),
                );
                theme.draw_rect(underline, color);
            }
        }

        // Icons (drawn after text so their square hit area sits over the
        // padded reservation rather than under any background-colour fill).
        let tint = *self.icon_tint.borrow();
        if inner_h > 0 {
            let icon_size = inner_h;
            let inner_top = rect.min.y + padding.top;
            if left_inset > 0 {
                let icon_x = rect.min.x + padding.left;
                let icon_rect = crate::types::rect(
                    (icon_x, inner_top),
                    (icon_x + icon_size, inner_top + icon_size),
                );
                self.draw_icon(theme, icon_rect, true, tint);
            }
            if right_inset > 0 {
                let icon_x = rect.max.x - padding.right - icon_size;
                let icon_rect = crate::types::rect(
                    (icon_x, inner_top),
                    (icon_x + icon_size, inner_top + icon_size),
                );
                self.draw_icon(theme, icon_rect, false, tint);
            }
        }

        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().main.state)
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
        let scale = self.state.borrow().main.scale;
        let extra_v = self.link_extra_v(scale);
        let has_left = !self.left_icon_path.borrow().is_empty();
        let has_right = !self.right_icon_path.borrow().is_empty();
        let gap = (ICON_GAP_DIP as f64 * scale).round() as i32;
        let state = self.state.borrow();
        match &state.cached_text {
            None => (0, extra_v),
            Some(text) => {
                let icon = if has_left || has_right { text.height().ceil() as i32 } else { 0 };
                let icon_reserve = (if has_left { icon + gap } else { 0 })
                    + (if has_right { icon + gap } else { 0 });
                let width = text.width().round() as i32 + icon_reserve;
                let height = text.height().round() as i32 + extra_v;
                (width, height)
            }
        }
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

    fn on_event(&mut self, event: EventType, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        self.state.borrow_mut().listeners.insert(event, func);
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.fire_click(ui)
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        if !*self.link.borrow() { return false; }
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        let old_state = self.state.borrow().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow().main.state != old_state
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !matches!(button, MouseButton::Left) { return false; }
        // Icon clicks work even when the label isn't a link — capture the press
        // and route to LeftIconClick / RightIconClick on mouse-up.
        let (left_rect, right_rect) = self.icon_hit_rects();
        if let Some(r) = left_rect {
            if r.hit((position.x, position.y)) {
                *self.pressed_icon.borrow_mut() = Some(true);
                return true;
            }
        }
        if let Some(r) = right_rect {
            if r.hit((position.x, position.y)) {
                *self.pressed_icon.borrow_mut() = Some(false);
                return true;
            }
        }
        if !*self.link.borrow() { return false; }
        if self.state.borrow().main.rect.hit((position.x, position.y)) {
            *self.pressed.borrow_mut() = true;
            self.state.borrow_mut().main.state.pressed = true;
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !matches!(button, MouseButton::Left) { return false; }
        let pressed_icon = self.pressed_icon.borrow_mut().take();
        if let Some(was_left) = pressed_icon {
            let (left_rect, right_rect) = self.icon_hit_rects();
            let target = if was_left { left_rect } else { right_rect };
            if let Some(r) = target {
                if r.hit((position.x, position.y)) {
                    let event = if was_left { EventType::LeftIconClick } else { EventType::RightIconClick };
                    self.fire_icon_event(ui, event);
                }
            }
            return true;
        }
        if !*self.link.borrow() { return false; }
        let was_pressed = *self.pressed.borrow();
        *self.pressed.borrow_mut() = false;
        self.state.borrow_mut().main.state.pressed = false;
        if was_pressed && self.state.borrow().main.rect.hit((position.x, position.y)) {
            self.fire_click(ui);
            return true;
        }
        false
    }

}

impl Default for Label {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        Label::new(rect, "", 48_f32)
    }
}

impl Label {
    fn fire_click(&self, ui: &mut UI) -> bool {
        let handler = self.state.borrow_mut().listeners.remove(&EventType::Click);
        if let Some(mut handler) = handler {
            let result = handler(ui, self as &dyn View);
            self.state.borrow_mut().listeners.insert(EventType::Click, handler);
            return result;
        }
        false
    }
}
