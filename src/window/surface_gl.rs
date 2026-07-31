//! GL [`RenderSurface`]: a host-owned glutin context + `speedy2d::GLRenderer`,
//! painted by [`RendererGL`]. The GL parallel of `surface_software.rs`.
//!
//! speedy2d is used purely as a renderer here (its `windowing` feature is off);
//! Lumio owns the winit window and the GL context. The glutin setup mirrors what
//! the vendored speedy2d does internally (`window_internal_glutin.rs`); it was
//! validated standalone by the Phase-1 spike before landing here.
//!
//! ## Context loss
//!
//! A GL context does not survive the graphics driver being replaced underneath
//! it — updating the display driver, disabling the adapter, a TDR. Afterwards
//! every `make_current`/`swap_buffers` fails and the window is frozen on its
//! last frame, which reads as a hung app. [`GlSurface`] therefore splits its GL
//! objects into a replaceable [`Live`] half: the first failure drops it, and the
//! surface then rebuilds against the same window and [`Config`] on a backoff
//! schedule until the adapter comes back. See `WindowState::render` for the
//! matching rule that an off-screen window is not painted at all.
//!
//! ## Machines without OpenGL 2.0
//!
//! A context is not guaranteed to honour the version asked for: without
//! `WGL_ARB_create_context` glutin falls back to a legacy `wglCreateContext`,
//! which on a machine with no GPU driver (Remote Desktop, a VM without 3D, the
//! Microsoft Basic Display Adapter) yields Windows' GDI generic OpenGL **1.1**.
//! speedy2d assumes 2.0 and starts compiling shaders, so the first
//! `glCreateShader` — a null entry point there — panics inside `glow` rather
//! than failing. [`check_gl_version`] rejects such a context up front, turning
//! the crash into the ordinary "GL setup failed" path: a software fallback in a
//! dual-backend build (see `App::create_surface`), a logged error otherwise.

use std::ffi::{CStr, CString, c_void};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};

use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext,
    PossiblyCurrentGlContext, Version,
};
use glutin::display::{Display, GetGlDisplay, GlDisplay};
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

/// Rebuild pacing after a context loss. A driver install can keep the adapter
/// unavailable for tens of seconds, so attempts continue indefinitely, backing
/// off from `RETRY_MIN` to `RETRY_MAX` — neither hammering a driver that is
/// mid-install nor giving up before it returns.
const RETRY_MIN: Duration = Duration::from_millis(250);
const RETRY_MAX: Duration = Duration::from_secs(2);

/// The GL version speedy2d's shader pipeline needs (it hard-codes
/// `GLVersion::OpenGL2_0` in `GLRenderer::new_for_gl_context`).
const MIN_GL_VERSION: (u32, u32) = (2, 0);

/// `GL_VERSION` — the one GL enum used here, spelled out rather than pulling in
/// a bindings crate for a single call.
const GL_VERSION: u32 = 0x1F02;

/// Check that the *current* context really implements OpenGL 2.0, before
/// speedy2d assumes it does. `glGetString` is OpenGL 1.1, so it is exported by
/// `opengl32.dll`/`libGL` even on the implementations this guards against.
///
/// A version string that cannot be parsed is accepted: a driver with unusual
/// spelling should not be locked out, and the `catch_unwind` around the
/// renderer still contains the fallout if it really is too old.
fn check_gl_version(display: &Display) -> Result<(), String> {
    type GlGetString = unsafe extern "system" fn(u32) -> *const u8;

    let symbol = CString::new("glGetString").unwrap();
    let ptr = display.get_proc_address(symbol.as_c_str());
    if ptr.is_null() {
        return Err("glGetString is missing — the GL context is unusable".to_string());
    }
    let get_string = unsafe { std::mem::transmute::<*const c_void, GlGetString>(ptr) };
    let raw = unsafe { get_string(GL_VERSION) };
    if raw.is_null() {
        return Err("glGetString(GL_VERSION) returned null — the GL context is unusable".to_string());
    }
    let version = unsafe { CStr::from_ptr(raw.cast()) }.to_string_lossy().into_owned();

    match parse_gl_version(&version) {
        Some(found) if found < MIN_GL_VERSION => Err(format!(
            "OpenGL {}.{} is required, but this context reports \"{version}\" — the machine has no \
             usable GPU driver (Windows' GDI generic renderer, a Remote Desktop session, or a VM \
             without 3D acceleration)",
            MIN_GL_VERSION.0, MIN_GL_VERSION.1
        )),
        Some(_) => Ok(()),
        None => {
            warn!("window: unrecognized GL version string {version:?}; assuming it is new enough");
            Ok(())
        }
    }
}

