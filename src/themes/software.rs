//! Software theme: a `Theme` implementation over `tiny_skia::Pixmap`. Mirror of
//! `Classic` but rasterizing on the CPU (used for headless rendering today; the
//! software window loop reuses it later). Text is rasterized per-glyph from the
//! fontdue payload carried by `TextBlock`.

use std::collections::{HashMap, VecDeque};

use tiny_skia::{FillRule, IntSize, Mask, Paint as TsPaint, PathBuilder, Pixmap, PixmapPaint, Rect as TsRect, Transform};

use super::super::drawing::engine_software::{argb_to_color, SoftwareDrawingEngine};
use super::super::drawing::{Drawable, DrawableRegistry, Palette};
use super::super::text::TextBlock;
use super::super::themes::{OpacityStack, Theme, ViewState};
use super::super::types::{Rect, rect};

/// Decoded images for the software backend, keyed by the owning `ImageSource`
/// id: straight (non-premultiplied) RGBA8 plus pixel size.
pub type SoftwareImageCache = HashMap<u64, (Vec<u8>, u32, u32)>;

/// Rasterized glyph coverage cache: `(font_hash, glyph_index, px.to_bits())`
/// → `(width, height, coverage)`. Persisted across frames by the window loop so
/// `draw_text` doesn't re-rasterize every glyph each frame (headless callers
/// pass a throwaway one).
pub type GlyphCache = HashMap<(usize, u16, u32), (usize, usize, Vec<u8>)>;

/// Outcome of testing a primitive's bounds against the current clip.
#[derive(Clone, Copy)]
enum ClipDecision {
    /// Fully outside the clip (or the clip is empty) — draw nothing.
    Skip,
    /// Fully inside the clip — draw with no mask.
    NoMask,
    /// Straddles the clip edge — draw with `clip_mask`.
    Masked,
}

pub struct SoftwareTheme<'h> {
    pixmap: &'h mut Pixmap,
    width: i32,
    height: i32,
    scale: f64,
    current_clip: Rect<i32>,
    clip_stack: VecDeque<Rect<i32>>,
    /// True when `current_clip` covers the whole pixmap (no clipping needed).
    clip_full: bool,
    /// Lazily-built coverage mask for `current_clip`, used only for primitives
    /// that straddle the clip edge (glyphs/images/drawables). `None` = not yet
    /// built for the current clip; reset whenever the clip changes. Rect fills
    /// clip geometrically and never touch this.
    clip_mask: Option<Mask>,
    opacity: OpacityStack,
    drawable_registry: &'h DrawableRegistry,
    palette: &'h Palette,
    image_cache: &'h mut SoftwareImageCache,
    glyph_cache: &'h mut GlyphCache,
}

