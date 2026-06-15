#[macro_use]
extern crate downcast_rs;

// Re-export so downstream apps can use the windowing types without
// depending on (and version-matching) speedy2d themselves. Only the GL
// backend pulls in speedy2d; the software backend builds without it.
#[cfg(feature = "backend-gl")]
pub use speedy2d;

pub mod app;
pub use app::{run, WindowConfig};
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
pub mod shortcut;
pub mod drawing;
/// Headless software rendering (UI → `tiny_skia::Pixmap`). Software backend only.
#[cfg(feature = "backend-software")]
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