/// Pull the `major.minor` out of a `GL_VERSION` string. The spec fixes the
/// leading token as `major.minor[.release]` optionally prefixed by `OpenGL ES`
/// (`"4.6.0 NVIDIA 560.94"`, `"OpenGL ES 3.2 Mesa 24.0"`, `"1.1.0"`), so take
/// the first token that starts with a digit and read the two numbers off it.
fn parse_gl_version(version: &str) -> Option<(u32, u32)> {
    let head = version
        .split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut parts = head.split('.');
    let major = parts.next()?.parse().ok()?;
    // Trailing junk on the minor part is tolerated: some drivers append a build
    // tag directly ("3.2build1"), and the digits before it are still the minor.
    let digits: String = parts.next()?.chars().take_while(char::is_ascii_digit).collect();
    Some((major, digits.parse().ok()?))
}

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
        let (window, gl_config) = match built {
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

        // On X11 the window must be (re)created to match the chosen config;
        // elsewhere DisplayBuilder already produced it.
        let window: Window = match window {
            Some(w) => w,
            None => match glutin_winit::finalize_window(event_loop, attrs, &gl_config) {
                Ok(w) => w,
                Err(e) => {
                    error!("window: finalize_window failed: {e}");
                    return None;
                }
            },
        };

        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));
        let live = match Live::create(&gl_config, &window, w, h) {
            Ok(live) => live,
            Err(e) => {
                error!("window: {e}");
                return None;
            }
        };

        let window = Rc::new(window);
        let gl_surface = GlSurface {
            live: Some(live),
            image_cache: ImageCache::new(),
            config: gl_config,
            width: w,
            height: h,
            gl_failures: 0,
            rebuilds: 0,
            retry_at: Instant::now(),
            retry_backoff: RETRY_MIN,
            window: Rc::clone(&window),
        };
        Some((window, gl_surface))
    }
}

/// The volatile half of a [`GlSurface`]: everything a lost context invalidates
/// and a rebuild replaces. Kept together so the whole set can be dropped and
/// recreated as a unit.
struct Live {
    surface: GlutinSurface<WindowSurface>,
    context: PossiblyCurrentContext,
    renderer: GLRenderer,
}

impl Live {
    /// Build the GL surface, context and renderer for `window` against
    /// `config`. Shared by first-time creation and post-loss rebuild, so a
    /// rebuilt context is set up exactly like the original one. The error is
    /// returned rather than logged: a first-time failure is worth an `error!`,
    /// while a retry during a driver install is expected and merely noisy.
    fn create(config: &Config, window: &Window, width: u32, height: u32) -> Result<Live, String> {
        let gl_display = config.display();
        let raw = window.window_handle().ok().map(|h| h.as_raw());
        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(2, 0))))
            .build(raw);
        let not_current = unsafe { gl_display.create_context(config, &context_attributes) }
            .map_err(|e| format!("GL create_context failed: {e}"))?;

        let surf_attrs = window
            .build_surface_attributes(SurfaceAttributesBuilder::default())
            .map_err(|e| format!("build_surface_attributes failed: {e}"))?;
        let surface = unsafe { gl_display.create_window_surface(config, &surf_attrs) }
            .map_err(|e| format!("create_window_surface failed: {e}"))?;
        let context = not_current
            .make_current(&surface)
            .map_err(|e| format!("make_current failed: {e}"))?;
        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        // Only meaningful with the context current, which it now is.
        check_gl_version(&gl_display)?;

        // speedy2d resolves the GL entry points it needs through the loader and
        // calls them unconditionally; a missing one panics inside `glow` instead
        // of returning an error. The version check above catches the case that
        // actually happens in the field, and this contains anything else — an
        // implementation that reports 2.0 yet omits an entry point — so it is a
        // failed window rather than a dead app.
        let renderer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            GLRenderer::new_for_gl_context(UVec2::new(width.max(1), height.max(1)), |symbol: &str| {
                let symbol = CString::new(symbol).unwrap();
                gl_display.get_proc_address(symbol.as_c_str())
            })
        }))
        .map_err(|_| {
            "GLRenderer creation panicked — a GL entry point speedy2d needs is not available"
                .to_string()
        })?
        .map_err(|e| format!("GLRenderer creation failed: {e}"))?;

        Ok(Live { surface, context, renderer })
    }
}