impl<'h> SoftwareTheme<'h> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pixmap: &'h mut Pixmap,
        drawable_registry: &'h DrawableRegistry,
        palette: &'h Palette,
        image_cache: &'h mut SoftwareImageCache,
        glyph_cache: &'h mut GlyphCache,
        width: i32,
        height: i32,
        scale: f64,
    ) -> Self {
        SoftwareTheme {
            pixmap,
            width,
            height,
            scale,
            current_clip: rect((0, 0), (width, height)),
            clip_stack: VecDeque::new(),
            clip_full: true,
            clip_mask: None,
            opacity: OpacityStack::new(),
            drawable_registry,
            palette,
            image_cache,
            glyph_cache,
        }
    }

    fn current_opacity(&self) -> f32 {
        self.opacity.current()
    }

    /// True when `dest` lies entirely within the current clip (so a primitive
    /// covering `dest` needs no clipping).
    fn clip_contains(&self, dest: Rect<i32>) -> bool {
        let c = self.current_clip;
        c.min.x <= dest.min.x && c.min.y <= dest.min.y && c.max.x >= dest.max.x && c.max.y >= dest.max.y
    }

    /// Decide how a primitive covering `dest` interacts with the current clip,
    /// building the lazy mask when it's needed. `Skip` means draw nothing — the
    /// primitive is fully outside the clip, or the clip is degenerate/empty (a
    /// missing mask must clip *everything*, not nothing, or off-screen content
    /// leaks, e.g. recycler items scrolled past the viewport).
    fn clip_decision(&mut self, dest: Rect<i32>) -> ClipDecision {
        if self.clip_full {
            return ClipDecision::NoMask;
        }
        let c = self.current_clip;
        let empty = c.max.x <= c.min.x || c.max.y <= c.min.y;
        let outside = dest.max.x <= c.min.x
            || dest.min.x >= c.max.x
            || dest.max.y <= c.min.y
            || dest.min.y >= c.max.y;
        if empty || outside {
            return ClipDecision::Skip;
        }
        if self.clip_contains(dest) {
            return ClipDecision::NoMask;
        }
        self.ensure_clip_mask();
        // A non-degenerate clip always builds; if it somehow didn't, be safe.
        if self.clip_mask.is_some() {
            ClipDecision::Masked
        } else {
            ClipDecision::Skip
        }
    }

    /// Build the clip mask for `current_clip` on first use; cached until the clip
    /// changes. Call only after `needs_mask` returned true.
    fn ensure_clip_mask(&mut self) {
        if self.clip_mask.is_none() {
            self.clip_mask = build_rect_mask(self.width as u32, self.height as u32, self.current_clip);
        }
    }

    /// Geometric intersection of `r` with the current clip (for rect fills, which
    /// never need a mask).
    fn clip_rect_geom(&self, r: Rect<i32>) -> Rect<i32> {
        self.current_clip.intersect(&r)
    }
}

impl<'h> Theme for SoftwareTheme<'h> {
    fn clear_screen(&mut self) {
        let color = argb_to_color(self.palette.color("background") | 0xFF00_0000, 1.0)
            .unwrap_or(tiny_skia::Color::WHITE);
        self.pixmap.fill(color);
    }

    fn palette(&self) -> &Palette {
        self.palette
    }

    fn set_clip(&mut self, rect: Rect<i32>) {
        self.current_clip = rect;
        let c = rect;
        self.clip_full = c.min.x <= 0 && c.min.y <= 0 && c.max.x >= self.width && c.max.y >= self.height;
        // Invalidate the lazy mask; it rebuilds on demand for the new clip.
        self.clip_mask = None;
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
        let block = text.payload();
        let opacity = self.current_opacity();
        let cr = ((color >> 16) & 0xff) as u32;
        let cg = ((color >> 8) & 0xff) as u32;
        let cb = (color & 0xff) as u32;
        let ca = ((color >> 24) & 0xff) as f32 / 255.0;
        let alpha_scale = (opacity * ca).clamp(0.0, 1.0);

        for g in &block.glyphs {
            // Key on the font's global hash, not the block-relative `font_index`
            // (which is 0 for the primary font of every block, so bold/italic/
            // regular would collide on shared glyph indices).
            let key = (g.font_hash, g.glyph_index, g.px.to_bits());
            if !self.glyph_cache.contains_key(&key) {
                let (m, cov) = block.fonts[g.font_index].rasterize_indexed(g.glyph_index, g.px);
                self.glyph_cache.insert(key, (m.width, m.height, cov));
            }
            // Glyph size first (cheap), so off-screen glyphs are clip-tested and
            // skipped BEFORE the per-glyph premultiply + pixmap allocation.
            let (gw, gh) = match self.glyph_cache.get(&key) {
                Some((w, h, _)) if *w > 0 && *h > 0 => (*w, *h),
                _ => continue,
            };
            let px = (x + g.x).round() as i32;
            let py = (y + g.y).round() as i32;
            let dest = rect((px, py), (px + gw as i32, py + gh as i32));
            let decision = self.clip_decision(dest);
            if matches!(decision, ClipDecision::Skip) {
                continue;
            }
            // Build the premultiplied glyph pixmap from cached coverage. The
            // `coverage` borrow (glyph_cache) ends before the `mask`/`pixmap`
            // borrows below — all disjoint fields.
            let Some(coverage) = self.glyph_cache.get(&key).map(|(_, _, cov)| cov) else {
                continue;
            };
            let mut data = vec![0u8; gw * gh * 4];
            for (i, &cov) in coverage.iter().enumerate() {
                let a = (cov as f32 * alpha_scale) as u32; // 0..255 premultiplied alpha
                let off = i * 4;
                data[off] = (cr * a / 255) as u8;
                data[off + 1] = (cg * a / 255) as u8;
                data[off + 2] = (cb * a / 255) as u8;
                data[off + 3] = a as u8;
            }
            let Some(size) = IntSize::from_wh(gw as u32, gh as u32) else {
                continue;
            };
            let Some(glyph_pm) = Pixmap::from_vec(data, size) else {
                continue;
            };
            let mask = match decision {
                ClipDecision::Masked => self.clip_mask.as_ref(),
                _ => None,
            };
            self.pixmap.as_mut().draw_pixmap(
                px,
                py,
                glyph_pm.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                mask,
            );
        }
    }

