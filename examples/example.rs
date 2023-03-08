#![windows_subsystem = "windows"]

extern crate include_dir;
extern crate speedy2d;
extern crate vinx;

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use vinx::assets::{AssetsProvider, set_provider};
use vinx::events::EventType;
use vinx::themes::Classic;
use vinx::themes::Theme;
use vinx::traits::View;
use vinx::ui::UI;
use vinx::views::{Button, CheckBox, Edit, List};
use vinx::win::{Win, WinEvent};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const TITLE: &'static str = "VinX";

// Usually you will not use the `examples` part
const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>
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

    let layout = include_str!("layout.xml");
    let mut ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface()).unwrap();

    if let Some(button) = ui.get_view("btn1") {
        button.borrow_mut().on_event(EventType::Click, Box::new(button1_click));
    }

    if let Some(button) = ui.get_view("btn2") {
        button.borrow_mut().on_event(EventType::Click, Box::new(button2_click));
    }

    ui.on_start(Box::new(on_start));

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}

fn button1_click(ui: &mut UI, view: &dyn View) -> bool {
    let mut checked = false;
    if let Some(checkbox) = ui.get_view("checkbox1") {
        if let Some(ch) = checkbox.borrow_mut().downcast_mut::<CheckBox>() {
            checked = ch.is_checked();
        }
    }

    // Change something in another view
    if let Some(edit) = ui.get_view("edit1") {
        if let Some(e) = edit.borrow_mut().downcast_mut::<Edit>() {
            e.set_text(&format!("CheckBox checked = {}", checked));
        }
    }
    // Change something in clicked view
    if let Some(button) = view.as_any().downcast_ref::<Button>() {
        button.set_text("Clicked!");
    }
    true
}

fn button2_click(ui: &mut UI, _view: &dyn View) -> bool {
    let mut buf = Vec::new();
    for i in 1..=20 {
        buf.push(format!("New item {}", i));
    }
    // Set items for list
    if let Some(list) = ui.get_view("list1") {
        if let Some(list) = list.borrow_mut().downcast_mut::<List>() {
            list.set_items(buf);
        }
    }
    true
}

fn on_start(ui: &mut UI) {
    let mut buf = Vec::new();
    for i in 1..=20 {
        buf.push(format!("Start item {}", i));
    }
    // Set items for list
    if let Some(list) = ui.get_view("list1") {
        if let Some(list) = list.borrow_mut().downcast_mut::<List>() {
            list.set_items(buf);
        }
    }
}