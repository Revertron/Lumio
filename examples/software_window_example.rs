//! A live software-rendered window (winit + tiny-skia + fontdue, no GL). Run with:
//!
//!   cargo run --example software_window_example --no-default-features --features backend-software
//!
//! The "Open dialog" button opens a modal child window, exercising the
//! software backend's multi-window + app-modal support.

use include_dir::{Dir, include_dir};

use lumio::drawing::{Palette, set_current_palette};
use lumio::prelude::*;

const WIDTH: u32 = 520;
const HEIGHT: u32 = 360;

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" padding="16" font="Noto Sans">
    <Label text="Lumio software backend (winit + tiny-skia + fontdue)" font_size="20"/>
    <Edit id="edit" text="Type here, select, copy/paste…" width="320"/>
    <CheckBox text="A checkbox" checked="true"/>
    <Button id="dlg" text="Open dialog" width="160"/>
</Frame>
"#;

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));
    set_current_palette(Palette::classic());

    let ui = UI::from_xml(LAYOUT, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();
    if let Some(btn) = ui.get_view("dlg") {
        btn.borrow_mut().on_event(
            EventType::Click,
            Box::new(|ui, _view, _data| {
                ui.show_message("Hello", "This modal dialog is a second window, rendered on the software backend.");
                true
            }),
        );
    }

    lumio::run(ui, WindowConfig::new("Lumio — software backend", WIDTH, HEIGHT).center());
}
