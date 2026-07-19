//! fontdue software text backend. Shapes + lays out text with `fontdue::layout`
//! and stores positioned glyphs in the payload for `RendererSoftware::draw_text` to
//! rasterize.
//!
//! Parity with the GL backend: honors `TextOptions::align` (via fontdue's
//! `horizontal_align`), `trim_each_line` (leading whitespace per wrapped line is
//! dropped), and per-glyph **font fallback** — fontdue has no automatic fallback
//! across a font chain, so the text is split into runs by the first font in the
//! chain that has each glyph (see [`append_with_fallback`]).

use std::rc::Rc;

use fontdue::Font;
use fontdue::layout::{CoordinateSystem, HorizontalAlign, Layout, LayoutSettings, TextStyle};

use super::{BackendBlock, Glyph, TextAlignment, TextBlock, TextLine, TextOptions};

/// Resolved fonts for the software backend: primary first, then any fallbacks.
pub type SwFont = Rc<Vec<Font>>;

/// A glyph positioned within a shaped block, ready to rasterize. Coordinates are
/// relative to the block origin, top-left (fontdue `PositiveYDown`).
#[derive(Clone)]
pub struct PlacedGlyph {
    /// Index into the block's own `fonts` list (used to fetch the `Font` to
    /// rasterize). NOT a stable cache key — index 0 is the primary font of every
    /// block regardless of style, so caches must key on `font_hash` instead.
    pub font_index: usize,
    /// fontdue's globally-unique hash of the actual font file. Distinguishes
    /// regular/bold/italic so a shared glyph index doesn't collide in caches.
    pub font_hash: usize,
    pub glyph_index: u16,
    pub px: f32,
    pub x: f32,
    pub y: f32,
}

/// Software draw payload: the fonts plus positioned glyphs. The software theme
/// rasterizes each glyph (`Font::rasterize_indexed`) and blits its coverage.
#[derive(Clone)]
pub struct SwBlock {
    pub fonts: SwFont,
    pub glyphs: Vec<PlacedGlyph>,
}

/// Shape `text` with fontdue. Called by [`super::FontHandle::layout_text`] for
/// fonts loaded by the software backend.
pub(crate) fn shape(fonts: &SwFont, text: &str, size_px: f32, options: &TextOptions) -> TextBlock {
    // `size_px` follows the GL/speedy2d convention where the value is the
    // line height. fontdue's `px` is the em size, which for typical fonts
    // renders ~1.3-1.4x larger. Convert so fontdue's resulting line height
    // equals `size_px` — keeps glyphs the same visual size as the GL backend
    // and makes `TextBlock::height()` ≈ `size_px`, which the layout code
    // assumes. `new_line_size` scales linearly with px, so the ratio at any
    // probe size gives the conversion factor.
    let render_px = fonts
        .first()
        .and_then(|f| f.horizontal_line_metrics(size_px))
        .filter(|m| m.new_line_size > 0.0)
        .map(|m| size_px * size_px / m.new_line_size)
        .unwrap_or(size_px);

    let horizontal_align = match options.align {
        TextAlignment::Left => HorizontalAlign::Left,
        TextAlignment::Center => HorizontalAlign::Center,
        TextAlignment::Right => HorizontalAlign::Right,
    };
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings {
        max_width: options.wrap_width,
        horizontal_align,
        // Word wrap / hard line breaks are the fontdue defaults.
        ..LayoutSettings::default()
    });
    append_with_fallback(&mut layout, fonts.as_slice(), text, render_px);

    let positions = layout.glyphs();
    let mut placed: Vec<PlacedGlyph> = Vec::new();
    let mut lines: Vec<TextLine> = Vec::new();
    let mut block_w = 0f32;

    if let Some(line_positions) = layout.lines() {
        for lp in line_positions {
            // `glyph_end` is inclusive; an empty line has glyph_start > glyph_end.
            let mut neutral: Vec<Glyph> = Vec::new();
            if lp.glyph_start < positions.len() && lp.glyph_start <= lp.glyph_end {
                // Honor `trim_each_line`: drop leading whitespace glyphs and
                // shift the line left by their width so visible content starts
                // at the line origin (matches the GL backend). A no-op for lines
                // with no leading whitespace, so the common case is unchanged.
                let mut start = lp.glyph_start;
                if options.trim_each_line {
                    while start < lp.glyph_end && positions[start].parent.is_whitespace() {
                        start += 1;
                    }
                }
                let lead = positions[start].x - positions[lp.glyph_start].x;
                let line_x0 = positions[start].x;
                for gp in &positions[start..=lp.glyph_end] {
                    let advance = fonts[gp.font_index]
                        .metrics_indexed(gp.key.glyph_index, gp.key.px)
                        .advance_width;
                    neutral.push(Glyph { position_x: gp.x - line_x0, advance_width: advance });
                    block_w = block_w.max(gp.x - lead + advance);
                    // Whitespace glyphs have a zero-size bitmap; skip rasterizing them.
                    if gp.width > 0 && gp.height > 0 {
                        placed.push(PlacedGlyph {
                            font_index: gp.font_index,
                            font_hash: gp.key.font_hash,
                            glyph_index: gp.key.glyph_index,
                            px: gp.key.px,
                            x: gp.x - lead,
                            y: gp.y,
                        });
                    }
                }
            }
            lines.push(TextLine { ascent: lp.max_ascent, descent: lp.min_descent, glyphs: neutral });
        }
    }

    TextBlock {
        width: block_w.ceil(),
        height: layout.height(),
        lines,
        payload: BackendBlock::Soft(SwBlock { fonts: Rc::clone(fonts), glyphs: placed }),
    }
}

/// Append `text` to `layout`, choosing a font per glyph: the first font in the
/// chain that has the glyph, else the primary (index 0, which renders `.notdef`).
/// fontdue lays out one font per appended run with no automatic fallback, so the
/// text is split into maximal same-font runs — this is what gives the software
/// backend the cross-chain fallback the GL backend's `FontFamily` does for free.
fn append_with_fallback(layout: &mut Layout, fonts: &[Font], text: &str, px: f32) {
    if text.is_empty() {
        // Still append once so an empty block has a line height, as before.
        layout.append(fonts, &TextStyle::new("", px, 0));
        return;
    }
    let pick = |c: char| fonts.iter().position(|f| f.lookup_glyph_index(c) != 0).unwrap_or(0);

    let mut run = String::new();
    let mut run_font = 0usize;
    for c in text.chars() {
        let fi = pick(c);
        if !run.is_empty() && fi != run_font {
            layout.append(fonts, &TextStyle::new(&run, px, run_font));
            run.clear();
        }
        if run.is_empty() {
            run_font = fi;
        }
        run.push(c);
    }
    if !run.is_empty() {
        layout.append(fonts, &TextStyle::new(&run, px, run_font));
    }
}
