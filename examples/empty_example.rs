#![windows_subsystem = "windows"]

use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const TITLE: &str = "Empty Frame Demo";

fn main() {
    let layout = include_str!("empty_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(
        window_size,
        Some(WindowPosition::PrimaryMonitorPixelsFromTopLeft(Vector2::new(10, 10))),
    );
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}
