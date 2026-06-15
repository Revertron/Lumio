#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 720;
const TITLE: &str = "Lumio — Label polish demo";

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

    let layout = include_str!("label_polish_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    // chip1, chip2 — proper removal via the deferred queue. The chip is
    // dropped from the parent Frame at the next `update()` tick.
    for id in ["chip1", "chip2"] {
        if let Some(el) = ui.get_view(id) {
            let id_owned = id.to_string();
            el.borrow_mut().on_event(EventType::RightIconClick, Box::new(move |ui, _, _data| {
                ui.remove_view(&id_owned);
                if let Some(status) = ui.get_view("status") {
                    if let Some(s) = status.borrow_mut().downcast_mut::<Label>() {
                        s.set_text(&format!("Removed chip: {}", id_owned));
                    }
                }
                true
            }));
        }
    }
    // chip3 — hide via Visibility::Gone (stays in the tree, just collapsed).
    // Useful when you want to bring it back later via `show()`.
    if let Some(el) = ui.get_view("chip3") {
        el.borrow_mut().on_event(EventType::RightIconClick, Box::new(|ui, view, _data| {
            if let Some(label) = view.as_any().downcast_ref::<Label>() {
                label.hide();
            }
            if let Some(status) = ui.get_view("status") {
                if let Some(s) = status.borrow_mut().downcast_mut::<Label>() {
                    s.set_text("Hid chip3 (still in tree — try ui.get_view)");
                }
            }
            true
        }));
    }

    if let Some(el) = ui.get_view("tag_locked") {
        el.borrow_mut().on_event(EventType::LeftIconClick, Box::new(|ui, _, _data| {
            if let Some(status) = ui.get_view("status") {
                if let Some(label) = status.borrow_mut().downcast_mut::<Label>() {
                    label.set_text("Lock icon clicked");
                }
            }
            true
        }));
    }

    if let Some(el) = ui.get_view("link_red") {
        el.borrow_mut().on_event(EventType::Click, Box::new(|ui, _, _data| {
            if let Some(status) = ui.get_view("status") {
                if let Some(label) = status.borrow_mut().downcast_mut::<Label>() {
                    label.set_text("Red link clicked");
                }
            }
            true
        }));
    }

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
