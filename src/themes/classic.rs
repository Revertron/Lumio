use std::cmp::{max, min};
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use speedy2d::color::Color;
use speedy2d::dimen::Vector2;
use speedy2d::font::FormattedTextBlock;
use speedy2d::dimen::UVec2;
use speedy2d::image::{ImageDataType, ImageHandle, ImageSmoothingMode};
use speedy2d::Graphics2D;
use speedy2d::shape::Rectangle;
use super::super::styles::selector::{DrawState, MainSelector};
use super::super::themes::{Theme, Typeface, ViewState};
use super::super::themes::utils::draw_dashed_rectangle;
use super::super::types::Rect;
use super::super::types;
use super::super::types::rect;
use super::super::drawing::{Drawable, DrawableRegistry, DrawingEngine};
use super::super::views::Direction;

/// Cache for GPU image handles, keyed by the raw pointer of the source byte slice.
pub type ImageCache = HashMap<usize, ImageHandle>;

#[allow(unused)]
pub struct Classic<'h> {
    graphics: &'h mut Graphics2D,
    width: i32,
    height: i32,
    scale: f64,
    current_clip: Rect<i32>,
    clip_stack: VecDeque<Rect<i32>>,
    opacity_stack: Vec<f32>,
    drawable_registry: &'h DrawableRegistry,
    image_cache: &'h mut ImageCache,
}

#[allow(dead_code)]
impl<'h> Classic<'h> {
    const BACKGROUND: u32 = 0xffd4d0c8;
    const BACKGROUND_LIGHT: u32 = 0xffe4e0d8;
    const BACKGROUND_WHITE: u32 = 0xffffffff;
    const LIGHT: u32 = 0xff808080;
    const DARK: u32 = 0xff404040;
    const BLACK: u32 = 0xff000000;

    fn current_opacity(&self) -> f32 {
        self.opacity_stack.last().copied().unwrap_or(1.0)
    }

    fn apply_color(&self, color: Color) -> Color {
        let opacity = self.current_opacity();
        if opacity >= 1.0 {
            return color;
        }
        Color::from_rgba(color.r(), color.g(), color.b(), color.a() * opacity)
    }

    fn color_rgb(&self, hex: u32) -> Color {
        self.apply_color(Color::from_hex_rgb(hex))
    }

    fn color_argb(&self, hex: u32) -> Color {
        self.apply_color(Color::from_hex_argb(hex))
    }

    pub fn new(graphics: &'h mut Graphics2D, drawable_registry: &'h DrawableRegistry, image_cache: &'h mut ImageCache, width: i32, height: i32, scale: f64) -> Self {
        let current_clip = rect((0, 0), (width, height));
        Classic {
            graphics,
            width,
            height,
            scale,
            current_clip,
            clip_stack: VecDeque::new(),
            opacity_stack: Vec::new(),
            drawable_registry,
            image_cache,
        }
    }
}

impl<'h> Theme for Classic<'h> {
    fn clear_screen(&mut self) {
        self.graphics.set_clip(None);
        self.graphics.clear_screen(Color::from_hex_rgb(Classic::BACKGROUND));
        self.set_clip(self.current_clip);
    }

    fn typeface() -> Typeface {
        Typeface::default()
    }

    fn get_back_color(&self, state: ViewState, selector: Option<&MainSelector>) -> u32 {
        if let Some(selector) = selector {
            if let Some(s) = selector.get_state(&state) {
                match s {
                    DrawState::Transparent => return 0x00000000,
                    DrawState::Color(c) => return *c,
                    _ => {}
                }
            }
        }
        Classic::BACKGROUND
    }

    fn get_text_color(&self, state: ViewState, selector: Option<&MainSelector>) -> u32 {
        if let Some(selector) = selector {
            if let Some(s) = selector.get_state(&state) {
                match s {
                    DrawState::Transparent => return 0x00000000,
                    DrawState::Color(c) => return *c,
                    _ => {}
                }
            }
        }
        0xff000000
    }

