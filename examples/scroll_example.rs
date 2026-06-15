#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

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
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    if let Some(button) = ui.get_view("scroll_top") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            if let Some(sv) = ui.get_view("scroll_v") {
                if let Some(scroll) = sv.borrow().downcast_ref::<ScrollView>() {
                    scroll.scroll_to_start();
                }
            }
            true
        }));
    }

    if let Some(button) = ui.get_view("scroll_bottom") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            if let Some(sv) = ui.get_view("scroll_v") {
                if let Some(scroll) = sv.borrow().downcast_ref::<ScrollView>() {
                    scroll.scroll_to_end();
                }
            }
            true
        }));
    }

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
