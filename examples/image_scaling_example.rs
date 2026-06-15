//! Image-scaling demo: draws one space wallpaper (a raster JPEG) at several
//! `ImageView` sizes, plus a window-filling one that rescales live as you resize
//! the window. Exercises the per-backend image scaling path — useful for checking
//! the software backend scales raster images to the destination rect (it now
//! matches the GL backend, which scales on the GPU).
//!
//! Backend-neutral (`lumio::run`); runs on either backend:
//!   cargo run --example image_scaling_example
//!   cargo run --example image_scaling_example --no-default-features --features backend-software

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 900;
const HEIGHT: u32 = 760;
const TITLE: &str = "Image Scaling Demo";

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

    let layout = include_str!("image_scaling_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