    fn set_clip(&mut self, rect: Rect<i32>) {
        self.current_clip = rect;
        let rect = Rectangle::from_tuples((rect.min.x, rect.min.y), (rect.max.x, rect.max.y));
        self.graphics.set_clip(Some(rect));
    }

    fn clip_rect(&mut self, rect: Rect<i32>) -> Rect<i32> {
        let min_x = max(rect.min.x, self.current_clip.min.x);
        let max_x = min(rect.max.x, self.current_clip.max.x);
        let min_y = max(rect.min.y, self.current_clip.min.y);
        let max_y = min(rect.max.y, self.current_clip.max.y);
        let rect = types::rect((min_x, min_y), (max_x, max_y));
        self.set_clip(rect);
        rect
    }

    fn push_clip(&mut self) {
        self.clip_stack.push_back(self.current_clip);
    }

    fn pop_clip(&mut self) {
        if let Some(clip) = self.clip_stack.pop_back() {
            self.set_clip(clip);
        }
    }

    #[allow(unused)]
    fn draw_button_back(&mut self, rect: Rect<i32>, state: ViewState) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = if state.hovered || state.pressed {
            self.color_rgb(Classic::BACKGROUND_LIGHT)
        } else {
            self.color_rgb(Classic::BACKGROUND)
        };
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    #[allow(unused)]
    fn draw_button_body(&mut self, rect: Rect<i32>, state: ViewState) {
        let border: f32 = self.scale as f32;
        let border_half: f32 = (self.scale / 2f64) as f32;
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        match state.pressed && state.hovered {
            true => {
                let border2: f32 = (self.scale * 2f64) as f32;
                let color = self.color_rgb(Classic::LIGHT);
                self.graphics.draw_line((top_left.x, top_left.y + border_half), (bottom_right.x - border, top_left.y + border_half), border, color);
                self.graphics.draw_line((top_left.x + border_half, top_left.y), (top_left.x + border_half, bottom_right.y - border), border, color);
                let color = self.color_rgb(Classic::DARK);
                self.graphics.draw_line((top_left.x + border, top_left.y + border + border_half), (bottom_right.x - border, top_left.y + border + border_half), border, color);
                self.graphics.draw_line((top_left.x + border + border_half, top_left.y + border), (top_left.x + border + border_half, bottom_right.y - border), border, color);

                let color = self.color_rgb(0xffffff);
                self.graphics.draw_line((top_left.x + border, bottom_right.y - border - border_half), (bottom_right.x - border, bottom_right.y - border - border_half), border, color);
                self.graphics.draw_line((bottom_right.x - border - border_half, top_left.y + border), (bottom_right.x - border - border_half, bottom_right.y - border), border, color);
            }
            false => {
                let color = self.color_rgb(0xffffff);
                self.graphics.draw_line((top_left.x, top_left.y + border_half), (bottom_right.x - border_half, top_left.y + border_half), border, color);
                self.graphics.draw_line((top_left.x + border_half, top_left.y + border_half), (top_left.x + border_half, bottom_right.y - border_half), border, color);
                let color = self.color_rgb(Classic::DARK);
                self.graphics.draw_line((top_left.x - border_half, bottom_right.y - border_half), (bottom_right.x, bottom_right.y - border_half), border, color);
                self.graphics.draw_line((bottom_right.x - border_half, top_left.y - border_half), (bottom_right.x - border_half, bottom_right.y + 0.5), border, color);
                let color = self.color_rgb(Classic::LIGHT);
                self.graphics.draw_line((top_left.x + border, bottom_right.y - border - border_half), (bottom_right.x - border, bottom_right.y - border - border_half), border, color);
                self.graphics.draw_line((bottom_right.x - border - border_half, top_left.y + border), (bottom_right.x - border - border_half, bottom_right.y - border), border, color);
            }
        }
        if state.focused {
            let color = self.color_rgb(0x000000);
            let padding = border * 4f32;
            draw_dashed_rectangle(self.graphics, top_left.x + padding - 1.0, top_left.y + padding - 1.0, bottom_right.x - padding, bottom_right.y - padding, 2.5f32, border, color);
            //self.graphics.draw_line((top_left.x + border * 4f32, top_left.y + border * 4f32), (bottom_right.x - border * 4f32, top_left.y + border * 4f32), border, color);
            //self.graphics.draw_line((top_left.x + border * 4f32, bottom_right.y - border * 4f32), (bottom_right.x - border * 4f32, bottom_right.y - border * 4f32), border, color);
        }
    }

