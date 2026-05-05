#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 720;
const TITLE: &str = "Lumio — Edit polish demo";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider { dir: Dir<'static> }
impl Provider { fn new(dir: Dir<'static>) -> Self { Self { dir } } }
impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("edit_polish_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    if let Some(b) = ui.get_view("btn_toggle") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            if let Some(edit) = ui.get_view("edit_error") {
                if let Some(e) = edit.borrow_mut().downcast_mut::<Edit>() {
                    e.set_error(!e.is_error());
                }
            }
            true
        }));
    }

    // Right-icon click on the search field clears its text.
    if let Some(e) = ui.get_view("edit_right") {
        e.borrow_mut().on_event(EventType::RightIconClick, Box::new(|_ui, view| {
            if let Some(edit) = view.as_any().downcast_ref::<Edit>() {
                edit.set_text("");
            }
            true
        }));
    }

    // Left-icon click on the read-only field copies its content to clipboard.
    if let Some(e) = ui.get_view("edit_ro") {
        e.borrow_mut().on_event(EventType::LeftIconClick, Box::new(|_ui, view| {
            if let Some(edit) = view.as_any().downcast_ref::<Edit>() {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(edit.get_text());
                    println!("Copied to clipboard: {}", edit.get_text());
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
