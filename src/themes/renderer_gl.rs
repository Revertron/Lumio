use std::collections::{HashMap, VecDeque};
use std::io::Cursor;

use log::error;

use speedy2d::Graphics2D;
use speedy2d::color::Color;
use speedy2d::dimen::{UVec2, Vector2};
use speedy2d::image::{ImageDataType, ImageHandle, ImageSmoothingMode};
use speedy2d::shape::Rectangle;

use super::super::drawing::{Drawable, DrawableRegistry, DrawingEngine, Palette};
use super::super::text::TextBlock;
use super::super::themes::{OpacityStack, Renderer, Typeface, ViewState};
use super::super::types::{Rect, rect};

/// Cache for GPU image handles, keyed by the owning `ImageSource`'s unique id.
pub type ImageCache = HashMap<u64, ImageHandle>;

#[allow(unused)]
pub struct RendererGL<'h> {
    graphics: &'h mut Graphics2D,
    width: i32,
    height: i32,
    scale: f64,
    current_clip: Rect<i32>,
    clip_stack: VecDeque<Rect<i32>>,
    opacity: OpacityStack,
    drawable_registry: &'h DrawableRegistry,
    palette: &'h Palette,
    image_cache: &'h mut ImageCache
}

#[allow(dead_code)]
impl<'h> RendererGL<'h> {
    fn current_opacity(&self) -> f32 {
        self.opacity.current()
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

    /// The default typeface of the currently active palette. Convenience for
    /// app startup (`UI::from_xml`), where no theme instance exists yet.
    /// The size is stripped: a root typeface with an explicit size would
    /// cascade into every view and shadow the palette's per-role font sizes
    /// ("button", "menu", ...).
    pub fn typeface() -> Typeface {
        super::default_typeface()
    }

    pub fn new(
        graphics: &'h mut Graphics2D,
        drawable_registry: &'h DrawableRegistry,
        palette: &'h Palette,
        image_cache: &'h mut ImageCache,
        width: i32,
        height: i32,
        scale: f64
    ) -> Self {
        let current_clip = rect((0, 0), (width, height));
        RendererGL {
            graphics,
            width,
            height,
            scale,
            current_clip,
            clip_stack: VecDeque::new(),
            opacity: OpacityStack::new(),
            drawable_registry,
            palette,
            image_cache
        }
    }
}

impl<'h> Renderer for RendererGL<'h> {
    fn clear_screen(&mut self) {
        self.graphics.set_clip(None);
        self.graphics.clear_screen(Color::from_hex_rgb(self.palette.color("background")));
        self.set_clip(self.current_clip);
    }

    fn palette(&self) -> &Palette {
        self.palette
    }

    fn set_clip(&mut self, rect: Rect<i32>) {
        self.current_clip = rect;
        let rect = Rectangle::from_tuples((rect.min.x, rect.min.y), (rect.max.x, rect.max.y));
        self.graphics.set_clip(Some(rect));
    }

    fn clip_rect(&mut self, rect: Rect<i32>) -> Rect<i32> {
        let clipped = self.current_clip.intersect(&rect);
        self.set_clip(clipped);
        clipped
    }

    fn push_clip(&mut self) {
        self.clip_stack.push_back(self.current_clip);
    }

    fn pop_clip(&mut self) {
        if let Some(clip) = self.clip_stack.pop_back() {
            self.set_clip(clip);
        }
    }

    fn draw_text(&mut self, x: f32, y: f32, color: u32, text: &TextBlock) {
        let color = self.color_argb(color);
        match text.payload() {
            crate::text::BackendBlock::Speedy(block) => self.graphics.draw_text((x, y), color, block),
            // Shaped by the other backend (only possible around a runtime
            // backend switch); skip — the next layout re-shapes.
            #[cfg(feature = "text-software")]
            _ => {}
        }
    }

    fn draw_text_cropped(&mut self, x: f32, y: f32, crop: Rect<i32>, color: u32, text: &TextBlock) {
        let crop = Rectangle::from_tuples((crop.min.x as f32, crop.min.y as f32), (crop.max.x as f32, crop.max.y as f32));
        let color = self.color_argb(color);
        match text.payload() {
            crate::text::BackendBlock::Speedy(block) => self.graphics.draw_text_cropped((x, y), crop, color, block),
            #[cfg(feature = "text-software")]
            _ => {}
        }
    }

    fn draw_rect(&mut self, rect: Rect<i32>, color: u32) {
        let top_left = Vector2::new(rect.min.x as f32, rect.min.y as f32);
        let bottom_right = Vector2::new(rect.max.x as f32, rect.max.y as f32);
        let color = self.color_argb(color);
        self.graphics.draw_rectangle(Rectangle::new(top_left, bottom_right), color);
    }

