//! Software [`RenderSurface`]: a `tiny_skia::Pixmap` painted by [`SoftwareTheme`]
//! and blitted (RGBA → 0RGB) to a softbuffer surface. Owns the per-window image
//! and glyph caches. This is the one place the neutral window loop touches
//! softbuffer / tiny-skia; the GL surface (Phase 3) is the parallel impl.

use std::num::NonZeroU32;
use std::rc::Rc;

use tiny_skia::Pixmap;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

use super::RenderSurface;
use crate::drawing::{DrawableRegistry, Palette};
use crate::themes::{GlyphCache, SoftwareImageCache, SoftwareTheme};
use crate::ui::UI;

type SbContext = softbuffer::Context<Rc<Window>>;
type SbSurface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

/// App-level software backend state: the shared softbuffer context (created from
/// the first window, reused for the rest) plus the per-window surface factory.
#[derive(Default)]
pub struct SoftwareBackend {
    context: Option<SbContext>,
}

impl SoftwareBackend {
    pub fn new() -> Self {
        SoftwareBackend::default()
    }

    /// Create a winit window and its software surface. Returns `None` (logging
    /// the cause) if the window can't be created.
    pub fn create(&mut self, event_loop: &ActiveEventLoop, attrs: WindowAttributes) -> Option<(Rc<Window>, SoftwareSurface)> {
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("window: failed to create window: {e}");
                return None;
            }
        };
        let size = window.inner_size();
        let (width, height) = (size.width.max(1), size.height.max(1));
        if self.context.is_none() {
            self.context = Some(softbuffer::Context::new(window.clone()).expect("softbuffer context"));
        }
        let mut surface = softbuffer::Surface::new(self.context.as_ref().unwrap(), window.clone())
            .expect("softbuffer surface");
        let _ = surface.resize(NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap());
        let pixmap = Pixmap::new(width, height).expect("pixmap");
        let sfc = SoftwareSurface {
            surface,
            pixmap,
            image_cache: SoftwareImageCache::new(),
            glyph_cache: GlyphCache::new(),
            width,
            height,
        };
        Some((window, sfc))
    }
}

/// Per-window software render target.
pub struct SoftwareSurface {
    surface: SbSurface,
    pixmap: Pixmap,
    image_cache: SoftwareImageCache,
    glyph_cache: GlyphCache,
    width: u32,
    height: u32,
}

impl RenderSurface for SoftwareSurface {
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        if let Some(pm) = Pixmap::new(width, height) {
            self.pixmap = pm;
        }
    }

    fn paint(&mut self, ui: &UI, palette: &Palette, registry: &DrawableRegistry, scale: f64) {
        // Free textures whose ImageSource was dropped or re-rasterized since the
        // last frame.
        crate::image_source::drain_evictions(&mut self.image_cache);

        {
            let mut theme = SoftwareTheme::new(
                &mut self.pixmap,
                registry,
                palette,
                &mut self.image_cache,
                &mut self.glyph_cache,
                self.width as i32,
                self.height as i32,
                scale,
            );
            ui.paint(&mut theme);
        }

        let (w, h) = (self.width, self.height);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = self.surface.buffer_mut() else {
            return;
        };
        let src = self.pixmap.data(); // premultiplied RGBA8; opaque bg ⇒ premult == straight
        let n = (w * h) as usize;
        for i in 0..n {
            let s = i * 4;
            let r = src[s] as u32;
            let g = src[s + 1] as u32;
            let b = src[s + 2] as u32;
            buf[i] = (r << 16) | (g << 8) | b;
        }
        let _ = buf.present();
    }
}
