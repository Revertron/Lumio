#[macro_use]
extern crate downcast_rs;

// Re-export so downstream apps can use the windowing types without
// depending on (and version-matching) speedy2d themselves.
pub use speedy2d;

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
/// The speedy2d (GL) window handler. Only built for the GL backend; the
/// software backend's window loop is a separate later step.
#[cfg(feature = "backend-gl")]
pub mod win;
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
pub mod prelude;
pub mod svg;

#[cfg(test)]
mod tests;