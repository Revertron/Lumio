#![windows_subsystem = "windows"]

//! Demo of Android-style 9-patch (`.9.png`) backgrounds: a single patch via
//! `background="panel.9.png"` and a per-state selector via
//! `background="fancy_button.xml"`. Patch padding is used unless the view sets
//! an explicit `padding` attribute.

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 900;
const HEIGHT: u32 = 420;
const TITLE: &str = "9-Patch Demo";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));

    let layout = include_str!("ninepatch_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
