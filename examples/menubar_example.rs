#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 480;
const TITLE: &str = "Lumio — MenuBar demo";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

fn menu_clicked(ui: &mut UI, view: &dyn View, _data: &EventData) -> bool {
    let clicked = view.as_any().downcast_ref::<MenuBar>()
        .and_then(|bar| bar.clicked_item());
    if let Some(id) = clicked {
        if let Some(status) = ui.get_view("status") {
            if let Some(label) = status.borrow_mut().downcast_mut::<Label>() {
                label.set_text(&format!("Clicked: {}", id));
            }
        }
    }
    true
}

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));

    let layout = include_str!("menubar_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    if let Some(bar) = ui.get_view("menubar") {
        bar.borrow_mut().on_event(EventType::Click, Box::new(menu_clicked));
    }

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
