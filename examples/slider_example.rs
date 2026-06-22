#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 660;
const HEIGHT: u32 = 1050;
const TITLE: &str = "Slider Example";

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
        self.dir.get_file(path).map(|file| file.contents())
    }
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("slider_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    // Live value readout: update a Label whenever the volume slider changes.
    if let Some(slider) = ui.get_view("vol") {
        slider.borrow_mut().on_event(EventType::ValueChanged, Box::new(|ui, _view, data| {
            if let EventData::Value(v) = data {
                if let Some(lbl) = ui.get_view("vol_value") {
                    if let Some(l) = lbl.borrow_mut().downcast_mut::<Label>() {
                        l.set_text(&format!("value: {}", *v as i32));
                    }
                }
            }
            true
        }));
    }

    // Dark-mode toggle to check both palettes.
    if let Some(check) = ui.get_view("dark_mode") {
        check.borrow_mut().on_event(EventType::CheckedChanged, Box::new(|ui, view, _data| {
            let on = view.get_state().map(|s| s.checked).unwrap_or(false);
            ui.set_palette(if on { Palette::dark() } else { Palette::classic() });
            true
        }));
    }

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