    fn draw_rounded_rect(&mut self, rect: Rect<i32>, color: u32, radius: i32) {
        let w = rect.width();
        let h = rect.height();
        if w <= 0 || h <= 0 {
            return;
        }
        let r = radius.min(w / 2).min(h / 2).max(0);
        if r == 0 {
            self.draw_rect(rect, color);
            return;
        }
        let c = self.color_argb(color);
        let (x0, y0, x1, y1) = (rect.min.x as f32, rect.min.y as f32, rect.max.x as f32, rect.max.y as f32);
        let rf = r as f32;
        // Top, bottom, and middle bands. Corners are filled by the four circles.
        // Note: circles overlap the bands by a fraction of a pixel; for opaque
        // colors this is invisible. With low alpha (e.g. inside a fading
        // notification) the corners will appear slightly more saturated.
        self.graphics.draw_rectangle(Rectangle::new(Vector2::new(x0 + rf, y0), Vector2::new(x1 - rf, y0 + rf)), c);
        self.graphics.draw_rectangle(Rectangle::new(Vector2::new(x0 + rf, y1 - rf), Vector2::new(x1 - rf, y1)), c);
        self.graphics.draw_rectangle(Rectangle::new(Vector2::new(x0, y0 + rf), Vector2::new(x1, y1 - rf)), c);
        self.graphics.draw_circle((x0 + rf, y0 + rf), rf, c);
        self.graphics.draw_circle((x1 - rf, y0 + rf), rf, c);
        self.graphics.draw_circle((x0 + rf, y1 - rf), rf, c);
        self.graphics.draw_circle((x1 - rf, y1 - rf), rf, c);
    }

    // New drawable-based methods implementation
    fn draw_drawable(&mut self, drawable: &Drawable, rect: Rect<i32>) {
        let mut engine = DrawingEngine::new(self.graphics, self.scale, self.palette);
        engine.draw_drawable(drawable, rect);
    }

    fn get_drawable_registry(&self) -> &DrawableRegistry {
        &self.drawable_registry
    }

    fn draw_component(&mut self, role: &str, rect: Rect<i32>, state: ViewState) {
        // Copy the registry ref out so the 9-patch paint below can take `&mut self`.
        let registry = self.drawable_registry;
        // A 9-patch override for this role wins over any shape drawable.
        if let Some(ninepatch) = registry.get_ninepatch(role) {
            let scale = self.scale;
            ninepatch.borrow_mut().paint(self, rect, &state, scale);
            return;
        }
        if let Some(selector) = registry.get(role) {
            if let Some(drawable) = selector.get_drawable(&state) {
                let mut engine = DrawingEngine::new(self.graphics, self.scale, self.palette);
                engine.draw_drawable(drawable, rect);
            }
        }
    }

    fn push_opacity(&mut self, opacity: f32) {
        self.opacity.push(opacity);
    }

    fn pop_opacity(&mut self) {
        self.opacity.pop();
    }

    fn draw_image(&mut self, rect: Rect<i32>, image_bytes: &[u8], cache_key: u64) {
        self.draw_image_tinted(rect, image_bytes, cache_key, 0xFFFFFFFF);
    }

    fn draw_raw_image(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), cache_key: u64) {
        self.draw_raw_image_tinted(rect, rgba, size, cache_key, 0xFFFFFFFF);
    }

    fn draw_image_tinted(&mut self, rect: Rect<i32>, image_bytes: &[u8], cache_key: u64, tint_argb: u32) {
        if !self.image_cache.contains_key(&cache_key) {
            let cursor = Cursor::new(image_bytes);
            match self.graphics.create_image_from_file_bytes(None, ImageSmoothingMode::Linear, cursor) {
                Ok(handle) => {
                    self.image_cache.insert(cache_key, handle);
                }
                Err(e) => {
                    error!("Error creating image: {}", e);
                    return;
                }
            }
        }
        if let Some(handle) = self.image_cache.get(&cache_key) {
            let speedy_rect = Rectangle::from_tuples((rect.min.x as f32, rect.min.y as f32), (rect.max.x as f32, rect.max.y as f32));
            let a = ((tint_argb >> 24) & 0xFF) as f32 / 255.0;
            let r = ((tint_argb >> 16) & 0xFF) as f32 / 255.0;
            let g = ((tint_argb >> 8) & 0xFF) as f32 / 255.0;
            let b = (tint_argb & 0xFF) as f32 / 255.0;
            let tint = Color::from_rgba(r, g, b, a * self.current_opacity());
            self.graphics.draw_rectangle_image_tinted(speedy_rect, tint, handle);
        }
    }

    fn draw_raw_image_tinted(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), cache_key: u64, tint_argb: u32) {
        let key = cache_key;
        if !self.image_cache.contains_key(&key) {
            match self.graphics.create_image_from_raw_pixels(
                ImageDataType::RGBA,
                ImageSmoothingMode::Linear,
                UVec2::new(size.0, size.1),
                rgba
            ) {
                Ok(handle) => {
                    self.image_cache.insert(key, handle);
                }
                Err(e) => {
                    error!("Error uploading raw image: {}", e);
                    return;
                }
            }
        }
        if let Some(handle) = self.image_cache.get(&key) {
            let speedy_rect = Rectangle::from_tuples((rect.min.x as f32, rect.min.y as f32), (rect.max.x as f32, rect.max.y as f32));
            let a = ((tint_argb >> 24) & 0xFF) as f32 / 255.0;
            let r = ((tint_argb >> 16) & 0xFF) as f32 / 255.0;
            let g = ((tint_argb >> 8) & 0xFF) as f32 / 255.0;
            let b = (tint_argb & 0xFF) as f32 / 255.0;
            let tint = Color::from_rgba(r, g, b, a * self.current_opacity());
            self.graphics.draw_rectangle_image_tinted(speedy_rect, tint, handle);
        }
    }
}
