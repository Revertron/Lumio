//! Headless software-render smoke test: lay out a small UI, render it with the
//! tiny-skia + fontdue backend, and assert that something actually drew (the
//! pixmap is not a single uniform color). Only compiled under `backend-software`.
#![cfg(feature = "backend-software")]

use include_dir::{Dir, include_dir};

use lumio::drawing::{DrawableRegistry, Palette, set_current_palette};
use lumio::prelude::*;
use lumio::views::TermGrid;
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
/// `RendererSoftware::draw_text` would skip it and both renders would be identical.
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

/// Every widget must survive a paint. `TermGrid` did not: it asked the palette
/// for `edit.back`, which is a drawable role rather than a colour token, and the
/// unknown-token `debug_assert` in `Palette::color` turned the first paint of a
/// terminal into a crash.
#[test]
fn termgrid_paints_without_panicking() {
    const TERM_LAYOUT: &str = r#"
    <Frame id="root" width="max" height="max" direction="vertical">
        <TermGrid id="term" width="max" height="max" cols="20" rows="5"/>
    </Frame>
    "#;

    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let mut ui = UI::from_xml(TERM_LAYOUT, 320, 160, default_typeface(), 1.0).unwrap();
    ui.layout(320, 160, 1.0);
    render_to_pixmap(&ui, 320, 160, 1.0, &palette, &registry).expect("failed to allocate pixmap");

    // A character cell is a LINE tall, not an em. `TextBlock::height()` is
    // normalised to the em size, and rows spaced by that clip the tails of
    // g, j and p into the row below.
    let element = ui.get_view("term").unwrap();
    let view = element.borrow();
    let grid = view.as_any().downcast_ref::<TermGrid>().unwrap();
    let (cell_w, cell_h) = grid.cell_size();
    assert!(cell_w > 0.0, "no cell metrics: is a monospace font installed?");
    assert!(
        cell_h > 14.0,
        "cell height {cell_h} is not taller than the 14dip em size, so descenders are clipped"
    );

    // A cell must be exactly one glyph ADVANCE wide, or drawn text creeps out of
    // its own grid: a run of N characters has to span N cells. When it did not,
    // the cursor block fell further behind the text with every column.
    let font = lumio::assets::get_font_family(default_mono_font_name(), FontStyle::Regular)
        .expect("no monospace font");
    let run = font.layout_text(&"W".repeat(20), 14.0, lumio::text::TextOptions::new());
    let drift = (run.width() - cell_w * 20.0).abs();
    assert!(
        drift < 1.0,
        "20 cells span {} but the text spans {}: text drifts {drift}px out of the grid",
        cell_w * 20.0,
        run.width()
    );
}