    fn draw_rect(&mut self, rect: Rect<i32>, color: u32) {
        let Some(c) = argb_to_color(color, self.current_opacity()) else {
            return;
        };
        // Axis-aligned fill: clip geometrically against the current clip rect —
        // no coverage mask needed.
        let r = self.clip_rect_geom(rect);
        if r.width() <= 0 || r.height() <= 0 {
            return;
        }
        let mut paint = TsPaint::default();
        paint.set_color(c);
        paint.anti_alias = false; // crisp axis-aligned fills
        if let Some(tr) = TsRect::from_ltrb(r.min.x as f32, r.min.y as f32, r.max.x as f32, r.max.y as f32) {
            self.pixmap.as_mut().fill_rect(tr, &paint, Transform::identity(), None);
        }
    }

    fn draw_rounded_rect(&mut self, rect: Rect<i32>, color: u32, radius: i32) {
        let Some(c) = argb_to_color(color, self.current_opacity()) else {
            return;
        };
        let w = rect.width();
        let h = rect.height();
        if w <= 0 || h <= 0 {
            return;
        }
        let r = (radius.min(w / 2).min(h / 2).max(0)) as f32;
        if r <= 0.0 {
            return self.draw_rect(rect, color);
        }
        let (x0, y0, x1, y1) = (rect.min.x as f32, rect.min.y as f32, rect.max.x as f32, rect.max.y as f32);
        let mut pb = PathBuilder::new();
        pb.move_to(x0 + r, y0);
        pb.line_to(x1 - r, y0);
        pb.quad_to(x1, y0, x1, y0 + r);
        pb.line_to(x1, y1 - r);
        pb.quad_to(x1, y1, x1 - r, y1);
        pb.line_to(x0 + r, y1);
        pb.quad_to(x0, y1, x0, y1 - r);
        pb.line_to(x0, y0 + r);
        pb.quad_to(x0, y0, x0 + r, y0);
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = TsPaint::default();
            paint.set_color(c);
            paint.anti_alias = true;
            let mask = match self.clip_decision(rect) {
                ClipDecision::Skip => return,
                ClipDecision::NoMask => None,
                ClipDecision::Masked => self.clip_mask.as_ref(),
            };
            self.pixmap.as_mut().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), mask);
        }
    }

    fn draw_drawable(&mut self, drawable: &Drawable, rect: Rect<i32>) {
        let clip = match self.clip_decision(rect) {
            ClipDecision::Skip => return,
            ClipDecision::NoMask => None,
            ClipDecision::Masked => self.clip_mask.as_ref(),
        };
        let mut engine = SoftwareDrawingEngine::new(&mut *self.pixmap, self.scale, self.palette, clip);
        engine.draw_drawable(drawable, rect);
    }

    fn get_drawable_registry(&self) -> &DrawableRegistry {
        self.drawable_registry
    }

    fn draw_component(&mut self, role: &str, rect: Rect<i32>, state: ViewState) {
        if let Some(selector) = self.drawable_registry.get(role) {
            if let Some(drawable) = selector.get_drawable(&state) {
                let clip = match self.clip_decision(rect) {
                    ClipDecision::Skip => return,
                    ClipDecision::NoMask => None,
                    ClipDecision::Masked => self.clip_mask.as_ref(),
                };
                let mut engine = SoftwareDrawingEngine::new(&mut *self.pixmap, self.scale, self.palette, clip);
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

    fn draw_raw_image(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), cache_key: u64) {
        self.draw_raw_image_tinted(rect, rgba, size, cache_key, 0xFFFFFFFF);
    }

    fn draw_raw_image_tinted(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), _cache_key: u64, tint_argb: u32) {
        self.blit_rgba(rect, rgba, size, tint_argb);
    }

    fn draw_image(&mut self, rect: Rect<i32>, image_bytes: &[u8], cache_key: u64) {
        self.draw_image_tinted(rect, image_bytes, cache_key, 0xFFFFFFFF);
    }

    fn draw_image_tinted(&mut self, rect: Rect<i32>, image_bytes: &[u8], cache_key: u64, tint_argb: u32) {
        if !self.image_cache.contains_key(&cache_key) {
            match image::load_from_memory(image_bytes) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let (w, h) = (rgba.width(), rgba.height());
                    self.image_cache.insert(cache_key, (rgba.into_raw(), w, h));
                }
                Err(e) => {
                    println!("Error decoding image: {}", e);
                    return;
                }
            }
        }
        if let Some((rgba, w, h)) = self.image_cache.get(&cache_key).cloned() {
            self.blit_rgba(rect, &rgba, (w, h), tint_argb);
        }
    }
}

