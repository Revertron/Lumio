//! Headless software rendering: lay out a UI and paint it into a
//! `tiny_skia::Pixmap` with the [`SoftwareTheme`](crate::themes::SoftwareTheme),
//! no window required. Useful
//! for tests, screenshots, and as the foundation the software window loop will
//! reuse. Only available under the `backend-software` feature.

use tiny_skia::Pixmap;

use crate::drawing::{DrawableRegistry, Palette};
use crate::themes::{GlyphCache, SoftwareImageCache, SoftwareTheme};
use crate::ui::UI;

/// Paint an already-laid-out `ui` into a fresh `width`×`height` pixmap at the
/// given DPI `scale`. Returns `None` only if the pixmap could not be allocated
/// (zero or absurd dimensions).
///
/// In a dual-backend build (`backend-gl` + `backend-software`), call
/// `lumio::backend::set_render_backend(RenderBackend::Software)` **before**
/// building and laying out the UI: text is shaped when the UI lays out, and
/// only software-shaped text can be painted here.
pub fn render_to_pixmap(
    ui: &UI,
    width: u32,
    height: u32,
    scale: f64,
    palette: &Palette,
    registry: &DrawableRegistry,
) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)?;
    let mut image_cache = SoftwareImageCache::new();
    let mut glyph_cache = GlyphCache::new();
    {
        let mut theme = SoftwareTheme::new(
            &mut pixmap,
            registry,
            palette,
            &mut image_cache,
            &mut glyph_cache,
            width as i32,
            height as i32,
            scale,
        );
        ui.paint(&mut theme);
    }
    Some(pixmap)
}

/// Paint ONE overlay of an already-laid-out `ui` — identified by its
/// [`OverlayDesc`](crate::ui::OverlayDesc) token, or the tooltip via
/// [`TOOLTIP_TOKEN`](crate::ui::TOOLTIP_TOKEN) — into a fresh pixmap of the overlay's own size,
/// origin at the overlay rect's top-left. For external-popups embedders
/// ([`UI::set_external_popups`]) that present each overlay in its own surface. Returns `None`
/// if the token is gone or the pixmap could not be allocated.
pub fn render_overlay_to_pixmap(
    ui: &UI,
    token: u64,
    width: u32,
    height: u32,
    scale: f64,
    palette: &Palette,
    registry: &DrawableRegistry,
) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)?;
    let mut image_cache = SoftwareImageCache::new();
    let mut glyph_cache = GlyphCache::new();
    let painted = {
        let mut theme = SoftwareTheme::new(
            &mut pixmap,
            registry,
            palette,
            &mut image_cache,
            &mut glyph_cache,
            width as i32,
            height as i32,
            scale,
        );
        ui.paint_overlay(token, &mut theme)
    };
    painted.then_some(pixmap)
}
