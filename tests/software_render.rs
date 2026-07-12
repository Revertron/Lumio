//! Headless software-render smoke test: lay out a small UI, render it with the
//! tiny-skia + fontdue backend, and assert that something actually drew (the
//! pixmap is not a single uniform color). Only compiled under `backend-software`.
#![cfg(feature = "backend-software")]

use include_dir::{Dir, include_dir};

use lumio::drawing::{DrawableRegistry, Palette, set_current_palette};
use lumio::prelude::*;
use lumio::render::render_to_pixmap;

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
<Frame id="root" width="max" height="max" direction="vertical" padding="10" font="Noto Sans">
    <Label id="l" text="Hello software render"/>
    <Button id="b" text="OK"/>
</Frame>
"#;

/// Dual-backend build only: after `set_render_backend(Software)`, fonts load
/// through the software backend and text actually renders headless. Compares
/// against an empty-text render — if the text were shaped by the GL backend,
/// `SoftwareTheme::draw_text` would skip it and both renders would be identical.
#[cfg(feature = "backend-gl")]
#[test]
fn dual_build_software_text_renders() {
    lumio::backend::set_render_backend(RenderBackend::Software);
    assert_eq!(active_backend(), RenderBackend::Software);

    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let (w, h) = (300u32, 100u32);
    let render = |text: &str| {
        let xml = format!(
            r#"<Frame width="max" height="max" padding="10" font="Noto Sans"><Label text="{text}"/></Frame>"#
        );
        let mut ui = UI::from_xml(&xml, w, h, default_typeface(), 1.0).unwrap();
        ui.layout(w, h, 1.0);
        render_to_pixmap(&ui, w, h, 1.0, &palette, &registry).expect("pixmap").take()
    };
    let with_text = render("Software text");
    let without_text = render("");
    assert_ne!(with_text, without_text, "text did not render in the dual-backend build");
}

#[test]
fn renders_non_blank() {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let (w, h) = (300u32, 200u32);
    let mut ui = UI::from_xml(LAYOUT, w, h, default_typeface(), 1.0).unwrap();
    ui.layout(w, h, 1.0);

    let pixmap = render_to_pixmap(&ui, w, h, 1.0, &palette, &registry).expect("pixmap");
    // The background is one solid color; widgets + text must introduce at least
    // one differing pixel, otherwise nothing was drawn.
    let data = pixmap.data();
    let first = [data[0], data[1], data[2], data[3]];
    let drew_something = data.chunks_exact(4).any(|px| px != first);
    assert!(drew_something, "rendered pixmap is uniformly blank — nothing drew");
}