/// Per-window GL render target: the glutin surface + (current) context plus the
/// speedy2d renderer and this window's GPU image cache.
pub struct GlSurface {
    /// The GL objects, or `None` between losing the context and a successful
    /// rebuild. Declared first so it drops before the image cache and the
    /// window, as these fields did when they were spelled out here.
    live: Option<Live>,
    image_cache: ImageCache,
    /// The config the context and surface are built from, kept so a rebuild can
    /// reuse it. Reusing it is what makes an in-place rebuild portable: on X11
    /// the window was created to match this config's visual, so a *different*
    /// config would require a new window too.
    config: Config,
    width: u32,
    height: u32,
    /// Consecutive frames that failed to present (`make_current` or
    /// `swap_buffers`); reset by the first frame that presents cleanly. Drives
    /// the rate-limited logging in [`Self::note_failure`].
    gl_failures: u32,
    /// Rebuild attempts since the context was lost; reset once one succeeds.
    rebuilds: u32,
    /// When the next rebuild may be attempted while `live` is `None`.
    retry_at: Instant,
    /// Current wait between rebuild attempts, growing to `RETRY_MAX`.
    retry_backoff: Duration,
    /// Keeps the winit window alive until everything above is dropped (fields
    /// drop in declaration order, so this must stay last) — and provides the
    /// window a rebuild builds its new surface against. Closing a window
    /// drops `WindowState`, whose `window` field comes first — without this
    /// reference the X11 window would be destroyed before glutin's
    /// `glXDestroyWindow`, which then fails with `GLXBadWindow`. X errors are
    /// asynchronous, so that error surfaces later inside an unrelated winit
    /// call (`XSetICFocus` → `check_errors`), which panics. The software
    /// surface is immune only because softbuffer holds the window itself.
    window: Rc<Window>,
}

impl GlSurface {
    /// Record a failed presentation step and log it, rate-limited: the first
    /// failure, then every 600th, so a lost context leaves a trail in the log
    /// without flooding it.
    fn note_failure(&mut self, step: &str, err: glutin::error::Error) {
        self.gl_failures += 1;
        if self.gl_failures == 1 || self.gl_failures.is_multiple_of(600) {
            error!(
                "window: GL {step} failed on {} consecutive frame(s): {err} — the context is \
                 probably lost (display-driver update, adapter reset); rebuilding it",
                self.gl_failures
            );
        }
    }

    /// Give up on the current GL objects after a failed presentation step and
    /// schedule a rebuild.
    fn lose_context(&mut self, step: &str, err: glutin::error::Error) {
        self.note_failure(step, err);
        // Drop everything the dead context owned, including this window's
        // textures: the cached `ImageHandle`s are names on a GPU that is gone.
        // `RendererGL` re-uploads on a cache miss, so clearing costs one
        // re-upload per image on the first frame that presents again.
        self.live = None;
        self.image_cache.clear();
        // Try again on the very next frame: a one-off failure recovers at once,
        // and a real outage starts backing off from there.
        self.retry_at = Instant::now();
        self.retry_backoff = RETRY_MIN;
    }