/// Painting must use the very typeface the metrics were measured with. The view
/// inherits its font size from whatever typeface it is resolved against, and
/// layout resolves against the parent in the view tree while painting used to
/// resolve against the theme's default. When those sizes differ the drawn text
/// advances by more than a cell and creeps out of the grid — the text ends up
/// right of its own cells, so a cursor block drawn at the right cell looks like
/// it is lagging behind the text.
#[test]
fn drawn_text_stays_inside_its_cells() {
    const TERM_LAYOUT: &str = r#"
    <Frame id="root" width="max" height="max" direction="vertical">
        <TermGrid id="term" width="max" height="max"/>
    </Frame>
    "#;
    const COLS: usize = 40;
    const ROWS: usize = 3;
    const WIDTH: u32 = 600;
    const HEIGHT: u32 = 120;

    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let mut ui = UI::from_xml(TERM_LAYOUT, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();
    ui.layout(WIDTH, HEIGHT, 1.0);

    let element = ui.get_view("term").unwrap();
    let cell_w = {
        let view = element.borrow();
        let grid = view.as_any().downcast_ref::<TermGrid>().unwrap();

        // A full row of white 'W' on black, then blanks.
        let mut data = Vec::new();
        data.extend_from_slice(&(COLS as u32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(ROWS as u32).to_le_bytes());
        for row in 0..ROWS {
            for _ in 0..COLS {
                let ch = if row == 0 { 'W' } else { ' ' };
                data.extend_from_slice(&(ch as u32).to_le_bytes());
                data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
                data.extend_from_slice(&0xFF000000u32.to_le_bytes());
                data.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        assert!(grid.apply_cells(&data), "malformed payload");
        grid.cell_size().0
    };

    let pixmap = render_to_pixmap(&ui, WIDTH, HEIGHT, 1.0, &palette, &registry).unwrap();
    let last_ink = (0..WIDTH as usize)
        .filter(|x| {
            (0..HEIGHT as usize).any(|y| {
                let p = pixmap.pixels()[y * WIDTH as usize + x];
                p.red() > 40 || p.green() > 40 || p.blue() > 40
            })
        })
        .next_back()
        .expect("nothing was drawn");

    let grid_right = (cell_w * COLS as f32).ceil() as usize;
    assert!(
        last_ink <= grid_right,
        "text reaches x {last_ink}, past the {COLS} cells that end at {grid_right}"
    );
}

/// Glyphs must sit in the middle of the cell the background fills, not hug its
/// top edge. The tolerance is loose on purpose: the two text backends disagree
/// about what `TextLine::ascent`/`descent` mean — speedy2d reports the font's
/// metrics, fontdue the extents of the glyphs actually laid out — so exact
/// balance differs between them. What must hold either way is that the glyph is
/// nowhere near an edge. Cells are at least 1.2em tall, so a shaped line usually has leading
/// to spare; when all of it was left below the glyphs, an inverse row (top's
/// header, or the cursor block) looked shifted down relative to its own text.
#[test]
fn glyphs_are_centred_in_their_cells() {
    const TERM_LAYOUT: &str = r#"
    <Frame id="root" width="max" height="max" direction="vertical">
        <TermGrid id="term" width="max" height="max"/>
    </Frame>
    "#;
    const COLS: usize = 20;
    const ROWS: usize = 3;
    const WIDTH: u32 = 300;
    const HEIGHT: u32 = 120;

    set_provider(Box::new(Provider { dir: ASSETS }));
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let mut ui = UI::from_xml(TERM_LAYOUT, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();
    ui.layout(WIDTH, HEIGHT, 1.0);

    let element = ui.get_view("term").unwrap();
    let cell_h = {
        let view = element.borrow();
        let grid = view.as_any().downcast_ref::<TermGrid>().unwrap();
        let mut data = Vec::new();
        data.extend_from_slice(&(COLS as u32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(ROWS as u32).to_le_bytes());
        for row in 0..ROWS {
            for _ in 0..COLS {
                // Row 1 is a white band with black text, like top's header.
                let (ch, fg, bg) = if row == 1 {
                    ('E', 0xFF000000u32, 0xFFFFFFFFu32)
                } else {
                    (' ', 0xFFFFFFFFu32, 0xFF000000u32)
                };
                data.extend_from_slice(&(ch as u32).to_le_bytes());
                data.extend_from_slice(&fg.to_le_bytes());
                data.extend_from_slice(&bg.to_le_bytes());
                data.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        assert!(grid.apply_cells(&data));
        grid.cell_size().1
    };

    let pixmap = render_to_pixmap(&ui, WIDTH, HEIGHT, 1.0, &palette, &registry).unwrap();
    let px = pixmap.pixels();
    let at = |x: usize, y: usize| px[y * WIDTH as usize + x];
    // The white band: rows where the far-right column (no glyph there) is white.
    let band: Vec<usize> = (0..HEIGHT as usize)
        .filter(|y| at(COLS * 7 - 2, *y).red() > 200)
        .collect();
    // The black glyph ink inside the band, sampled in the first cell.
    let ink: Vec<usize> = (0..HEIGHT as usize)
        .filter(|y| (0..6).any(|x| at(x + 1, *y).red() < 60) && band.contains(y))
        .collect();
    let (band_top, band_bottom) = (*band.first().unwrap(), *band.last().unwrap());
    let (ink_top, ink_bottom) = (*ink.first().unwrap(), *ink.last().unwrap());
    let above = ink_top - band_top;
    let below = band_bottom - ink_bottom;
    assert!(
        above.abs_diff(below) as f32 <= cell_h / 4.0,
        "glyph sits {above}px below the cell top and {below}px above its bottom          (cell {cell_h}px): it is not centred in the filled cell"
    );
}