    #[allow(unused)]
    fn draw_button_text(&mut self, rect: Rect<i32>, state: ViewState, size: usize, text: &str) {
        todo!()
    }

    #[allow(unused)]
    fn draw_edit_back(&mut self, rect: Rect<i32>, state: ViewState) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_rgb(0xffffff);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    #[allow(unused)]
    fn draw_edit_body(&mut self, rect: Rect<i32>, state: ViewState) {
        let border: f32 = self.scale as f32;
        //let border2: f32 = (self.scale * 2f64) as f32;
        let border_half: f32 = (self.scale / 2f64) as f32;
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_rgb(Classic::LIGHT);
        self.graphics.draw_line((top_left.x, top_left.y + border_half), (bottom_right.x - border, top_left.y + border_half), border, color);
        self.graphics.draw_line((top_left.x + border_half, top_left.y), (top_left.x + border_half, bottom_right.y - border), border, color);
        let color = self.color_rgb(Classic::DARK);
        self.graphics.draw_line((top_left.x + border, top_left.y + border + border_half), (bottom_right.x - border, top_left.y + border + border_half), border, color);
        self.graphics.draw_line((top_left.x + border + border_half, top_left.y + border), (top_left.x + border + border_half, bottom_right.y - border), border, color);

        let color = self.color_rgb(Classic::BACKGROUND);
        self.graphics.draw_line((top_left.x + border, bottom_right.y - border - border_half), (bottom_right.x - border, bottom_right.y - border - border_half), border, color);
        self.graphics.draw_line((bottom_right.x - border - border_half, top_left.y + border), (bottom_right.x - border - border_half, bottom_right.y - border), border, color);

        let color = self.color_rgb(0xffffff);
        self.graphics.draw_line((top_left.x + border, bottom_right.y - border_half), (bottom_right.x - border, bottom_right.y - border_half), border, color);
    }