    /// Attempt to rebuild the GL objects, at most once per backoff interval.
    /// Returns whether the surface is usable afterwards.
    fn try_rebuild(&mut self) -> bool {
        if Instant::now() < self.retry_at {
            return false;
        }
        self.rebuilds += 1;
        match Live::create(&self.config, &self.window, self.width, self.height) {
            Ok(live) => {
                self.live = Some(live);
                warn!(
                    "window: GL context rebuilt after {} failed frame(s) and {} attempt(s)",
                    self.gl_failures, self.rebuilds
                );
                self.rebuilds = 0;
                true
            }
            Err(e) => {
                // Expected while a driver install has the adapter offline, so
                // report periodically rather than once per attempt.
                if self.rebuilds == 1 || self.rebuilds.is_multiple_of(10) {
                    error!("window: GL rebuild attempt {} failed: {e}", self.rebuilds);
                } else {
                    debug!("window: GL rebuild attempt {} failed: {e}", self.rebuilds);
                }
                self.retry_backoff = (self.retry_backoff * 2).min(RETRY_MAX);
                self.retry_at = Instant::now() + self.retry_backoff;
                false
            }
        }
    }
}

impl RenderSurface for GlSurface {
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        // While the context is lost there is nothing to resize; the rebuild
        // picks the new size up from the fields above.
        let Some(live) = self.live.as_mut() else { return };
        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            live.surface.resize(&live.context, w, h);
            live.renderer.set_viewport_size_pixels(UVec2::new(width, height));
        }
    }

    fn needs_repaint(&self) -> bool {
        // Nothing in the UI schedules a frame while the context is down, so the
        // loop has to keep asking — this is what drives `try_rebuild`.
        self.live.is_none()
    }

    fn paint(&mut self, ui: &UI, palette: &Palette, registry: &DrawableRegistry, scale: f64) {
        // Lost context: retry on the backoff schedule, and skip the frame until
        // one of those attempts succeeds.
        if self.live.is_none() && !self.try_rebuild() {
            return;
        }

        // This window's context must be current before touching its GL resources
        // (multi-window: each window owns a context) — and before dropping evicted
        // ImageHandles, which free GL textures.
        {
            let Some(live) = self.live.as_mut() else { return };
            if let Err(e) = live.context.make_current(&live.surface) {
                self.lose_context("make_current", e);
                return;
            }
        }
        crate::image_source::drain_evictions(&mut self.image_cache);

        let (w, h) = (self.width as i32, self.height as i32);
        {
            let Some(live) = self.live.as_mut() else { return };
            let renderer = &mut live.renderer;
            let image_cache = &mut self.image_cache;
            renderer.draw_frame(|graphics| {
                let mut theme = RendererGL::new(graphics, registry, palette, image_cache, w, h, scale);
                ui.paint(&mut theme);
            });
        }

        {
            let Some(live) = self.live.as_mut() else { return };
            if let Err(e) = live.surface.swap_buffers(&live.context) {
                self.lose_context("swap_buffers", e);
                return;
            }
        }
        if self.gl_failures > 0 {
            info!("window: GL presentation recovered after {} failed frame(s)", self.gl_failures);
            self.gl_failures = 0;
            self.retry_backoff = RETRY_MIN;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MIN_GL_VERSION, parse_gl_version};

    #[test]
    fn parses_real_gl_version_strings() {
        assert_eq!(parse_gl_version("4.6.0 NVIDIA 560.94"), Some((4, 6)));
        assert_eq!(parse_gl_version("3.3.0 - Build 27.20.100.9316"), Some((3, 3)));
        assert_eq!(parse_gl_version("4.5 (Compatibility Profile) Mesa 24.0.9"), Some((4, 5)));
        assert_eq!(parse_gl_version("OpenGL ES 3.2 Mesa 24.0.9"), Some((3, 2)));
        // What the GDI generic implementation reports — the case this guards.
        assert_eq!(parse_gl_version("1.1.0"), Some((1, 1)));
        assert_eq!(parse_gl_version("3.2build1"), Some((3, 2)));
    }

    #[test]
    fn unparseable_version_strings_yield_none() {
        assert_eq!(parse_gl_version(""), None);
        assert_eq!(parse_gl_version("OpenGL"), None);
        assert_eq!(parse_gl_version("4"), None);
    }

    #[test]
    fn only_below_2_0_is_rejected() {
        assert!(parse_gl_version("1.1.0").unwrap() < MIN_GL_VERSION);
        assert!(parse_gl_version("2.0.0").unwrap() >= MIN_GL_VERSION);
        assert!(parse_gl_version("4.6.0 NVIDIA 560.94").unwrap() >= MIN_GL_VERSION);
    }
}
