//! Lumio — a declarative, XML-based retained-mode GUI toolkit for Rust desktop
//! apps. Define the view tree in XML, load it with `UI::from_xml`, wire event
//! handlers, and run with the backend-neutral `lumio::run` launcher:
//!
//! ```no_run
//! use lumio::prelude::*;
//! # fn demo() {
//! let ui = UI::from_xml(
//!     r#"<Frame padding="16"><Label text="Hello, Lumio!"/></Frame>"#,
//!     400, 200, default_typeface(), 1.0,
//! ).unwrap();
//! lumio::run(ui, WindowConfig::new("Demo", 400, 200).center());
//! # }
//! ```
//!
//! # Rendering backends
//!
//! Both backends run on one Lumio-owned winit window loop; pick them via Cargo
//! features:
//!
//! - `backend-gl` (default) — OpenGL via the vendored `speedy2d`, used as a renderer.
//! - `backend-software` — CPU rendering via `tiny-skia` + `fontdue`, plus a headless
//!   UI → `tiny_skia::Pixmap`/PNG path (the `render` module, software only).
//! - **Both together** — the runtime tries GL first and automatically falls back
//!   to software rendering if GL initialization fails (e.g. a VM with only an
//!   emulated framebuffer). The `LUMIO_BACKEND` environment variable (`gl` or
//!   `software`) forces a backend; [`backend::active_backend`] reports the one
//!   in use.
//!
//! See the README for the full widget list and a fuller tour.

#[macro_use]
extern crate downcast_rs;

// Re-export the renderer crate so GL-backend apps can name speedy2d render types
// (e.g. `Color`) without version-matching it themselves. Only the GL backend
// pulls in speedy2d (as a renderer — its windowing feature is off); the software
// backend builds without it.
#[cfg(feature = "backend-gl")]
pub use speedy2d;

// Re-export the accessibility vocabulary (Role/Node/TreeUpdate) so downstream
// custom `View` impls build their nodes against the same accesskit version.
pub use accesskit;

pub mod app;
pub use app::WindowConfig;
/// Runtime render-backend selection (GL → software fallback in dual builds).
pub mod backend;
pub use backend::{RenderBackend, active_backend};
// `run` opens a window, so it exists only with a windowed backend; `WindowConfig`
// is windowing-neutral and stays available to headless embedders.
#[cfg(any(feature = "backend-gl", feature = "backend-software"))]
pub use app::run;
pub mod clipboard;
pub mod common;
pub mod input;
pub mod text;
pub mod ui;
pub mod events;
pub mod traits;
pub mod containers;
pub mod dialog;
pub mod layout;
pub mod background;
pub mod image_source;
pub mod views;
pub mod themes;
pub mod types;
pub mod assets;
pub mod styles;
pub mod view_base;
/// Screen-reader support: builds the per-window AccessKit tree from the view
/// hierarchy and routes AT action requests back into the UI.
pub mod accessibility;
pub mod shortcut;
pub mod drawing;
/// Headless software rendering (UI → `tiny_skia::Pixmap`). Software core only.
#[cfg(feature = "software")]
pub mod render;
/// Backend-neutral winit window loop, shared by both backends; the per-window
/// paint sits behind a `RenderSurface` (GL or software). See
/// docs/unified_window_loop.md.
#[cfg(any(feature = "backend-gl", feature = "backend-software"))]
pub mod window;
pub mod prelude;
pub mod svg;

#[cfg(test)]
mod tests;