    fn draw_edit_caret(&mut self, rect: Rect<i32>, state: ViewState) {
        if !state.focused {
            return;
        }
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_rgb(Classic::BLACK);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    fn draw_checkbox_back(&mut self, rect: Rect<i32>, state: ViewState) {
        self.draw_edit_back(rect, state);
    }

    fn draw_checkbox_body(&mut self, rect: Rect<i32>, state: ViewState) {
        self.draw_edit_body(rect, state);
        if state.checked {
            self.draw_checkbox_checkmark(rect, state);
        }
    }

    fn draw_checkbox_checkmark(&mut self, rect: Rect<i32>, _state: ViewState) {
        let top_left = Vector2::new(rect.min.x as f32 + self.scale as f32 * 3.0, rect.min.y as f32 + self.scale as f32 * 3.0);
        let bottom_right = Vector2::new(rect.max.x as f32 - self.scale as f32 * 3.0, rect.max.y as f32 - self.scale as f32 * 3.0);
        let width = bottom_right.x - top_left.x;
        let height = bottom_right.y - top_left.y;
        let color = self.color_rgb(Classic::BLACK);
        self.graphics.draw_line((top_left.x, top_left.y + height / 2f32), (top_left.x + width / 3f32, bottom_right.y - height / 8f32), self.scale as f32, color);
        self.graphics.draw_line((top_left.x + width / 3f32, bottom_right.y - height / 8f32), (bottom_right.x, top_left.y + height / 8f32), self.scale as f32, color);
    }

    fn draw_radiobutton_back(&mut self, rect: Rect<i32>, _state: ViewState) {
        let cx = (rect.min.x + rect.max.x) as f32 / 2.0;
        let cy = (rect.min.y + rect.max.y) as f32 / 2.0;
        let radius = (rect.max.x - rect.min.x) as f32 / 2.0;
        let color = self.color_rgb(0xffffff);
        self.graphics.draw_circle((cx, cy), radius, color);
    }

    fn draw_radiobutton_body(&mut self, rect: Rect<i32>, state: ViewState) {
        let cx = (rect.min.x + rect.max.x) as f32 / 2.0;
        let cy = (rect.min.y + rect.max.y) as f32 / 2.0;
        let radius = (rect.max.x - rect.min.x) as f32 / 2.0;
        let border = self.scale as f32;
        // Draw outer circle border using lines approximating a circle
        let color = self.color_rgb(Classic::LIGHT);
        let segments = 32;
        for i in 0..segments {
            let angle1 = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
            let angle2 = 2.0 * std::f32::consts::PI * (i + 1) as f32 / segments as f32;
            let x1 = cx + radius * angle1.cos();
            let y1 = cy + radius * angle1.sin();
            let x2 = cx + radius * angle2.cos();
            let y2 = cy + radius * angle2.sin();
            self.graphics.draw_line((x1, y1), (x2, y2), border, color);
        }
        if state.focused {
            let color = self.color_rgb(Classic::DARK);
            let outer_radius = radius + border * 2.0;
            for i in 0..segments {
                let angle1 = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
                let angle2 = 2.0 * std::f32::consts::PI * (i + 1) as f32 / segments as f32;
                let x1 = cx + outer_radius * angle1.cos();
                let y1 = cy + outer_radius * angle1.sin();
                let x2 = cx + outer_radius * angle2.cos();
                let y2 = cy + outer_radius * angle2.sin();
                self.graphics.draw_line((x1, y1), (x2, y2), border, color);
            }
        }
    }

    fn draw_radiobutton_indicator(&mut self, rect: Rect<i32>, _state: ViewState) {
        let cx = (rect.min.x + rect.max.x) as f32 / 2.0;
        let cy = (rect.min.y + rect.max.y) as f32 / 2.0;
        let radius = (rect.max.x - rect.min.x) as f32 / 2.0;
        let dot_radius = radius * 0.45;
        let color = self.color_rgb(Classic::BLACK);
        self.graphics.draw_circle((cx, cy), dot_radius, color);
    }

    fn draw_combobox_arrow(&mut self, rect: Rect<i32>, _state: ViewState) {
        let cx = (rect.min.x + rect.max.x) as f32 / 2.0;
        let cy = (rect.min.y + rect.max.y) as f32 / 2.0;
        let half_w = (4.0 * self.scale).round() as f32;
        let half_h = (2.0 * self.scale).round() as f32;
        let color = self.color_rgb(Classic::BLACK);
        // Filled downward triangle
        self.graphics.draw_triangle_three_color(
            [Vector2::new(cx - half_w, cy - half_h), Vector2::new(cx + half_w, cy - half_h), Vector2::new(cx, cy + half_h)],
            [color, color, color],
        );
    }

    fn draw_list_back(&mut self, rect: Rect<i32>, state: ViewState) {
        self.draw_edit_back(rect, state);
    }

    fn draw_list_body(&mut self, rect: Rect<i32>, state: ViewState) {
        self.draw_edit_body(rect, state);
    }

    #[allow(unused)]
    fn draw_panel_back(&mut self, rect: Rect<i32>, state: ViewState) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_rgb(Classic::BACKGROUND);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    #[allow(unused)]
    fn draw_panel_body(&mut self, rect: Rect<i32>, state: ViewState) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let border: f32 = 1f32;
        let color = self.color_rgb(0xff808080);
        let half = 0.5f32;
        //draw_rounded_rectangle(self.graphics, rect.min.x as f32, rect.min.y as f32, rect.max.x as f32, rect.max.y as f32, 16f32, 2f32, color);
        self.graphics.draw_line((top_left.x, top_left.y + border - half), (bottom_right.x, top_left.y + border - half), border, color);
        self.graphics.draw_line((top_left.x, bottom_right.y - half), (bottom_right.x, bottom_right.y - half), border, color);
        self.graphics.draw_line((top_left.x + half, top_left.y + border), (top_left.x + half, bottom_right.y + border), border, color);
        self.graphics.draw_line((bottom_right.x - half, top_left.y + border + half), (bottom_right.x - half, bottom_right.y + border - half), border, color);
    }

