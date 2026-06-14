#[macro_use]
extern crate downcast_rs;

// Re-export so downstream apps can use the windowing types without
// depending on (and version-matching) speedy2d themselves.
pub use speedy2d;

pub mod common;
pub mod ui;
pub mod events;
pub mod traits;
pub mod containers;
pub mod dialog;
pub mod layout;
pub mod background;
pub mod image_source;
pub mod views;
pub mod win;
pub mod themes;
pub mod types;
pub mod assets;
pub mod styles;
pub mod view_base;
pub mod shortcut;
pub mod drawing;
pub mod prelude;
pub mod svg;

#[cfg(test)]
mod tests;