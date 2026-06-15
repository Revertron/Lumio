#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

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
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    if let Some(b) = ui.get_view("btn_toggle") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _, _data| {
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
        e.borrow_mut().on_event(EventType::RightIconClick, Box::new(|_ui, view, _data| {
            if let Some(edit) = view.as_any().downcast_ref::<Edit>() {
                edit.set_text("");
            }
            true
        }));
    }

    if let Some(l) = ui.get_view("link1") {
        l.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            if let Some(status) = ui.get_view("link_status") {
                if let Some(label) = status.borrow_mut().downcast_mut::<Label>() {
                    label.set_text("Link clicked!");
                }
            }
            true
        }));
    }

    // Left-icon click on the read-only field copies its content to clipboard.
    if let Some(e) = ui.get_view("edit_ro") {
        e.borrow_mut().on_event(EventType::LeftIconClick, Box::new(|_ui, view, _data| {
            if let Some(edit) = view.as_any().downcast_ref::<Edit>() {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(edit.get_text());
                    println!("Copied to clipboard: {}", edit.get_text());
                }
            }
            true
        }));
    }

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