    fn draw_text(&mut self, x: f32, y: f32, color: u32, text: &FormattedTextBlock) {
        let color = self.color_argb(color);
        self.graphics.draw_text((x, y), color, text);
    }

    fn draw_rect(&mut self, rect: Rect<i32>, color: u32) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_argb(color);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    // New drawable-based methods implementation
    fn draw_drawable(&mut self, drawable: &Drawable, rect: Rect<i32>) {
        let mut engine = DrawingEngine::new(self.graphics, self.scale);
        engine.draw_drawable(drawable, rect);
    }

    fn get_drawable_registry(&self) -> &DrawableRegistry {
        &self.drawable_registry
    }

    fn draw_component(&mut self, drawable_name: &str, rect: Rect<i32>, state: ViewState) {
        // Get drawable from registry
        if let Some(selector) = self.drawable_registry.get(drawable_name) {
            if let Some(drawable) = selector.get_drawable(&state) {
                let mut engine = DrawingEngine::new(self.graphics, self.scale);
                engine.draw_drawable(drawable, rect);
            }
        }
    }

    fn draw_progressbar_track(&mut self, rect: Rect<i32>) {
        // Sunken 3D border (same as edit field) with background fill
        self.draw_edit_back(rect, ViewState::no_focus());
        self.draw_edit_body(rect, ViewState::no_focus());
    }

    fn draw_progressbar_fill(&mut self, rect: Rect<i32>) {
        // Classic Win95 blue progress fill: navy blue (0x000080)
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_rgb(0xff000080);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    fn draw_scrollbar_track(&mut self, rect: Rect<i32>, _direction: Direction) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_rgb(Classic::BACKGROUND);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    fn draw_scrollbar_thumb(&mut self, rect: Rect<i32>, state: ViewState, _direction: Direction) {
        self.draw_button_back(rect, state);
        self.draw_button_body(rect, state);
    }

    fn draw_scrollbar_arrow_button(&mut self, rect: Rect<i32>, state: ViewState, toward_start: bool, direction: Direction) {
        self.draw_button_back(rect, state);
        self.draw_button_body(rect, state);

        // Draw arrow triangle — nudge left/up by half a pixel to center within the 3D button borders
        let border_offset = (self.scale * 0.5) as f32;
        let cx = (rect.min.x + rect.max.x) as f32 / 2.0 - border_offset;
        let cy = (rect.min.y + rect.max.y) as f32 / 2.0 - border_offset;
        let half_w = (3.0 * self.scale).round() as f32;
        let half_h = (2.0 * self.scale).round() as f32;
        let color = self.color_rgb(Classic::BLACK);
        let offset = if state.pressed { self.scale as f32 } else { 0.0 };

        let (p1, p2, p3) = match (direction, toward_start) {
            (Direction::Vertical, true) => {
                // Up arrow
                (Vector2::new(cx + offset, cy - half_h + offset),
                 Vector2::new(cx - half_w + offset, cy + half_h + offset),
                 Vector2::new(cx + half_w + offset, cy + half_h + offset))
            }
            (Direction::Vertical, false) => {
                // Down arrow
                (Vector2::new(cx - half_w + offset, cy - half_h + offset),
                 Vector2::new(cx + half_w + offset, cy - half_h + offset),
                 Vector2::new(cx + offset, cy + half_h + offset))
            }
            (Direction::Horizontal, true) => {
                // Left arrow
                (Vector2::new(cx - half_h + offset, cy + offset),
                 Vector2::new(cx + half_h + offset, cy - half_w + offset),
                 Vector2::new(cx + half_h + offset, cy + half_w + offset))
            }
            (Direction::Horizontal, false) => {
                // Right arrow
                (Vector2::new(cx - half_h + offset, cy - half_w + offset),
                 Vector2::new(cx - half_h + offset, cy + half_w + offset),
                 Vector2::new(cx + half_h + offset, cy + offset))
            }
        };
        self.graphics.draw_triangle_three_color([p1, p2, p3], [color, color, color]);
    }

    fn draw_tab_active(&mut self, rect: Rect<i32>, _state: ViewState) {
        let border: f32 = self.scale as f32;
        let border_half: f32 = (self.scale / 2.0) as f32;
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);

        // Background fill — extend past bottom to erase the content area's top border
        let bg_color = self.color_rgb(Classic::BACKGROUND);
        self.graphics.draw_rectangle(
            Rectangle::new(
                Vector2::new(top_left.x + border, top_left.y + border),
                Vector2::new(bottom_right.x - border, bottom_right.y + border),
            ),
            bg_color,
        );

        // White left border
        let white = self.color_rgb(0xffffff);
        self.graphics.draw_line(
            (top_left.x + border_half, top_left.y),
            (top_left.x + border_half, bottom_right.y),
            border, white,
        );
        // White top border
        self.graphics.draw_line(
            (top_left.x, top_left.y + border_half),
            (bottom_right.x, top_left.y + border_half),
            border, white,
        );
        // Dark right border (outer)
        let dark = self.color_rgb(Classic::DARK);
        self.graphics.draw_line(
            (bottom_right.x - border_half, top_left.y),
            (bottom_right.x - border_half, bottom_right.y),
            border, dark,
        );
        // Light right border (inner)
        let light = self.color_rgb(Classic::LIGHT);
        self.graphics.draw_line(
            (bottom_right.x - border - border_half, top_left.y + border),
            (bottom_right.x - border - border_half, bottom_right.y),
            border, light,
        );
    }

