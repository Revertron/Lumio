//! GL [`RenderSurface`]: a host-owned glutin context + `speedy2d::GLRenderer`,
//! painted by [`RendererGL`]. The GL parallel of `surface_software.rs`.
//!
//! speedy2d is used purely as a renderer here (its `windowing` feature is off);
//! Lumio owns the winit window and the GL context. The glutin setup mirrors what
//! the vendored speedy2d does internally (`window_internal_glutin.rs`); it was
//! validated standalone by the Phase-1 spike before landing here.

use std::ffi::CString;
use std::num::NonZeroU32;
use std::rc::Rc;

use log::error;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext,
    PossiblyCurrentGlContext, Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{
    GlSurface as _, Surface as GlutinSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface,
};
use glutin_winit::{DisplayBuilder, GlWindow};
use raw_window_handle::HasWindowHandle;
use speedy2d::GLRenderer;
use speedy2d::dimen::UVec2;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

use super::RenderSurface;
use crate::drawing::{DrawableRegistry, Palette};
use crate::themes::{RendererGL, ImageCache};
use crate::ui::UI;

/// GL backend: a stateless window + surface factory. Each window gets its own
/// glutin display/context/surface/renderer — Lumio doesn't share GL resources
/// across windows (image caches are per-surface). Parallels `SoftwareBackend`.
#[derive(Default)]
pub struct GlBackend;

impl GlBackend {
    pub fn new() -> Self {
        GlBackend
    }

    /// Create a winit window with a matching GL context, surface, and speedy2d
    /// renderer. Returns `None` (logging the cause) on any GL setup failure.
    pub fn create(&mut self, event_loop: &ActiveEventLoop, attrs: WindowAttributes) -> Option<(Rc<Window>, GlSurface)> {
        let template = ConfigTemplateBuilder::new().with_alpha_size(8);
        // glutin-winit's config picker must return a `Config` by value, so an
        // empty config list (e.g. a VM with only an emulated framebuffer) can
        // only surface as a panic — catch it and fail gracefully like every
        // other GL setup error, so a dual-backend build can fall back to
        // software rendering.
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            DisplayBuilder::new()
                .with_window_attributes(Some(attrs.clone()))
                .build(event_loop, template, |configs| {
                    // Prefer the config with the most MSAA samples.
                    configs
                        .reduce(|a, b| if b.num_samples() > a.num_samples() { b } else { a })
                        .expect("no GL config")
                })
        }));
        let (mut window, gl_config) = match built {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                error!("window: GL display build failed: {e}");
                return None;
            }
            Err(_) => {
                error!("window: no suitable GL config found");
                return None;
            }
        };

        let gl_display = gl_config.display();
        let raw = window.as_ref().and_then(|w| w.window_handle().ok()).map(|h| h.as_raw());
        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(2, 0))))
            .build(raw);
        let not_current = match unsafe { gl_display.create_context(&gl_config, &context_attributes) } {
            Ok(c) => c,
            Err(e) => {
                error!("window: GL create_context failed: {e}");
                return None;
            }
        };

        // On X11 the window must be (re)created to match the chosen config;
        // elsewhere DisplayBuilder already produced it.
        let window: Window = match window.take() {
            Some(w) => w,
            None => match glutin_winit::finalize_window(event_loop, attrs, &gl_config) {
                Ok(w) => w,
                Err(e) => {
                    error!("window: finalize_window failed: {e}");
                    return None;
                }
            },
        };

        let surf_attrs = match window.build_surface_attributes(SurfaceAttributesBuilder::default()) {
            Ok(a) => a,
            Err(e) => {
                error!("window: build_surface_attributes failed: {e}");
                return None;
            }
        };
        let surface = match unsafe { gl_display.create_window_surface(&gl_config, &surf_attrs) } {
            Ok(s) => s,
            Err(e) => {
                error!("window: create_window_surface failed: {e}");
                return None;
            }
        };
        let context = match not_current.make_current(&surface) {
            Ok(c) => c,
            Err(e) => {
                error!("window: make_current failed: {e}");
                return None;
            }
        };
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let renderer = match unsafe {
            GLRenderer::new_for_gl_context(UVec2::new(w, h), |symbol: &str| {
                let symbol = CString::new(symbol).unwrap();
                gl_display.get_proc_address(symbol.as_c_str())
            })
        } {
            Ok(r) => r,
            Err(e) => {
                error!("window: GLRenderer creation failed: {e}");
                return None;
            }
        };

        let window = Rc::new(window);
        let gl_surface = GlSurface {
            surface,
            context,
            renderer,
            image_cache: ImageCache::new(),
            width: w,
            height: h,
            _window: Rc::clone(&window),
        };
        Some((window, gl_surface))
    }
}

/// Per-window GL render target: the glutin surface + (current) context plus the
/// speedy2d renderer and this window's GPU image cache.
pub struct GlSurface {
    surface: GlutinSurface<WindowSurface>,
    context: PossiblyCurrentContext,
    renderer: GLRenderer,
    image_cache: ImageCache,
    width: u32,
    height: u32,
    /// Keeps the winit window alive until everything above is dropped (fields
    /// drop in declaration order, so this must stay last). Closing a window
    /// drops `WindowState`, whose `window` field comes first — without this
    /// reference the X11 window would be destroyed before glutin's
    /// `glXDestroyWindow`, which then fails with `GLXBadWindow`. X errors are
    /// asynchronous, so that error surfaces later inside an unrelated winit
    /// call (`XSetICFocus` → `check_errors`), which panics. The software
    /// surface is immune only because softbuffer holds the window itself.
    _window: Rc<Window>,
}

impl RenderSurface for GlSurface {
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            self.surface.resize(&self.context, w, h);
            self.renderer.set_viewport_size_pixels(UVec2::new(width, height));
        }
    }

    fn paint(&mut self, ui: &UI, palette: &Palette, registry: &DrawableRegistry, scale: f64) {
        // This window's context must be current before touching its GL resources
        // (multi-window: each window owns a context) — and before dropping evicted
        // ImageHandles, which free GL textures.
        if self.context.make_current(&self.surface).is_err() {
            return;
        }
        crate::image_source::drain_evictions(&mut self.image_cache);

        let (w, h) = (self.width as i32, self.height as i32);
        {
            let renderer = &mut self.renderer;
            let image_cache = &mut self.image_cache;
            renderer.draw_frame(|graphics| {
                let mut theme = RendererGL::new(graphics, registry, palette, image_cache, w, h, scale);
                ui.paint(&mut theme);
            });
        }
        let _ = self.surface.swap_buffers(&self.context);
    }
}
