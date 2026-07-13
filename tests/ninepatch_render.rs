//! Headless end-to-end tests for 9-patch backgrounds: XML `background`
//! attribute → view_base → CPU composite → software blit. Only compiled under
//! `backend-software`.
#![cfg(feature = "backend-software")]

use include_dir::{Dir, include_dir};
use image::{Rgba, RgbaImage};

use lumio::drawing::{DrawableRegistry, Palette, set_current_palette};
use lumio::prelude::*;
use lumio::render::render_to_pixmap;

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct DirProvider {
    dir: Dir<'static>,
}

impl AssetsProvider for DirProvider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

/// Serves PNGs built in memory by the test, for exact-color assertions.
struct MemProvider {
    files: Vec<(String, Vec<u8>)>,
}

impl AssetsProvider for MemProvider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.files.iter().find(|(p, _)| p == path).map(|(_, b)| b.as_slice())
    }
}

fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    bytes
}

/// A 5x5 `.9.png` (3x3 content): distinct opaque corner colors, red center,
/// middle row/column stretchable. No padding markers.
fn test_patch() -> RgbaImage {
    let mut img = RgbaImage::new(5, 5);
    for y in 0..3 {
        for x in 0..3 {
            let px = match (x, y) {
                (0, 0) => Rgba([255, 0, 0, 255]),
                (2, 0) => Rgba([0, 255, 0, 255]),
                (0, 2) => Rgba([0, 0, 255, 255]),
                (2, 2) => Rgba([255, 255, 0, 255]),
                _ => Rgba([200, 100, 50, 255]),
            };
            img.put_pixel(x + 1, y + 1, px);
        }
    }
    img.put_pixel(2, 0, Rgba([0, 0, 0, 255])); // stretchable middle column
    img.put_pixel(0, 2, Rgba([0, 0, 0, 255])); // stretchable middle row
    img
}

fn pixel(pixmap: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * pixmap.width() + x) * 4) as usize;
    let d = pixmap.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

/// Corners of the patch must land untouched in the pixmap corners while the
/// middle stretches — at several destination sizes and scales. Cells are
/// composited independently, so even scaled fixed cells stay exact-color.
#[test]
fn stretches_with_fixed_corners() {
    set_provider(Box::new(MemProvider {
        files: vec![("test.9.png".to_owned(), encode_png(&test_patch()))],
    }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let xml = r#"<Frame width="max" height="max" background="test.9.png"></Frame>"#;
    for &(w, h, scale) in &[(60u32, 40u32, 1.0f64), (120, 80, 1.0), (60, 40, 1.5)] {
        let mut ui = UI::from_xml(xml, w, h, default_typeface(), scale).unwrap();
        ui.layout(w, h, scale);
        let pixmap = render_to_pixmap(&ui, w, h, scale, &palette, &registry).expect("pixmap");

        // tiny-skia stores premultiplied RGBA, but all patch pixels are
        // opaque, so values compare exactly.
        assert_eq!(pixel(&pixmap, 0, 0), [255, 0, 0, 255], "{}x{}@{}", w, h, scale);
        assert_eq!(pixel(&pixmap, w - 1, 0), [0, 255, 0, 255], "{}x{}@{}", w, h, scale);
        assert_eq!(pixel(&pixmap, 0, h - 1), [0, 0, 255, 255], "{}x{}@{}", w, h, scale);
        assert_eq!(pixel(&pixmap, w - 1, h - 1), [255, 255, 0, 255], "{}x{}@{}", w, h, scale);
        assert_eq!(pixel(&pixmap, w / 2, h / 2), [200, 100, 50, 255], "{}x{}@{}", w, h, scale);
    }
}

/// The per-state selector XML loads through the `background` attribute and the
/// default item renders (a blue-ish button instead of the classic gray one).
#[test]
fn selector_renders_default_state() {
    set_provider(Box::new(DirProvider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let (w, h) = (200u32, 60u32);
    let render = |xml: &str| {
        let mut ui = UI::from_xml(xml, w, h, default_typeface(), 1.0).unwrap();
        ui.layout(w, h, 1.0);
        render_to_pixmap(&ui, w, h, 1.0, &palette, &registry).expect("pixmap")
    };

    let fancy = render(
        r#"<Frame width="max" height="max" padding="4">
               <Button background="fancy_button.xml" text="" width="max" height="max"/>
           </Frame>"#,
    );
    let classic = render(
        r#"<Frame width="max" height="max" padding="4">
               <Button text="" width="max" height="max"/>
           </Frame>"#,
    );
    assert_ne!(fancy.data(), classic.data(), "9-patch button rendered same as classic");

    // Center of the fancy button must be the blue fill from button.9.png.
    let [r, g, b, _] = pixel(&fancy, w / 2, h / 2);
    assert!(b > r && b > 100, "expected blue-ish button center, got rgb({},{},{})", r, g, b);
}

/// Not an assertion test: renders the demo layout at both palettes and two
/// scales into the directory named by `LUMIO_RENDER_DUMP` for eyeballing.
/// Skipped (trivially passes) when the variable is unset.
#[test]
fn dump_visual_pngs() {
    let Ok(dir) = std::env::var("LUMIO_RENDER_DUMP") else {
        return;
    };
    set_provider(Box::new(DirProvider { dir: ASSETS }));
    let registry = DrawableRegistry::new();
    let xml = r#"
    <Frame width="max" height="max" direction="vertical" padding="12" font="Noto Sans">
        <Frame background="panel.9.png" direction="vertical" width="260" margin="6">
            <Label text="Small panel"/>
            <Label text="(padding from the patch)"/>
        </Frame>
        <Frame background="panel.9.png" direction="vertical" width="max" margin="6">
            <Label text="Wide panel, same patch stretched"/>
        </Frame>
        <Frame direction="horizontal" width="max" height="min">
            <Button background="fancy_button.xml" text="Fancy" margin="6"/>
            <Button background="fancy_button.xml" text="Padded" padding="12" margin="6"/>
            <Button text="Classic" margin="6"/>
        </Frame>
        <Frame direction="horizontal" width="max" height="min">
            <Label background="panel.9.png" text="Label on 9-patch" margin="6"/>
            <Edit background="panel.9.png" width="200" text="Edit on 9-patch" margin="6"/>
        </Frame>
        <Frame direction="horizontal" width="max" height="min">
            <Memo background="panel.9.png" width="200" height="70" text="Memo on 9-patch" margin="6"/>
            <Frame direction="vertical" width="max" weight="1">
                <ComboBox background="panel.9.png" width="180" margin="6"
                          items="ComboBox on 9-patch" selected="0"/>
                <CheckBox background="panel.9.png" text="CheckBox on 9-patch" margin="6"/>
                <ProgressBar background="panel.9.png" width="180" value="0.6" margin="6"/>
            </Frame>
        </Frame>
    </Frame>"#;

    for (palette, pname) in [(Palette::classic(), "classic"), (Palette::dark(), "dark")] {
        set_current_palette(palette.clone());
        for scale in [1.0f64, 1.5] {
            let (w, h) = ((520.0 * scale) as u32, (460.0 * scale) as u32);
            let mut ui = UI::from_xml(xml, w, h, default_typeface(), scale).unwrap();
            ui.layout(w, h, scale);
            let pixmap = render_to_pixmap(&ui, w, h, scale, &palette, &registry).expect("pixmap");
            let path = format!("{}/ninepatch_{}_{}.png", dir, pname, scale);
            pixmap.save_png(&path).expect("save png");
            println!("wrote {}", path);
        }
    }
}
