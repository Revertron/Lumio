#![windows_subsystem = "windows"]

use lumio::prelude::*;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const TITLE: &str = "Empty Frame Demo";

fn main() {
    let layout = include_str!("empty_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
