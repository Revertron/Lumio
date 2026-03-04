#[macro_use]
extern crate downcast_rs;

pub mod common;
pub mod ui;
pub mod events;
pub mod traits;
pub mod containers;
pub mod views;
pub mod win;
pub mod themes;
pub mod types;
pub mod assets;
pub mod styles;
pub mod view_base;
pub mod drawing;
pub mod prelude;

#[cfg(test)]
mod tests;