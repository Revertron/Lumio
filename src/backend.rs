//! Runtime render-backend selection.
//!
//! With a single backend feature enabled (`backend-gl` *or* `backend-software`)
//! everything here is a compile-time constant and this module adds nothing at
//! runtime. When **both** features are enabled, the window loop tries GL first
//! and falls back to software rendering if GL initialization fails (see
//! `App::create_surface` in `src/window/mod.rs`); the decision is made once, at
//! the first window, and recorded here so font loading and text shaping follow
//! the same backend. The `LUMIO_BACKEND` environment variable (`gl` or
//! `software`) forces a backend in such dual builds.

#[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
use std::cell::Cell;

/// Which rendering/text backend is active for this process.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderBackend {
    /// OpenGL rendering (`backend-gl`): speedy2d renderer + speedy2d text.
    Gl,
    /// CPU rendering (`backend-software`): tiny-skia renderer + fontdue text.
    Software,
}

// The selector exists only when both text backends are compiled in (keyed on
// the text features, not the window features: the headless `software` feature
// enables `text-software` without `backend-software`). Thread-local because all
// UI work — layout, shaping, painting — runs on the UI thread (same pattern as
// the font caches in `assets.rs`), and it keeps parallel test threads independent.
#[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
thread_local! {
    static ACTIVE: Cell<Option<RenderBackend>> = const { Cell::new(None) };
}

/// The backend currently used for font loading, text shaping and rendering.
///
/// Single-backend builds return a constant. Dual builds resolve lazily on first
/// call: `LUMIO_BACKEND` if set, otherwise GL (the window loop may later flip it
/// to software if GL initialization fails at the first window).
pub fn active_backend() -> RenderBackend {
    #[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
    {
        ACTIVE.with(|c| match c.get() {
            Some(b) => b,
            None => {
                let b = env_backend().unwrap_or(RenderBackend::Gl);
                c.set(Some(b));
                b
            }
        })
    }
    #[cfg(all(feature = "text-speedy2d", not(feature = "text-software")))]
    {
        RenderBackend::Gl
    }
    #[cfg(all(feature = "text-software", not(feature = "text-speedy2d")))]
    {
        RenderBackend::Software
    }
}

/// The backend requested via the `LUMIO_BACKEND` environment variable, if any.
/// Unknown values are ignored with a warning.
#[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
pub(crate) fn env_backend() -> Option<RenderBackend> {
    let value = std::env::var("LUMIO_BACKEND").ok()?;
    match value.to_ascii_lowercase().as_str() {
        "gl" => Some(RenderBackend::Gl),
        "software" => Some(RenderBackend::Software),
        other => {
            eprintln!("lumio: ignoring unknown LUMIO_BACKEND value {other:?} (expected \"gl\" or \"software\")");
            None
        }
    }
}

#[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
pub(crate) fn set_active_backend(backend: RenderBackend) {
    ACTIVE.with(|c| c.set(Some(backend)));
}

/// Force the render backend in a dual-backend build, overriding both the
/// GL-first default and `LUMIO_BACKEND`. Call before building any UI or shaping
/// any text (fonts are loaded for the backend active at shaping time). Headless
/// embedders that use [`crate::render`] in a dual build must call
/// `set_render_backend(RenderBackend::Software)` first so text is shaped by the
/// software backend.
#[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
pub fn set_render_backend(backend: RenderBackend) {
    set_active_backend(backend);
}