impl<'h> SoftwareTheme<'h> {
    /// Blit straight RGBA8 (tinted, opacity-folded, premultiplied) at `rect`'s
    /// top-left, 1:1 (no scaling — sources are expected to be laid-out size; SVG
    /// is re-rasterized by `ImageSource`). A future enhancement adds scaling.
    fn blit_rgba(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), tint_argb: u32) {
        let (w, h) = size;
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        let opacity = self.current_opacity();
        let ta = ((tint_argb >> 24) & 0xff) as u32;
        let tr = ((tint_argb >> 16) & 0xff) as u32;
        let tg = ((tint_argb >> 8) & 0xff) as u32;
        let tb = (tint_argb & 0xff) as u32;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for i in 0..(w * h) as usize {
            let s = i * 4;
            let sr = rgba[s] as u32 * tr / 255;
            let sg = rgba[s + 1] as u32 * tg / 255;
            let sb = rgba[s + 2] as u32 * tb / 255;
            let sa = ((rgba[s + 3] as u32 * ta / 255) as f32 * opacity) as u32;
            // premultiply
            data[s] = (sr * sa / 255) as u8;
            data[s + 1] = (sg * sa / 255) as u8;
            data[s + 2] = (sb * sa / 255) as u8;
            data[s + 3] = sa as u8;
        }
        let Some(isize) = IntSize::from_wh(w, h) else { return };
        let Some(src) = Pixmap::from_vec(data, isize) else { return };
        let dest = crate::types::rect(
            (rect.min.x, rect.min.y),
            (rect.min.x + w as i32, rect.min.y + h as i32),
        );
        let mask = match self.clip_decision(dest) {
            ClipDecision::Skip => return,
            ClipDecision::NoMask => None,
            ClipDecision::Masked => self.clip_mask.as_ref(),
        };
        self.pixmap.as_mut().draw_pixmap(
            rect.min.x,
            rect.min.y,
            src.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            mask,
        );
    }
}

/// Build a rectangular coverage mask (the clip) the size of the pixmap.
fn build_rect_mask(width: u32, height: u32, clip: Rect<i32>) -> Option<Mask> {
    let mut mask = Mask::new(width, height)?;
    let r = TsRect::from_ltrb(
        clip.min.x.max(0) as f32,
        clip.min.y.max(0) as f32,
        clip.max.x.max(0) as f32,
        clip.max.y.max(0) as f32,
    )?;
    let mut pb = PathBuilder::new();
    pb.push_rect(r);
    let path = pb.finish()?;
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
    Some(mask)
}
