#[macro_use]
extern crate downcast_rs;
extern crate quick_xml;
extern crate speedy2d;
extern crate rand;

pub mod gui;
pub use gui::*;
#[cfg(test)]
mod tests;