    fn draw_tab_inactive(&mut self, rect: Rect<i32>, _state: ViewState) {
        let border: f32 = self.scale as f32;
        let border_half: f32 = (self.scale / 2.0) as f32;
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);

        // Background fill (slightly lighter for inactive)
        let bg_color = self.color_rgb(Classic::BACKGROUND);
        self.graphics.draw_rectangle(
            Rectangle::new(
                Vector2::new(top_left.x + border, top_left.y + border),
                Vector2::new(bottom_right.x - border, bottom_right.y - border),
            ),
            bg_color,
        );

        // White left border
        let white = self.color_rgb(0xffffff);
        self.graphics.draw_line(
            (top_left.x + border_half, top_left.y),
            (top_left.x + border_half, bottom_right.y),
            border, white,
        );
        // White top border
        self.graphics.draw_line(
            (top_left.x, top_left.y + border_half),
            (bottom_right.x, top_left.y + border_half),
            border, white,
        );
        // Dark right border (outer)
        let dark = self.color_rgb(Classic::DARK);
        self.graphics.draw_line(
            (bottom_right.x - border_half, top_left.y),
            (bottom_right.x - border_half, bottom_right.y),
            border, dark,
        );
        // Light right border (inner)
        let light = self.color_rgb(Classic::LIGHT);
        self.graphics.draw_line(
            (bottom_right.x - border - border_half, top_left.y + border),
            (bottom_right.x - border - border_half, bottom_right.y),
            border, light,
        );
    }

    fn draw_tab_content_area(&mut self, rect: Rect<i32>, _state: ViewState) {
        let border: f32 = self.scale as f32;
        let border_half: f32 = (self.scale / 2.0) as f32;
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);

        // Background fill
        let bg_color = self.color_rgb(Classic::BACKGROUND);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), bg_color);

        // Raised 3D border: white top + left
        let white = self.color_rgb(0xffffff);
        self.graphics.draw_line(
            (top_left.x, top_left.y + border_half),
            (bottom_right.x, top_left.y + border_half),
            border, white,
        );
        self.graphics.draw_line(
            (top_left.x + border_half, top_left.y),
            (top_left.x + border_half, bottom_right.y),
            border, white,
        );

        // Dark bottom + right
        let dark = self.color_rgb(Classic::DARK);
        self.graphics.draw_line(
            (top_left.x, bottom_right.y - border_half),
            (bottom_right.x, bottom_right.y - border_half),
            border, dark,
        );
        self.graphics.draw_line(
            (bottom_right.x - border_half, top_left.y),
            (bottom_right.x - border_half, bottom_right.y),
            border, dark,
        );

        // Inner shadow: light gray inside top+left
        let light = self.color_rgb(Classic::LIGHT);
        self.graphics.draw_line(
            (top_left.x + border, bottom_right.y - border - border_half),
            (bottom_right.x - border, bottom_right.y - border - border_half),
            border, light,
        );
        self.graphics.draw_line(
            (bottom_right.x - border - border_half, top_left.y + border),
            (bottom_right.x - border - border_half, bottom_right.y - border),
            border, light,
        );
    }

    fn draw_separator(&mut self, rect: Rect<i32>, _state: ViewState) {
        let border: f32 = self.scale as f32;
        let border_half: f32 = (self.scale / 2.0) as f32;
        // Determine orientation from rect aspect ratio
        if rect.width() >= rect.height() {
            // Horizontal separator: center the etched line pair vertically
            let cy = (rect.min.y + rect.max.y) as f32 / 2.0;
            let x1 = rect.min.x as f32;
            let x2 = rect.max.x as f32;
            let color = self.color_rgb(Classic::LIGHT);
            self.graphics.draw_line((x1, cy - border_half), (x2, cy - border_half), border, color);
            let color = self.color_rgb(0xffffff);
            self.graphics.draw_line((x1, cy + border_half), (x2, cy + border_half), border, color);
        } else {
            // Vertical separator: center the etched line pair horizontally
            let cx = (rect.min.x + rect.max.x) as f32 / 2.0;
            let y1 = rect.min.y as f32;
            let y2 = rect.max.y as f32;
            let color = self.color_rgb(Classic::LIGHT);
            self.graphics.draw_line((cx - border_half, y1), (cx - border_half, y2), border, color);
            let color = self.color_rgb(0xffffff);
            self.graphics.draw_line((cx + border_half, y1), (cx + border_half, y2), border, color);
        }
    }

    fn push_opacity(&mut self, opacity: f32) {
        let current = self.current_opacity();
        self.opacity_stack.push(current * opacity);
    }

    fn pop_opacity(&mut self) {
        self.opacity_stack.pop();
    }

    fn draw_image(&mut self, rect: Rect<i32>, image_bytes: &[u8]) {
        let cache_key = image_bytes.as_ptr() as usize;
        if !self.image_cache.contains_key(&cache_key) {
            let cursor = Cursor::new(image_bytes);
            match self.graphics.create_image_from_file_bytes(None, ImageSmoothingMode::Linear, cursor) {
                Ok(handle) => {
                    self.image_cache.insert(cache_key, handle);
                }
                Err(e) => {
                    println!("Error creating image: {}", e);
                    return;
                }
            }
        }
        if let Some(handle) = self.image_cache.get(&cache_key) {
            let speedy_rect = Rectangle::from_tuples(
                (rect.min.x as f32, rect.min.y as f32),
                (rect.max.x as f32, rect.max.y as f32),
            );
            self.graphics.draw_rectangle_image(speedy_rect, handle);
        }
    }

    fn draw_raw_image(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), cache_key: u64) {
        let key = cache_key as usize;
        if !self.image_cache.contains_key(&key) {
            match self.graphics.create_image_from_raw_pixels(
                ImageDataType::RGBA,
                ImageSmoothingMode::Linear,
                UVec2::new(size.0, size.1),
                rgba,
            ) {
                Ok(handle) => {
                    self.image_cache.insert(key, handle);
                }
                Err(e) => {
                    println!("Error uploading raw image: {}", e);
                    return;
                }
            }
        }
        if let Some(handle) = self.image_cache.get(&key) {
            let speedy_rect = Rectangle::from_tuples(
                (rect.min.x as f32, rect.min.y as f32),
                (rect.max.x as f32, rect.max.y as f32),
            );
            self.graphics.draw_rectangle_image(speedy_rect, handle);
        }
    }
}