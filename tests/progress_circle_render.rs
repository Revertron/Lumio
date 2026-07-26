//! Headless render checks for `ProgressCircle`: the determinate ring actually
//! paints its fill arc, the arc grows with the value, a set value eases in over
//! successive ticks, and the indeterminate ring animates. Only compiled under
//! `backend-software`.
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

const SIZE: u32 = 80;

fn setup() -> Palette {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    palette
}

fn circle_ui(attrs: &str) -> UI {
    let xml = format!(
        r#"<Frame width="max" height="max" font="Noto Sans"><ProgressCircle id="c" width="80" height="80" thickness="8" {attrs}/></Frame>"#
    );
    let mut ui = UI::from_xml(&xml, SIZE, SIZE, default_typeface(), 1.0).unwrap();
    ui.layout(SIZE, SIZE, 1.0);
    ui
}

/// Pixels painted in the palette's `progress_fill` colour — i.e. the size of
/// the drawn arc (or of the dots, in indeterminate mode).
fn fill_pixels(ui: &UI, palette: &Palette) -> usize {
    let registry = DrawableRegistry::new();
    let pixmap = render_to_pixmap(ui, SIZE, SIZE, 1.0, palette, &registry).expect("pixmap");
    let fill = palette.color("progress_fill");
    let (r, g, b) = (((fill >> 16) & 0xff) as u8, ((fill >> 8) & 0xff) as u8, (fill & 0xff) as u8);
    // Fully opaque pixels are premultiplied by 1.0, so the channels compare directly.
    pixmap
        .pixels()
        .iter()
        .filter(|p| p.alpha() == 255 && p.red() == r && p.green() == g && p.blue() == b)
        .count()
}

#[test]
fn determinate_arc_grows_with_value() {
    let palette = setup();
    let empty = fill_pixels(&circle_ui(r#"value="0.0""#), &palette);
    let quarter = fill_pixels(&circle_ui(r#"value="0.25""#), &palette);
    let full = fill_pixels(&circle_ui(r#"value="1.0""#), &palette);

    assert_eq!(empty, 0, "a zero-valued circle must not draw a fill arc");
    assert!(quarter > 0, "a quarter-valued circle drew no fill arc");
    assert!(
        full > quarter * 2,
        "the full ring ({full} px) should dwarf a quarter of it ({quarter} px)"
    );
}

#[test]
fn value_eases_toward_the_target() {
    let palette = setup();
    let mut ui = circle_ui(r#"value="0.0""#);
    assert_eq!(fill_pixels(&ui, &palette), 0);

    let element = ui.get_view("c").expect("circle");
    element.borrow().downcast_ref::<ProgressCircle>().expect("type").set_value(1.0);
    // The target is set immediately; only the drawn arc lags behind.
    assert_eq!(element.borrow().downcast_ref::<ProgressCircle>().unwrap().get_value(), 1.0);

    std::thread::sleep(std::time::Duration::from_millis(40));
    ui.update();
    let partway = fill_pixels(&ui, &palette);
    assert!(partway > 0, "the arc did not start growing");

    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        ui.update();
    }
    let settled = fill_pixels(&ui, &palette);
    assert!(settled > partway, "the arc stopped short: {partway} px → {settled} px");
}

#[test]
fn indeterminate_draws_dots_and_animates() {
    let palette = setup();
    let registry = DrawableRegistry::new();
    let ui = circle_ui(r#"indeterminate="true""#);
    let render = || render_to_pixmap(&ui, SIZE, SIZE, 1.0, &palette, &registry).expect("pixmap").take();

    let first = render();
    let background = [first[0], first[1], first[2], first[3]];
    assert!(
        first.chunks_exact(4).any(|px| px != background),
        "the indeterminate circle drew nothing"
    );

    // A quarter of a revolution later, the dots must have moved.
    std::thread::sleep(std::time::Duration::from_millis(350));
    assert_ne!(first, render(), "the indeterminate circle did not animate");
}
