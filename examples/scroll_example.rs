#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 600;
const TITLE: &str = "ScrollView Example";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl Provider {
    pub fn new(dir: Dir<'static>) -> Self {
        Self { dir }
    }
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        if let Some(file) = self.dir.get_file(path) {
            return Some(file.contents());
        }
        None
    }
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("scroll_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    if let Some(button) = ui.get_view("scroll_top") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view| {
            if let Some(sv) = ui.get_view("scroll_v") {
                if let Some(scroll) = sv.borrow().downcast_ref::<ScrollView>() {
                    scroll.scroll_to_start();
                }
            }
            true
        }));
    }

    if let Some(button) = ui.get_view("scroll_bottom") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view| {
            if let Some(sv) = ui.get_view("scroll_v") {
                if let Some(scroll) = sv.borrow().downcast_ref::<ScrollView>() {
                    scroll.scroll_to_end();
                }
            }
            true
        }));
    }

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}
