//! Headless software rendering demo: lays out a small UI and renders it to a
//! PNG with the tiny-skia + fontdue backend — no window. Run with:
//!
//!   cargo run --example headless_render --no-default-features --features backend-software
//!
//! It writes `headless_out.png` in the working directory; open it to eyeball the
//! software renderer's output.

use include_dir::{Dir, include_dir};

use lumio::drawing::{DrawableRegistry, Palette, set_current_palette};
use lumio::prelude::*;
use lumio::render::render_to_pixmap;

const WIDTH: u32 = 480;
const HEIGHT: u32 = 320;

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
    <Label id="title" text="Headless software render" font_size="22"/>
    <Label id="sub" text="tiny-skia + fontdue backend"/>
    <Edit id="edit" text="Editable field" width="240"/>
    <Button id="btn" text="A Button" width="160"/>
    <CheckBox id="chk" text="A checkbox" checked="true"/>
</Frame>
"#;

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let mut ui = UI::from_xml(LAYOUT, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();
    ui.layout(WIDTH, HEIGHT, 1.0);

    let pixmap = render_to_pixmap(&ui, WIDTH, HEIGHT, 1.0, &palette, &registry)
        .expect("failed to allocate pixmap");
    pixmap.save_png("headless_out.png").expect("failed to write PNG");
    println!("Wrote headless_out.png ({}x{})", WIDTH, HEIGHT);
}
