#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 800;
const TITLE: &str = "Visibility & Enabled Demo";

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
        self.dir.get_file(path).map(|f| f.contents())
    }
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("visibility_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    // "Toggle Enable" — toggles enabled state on target views
    if let Some(btn) = ui.get_view("btn_toggle_enable") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            let ids = ["target_btn", "target_cb", "target_edit", "target_rb1", "target_rb2"];
            let mut new_enabled = true;
            // Read current state from the first target
            if let Some(v) = ui.get_view("target_btn") {
                new_enabled = !v.borrow().is_enabled();
            }
            for id in &ids {
                if let Some(v) = ui.get_view(id) {
                    v.borrow_mut().set_enabled(new_enabled);
                }
            }
            // Update status label
            if let Some(label) = ui.get_view("status_label") {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    let state = if new_enabled { "enabled" } else { "disabled" };
                    l.set_text(&format!("Target views are now {}", state));
                }
            }
            true
        }));
    }

    // "Hide" — sets target views to Hidden (still occupy layout space)
    if let Some(btn) = ui.get_view("btn_hide") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            let ids = ["target_btn", "target_cb", "target_edit", "target_rb1", "target_rb2"];
            for id in &ids {
                if let Some(v) = ui.get_view(id) {
                    v.borrow_mut().set_visibility(Visibility::Hidden);
                }
            }
            if let Some(label) = ui.get_view("status_label") {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text("Target views are Hidden (space reserved, not painted)");
                }
            }
            true
        }));
    }

    // "Gone" — sets target views to Gone (no layout space)
    if let Some(btn) = ui.get_view("btn_gone") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            let ids = ["target_btn", "target_cb", "target_edit", "target_rb1", "target_rb2"];
            for id in &ids {
                if let Some(v) = ui.get_view(id) {
                    v.borrow_mut().set_visibility(Visibility::Gone);
                }
            }
            if let Some(label) = ui.get_view("status_label") {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text("Target views are Gone (no space, not painted)");
                }
            }
            // Relayout so Gone takes effect
            ui.relayout();
            true
        }));
    }

    // "Show" — sets target views back to Visible
    if let Some(btn) = ui.get_view("btn_show") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            let ids = ["target_btn", "target_cb", "target_edit", "target_rb1", "target_rb2"];
            for id in &ids {
                if let Some(v) = ui.get_view(id) {
                    v.borrow_mut().set_visibility(Visibility::Visible);
                }
            }
            if let Some(label) = ui.get_view("status_label") {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text("Target views are Visible again");
                }
            }
            ui.relayout();
            true
        }));
    }

    // Target button click — proves it only fires when enabled & visible
    if let Some(btn) = ui.get_view("target_btn") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            if let Some(label) = ui.get_view("status_label") {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text("Target Button was clicked!");
                }
            }
            true
        }));
    }

    // Disabled button click — should never fire
    if let Some(btn) = ui.get_view("disabled_btn") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|_ui, _view, _data| {
            println!("BUG: disabled button was clicked!");
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
