//! fontdue software text backend. Shapes + lays out text with `fontdue::layout`
//! and stores positioned glyphs in the payload for `SoftwareTheme::draw_text` to
//! rasterize.
//!
//! v1 limitation: fontdue does not auto-fall-back across a font chain for missing
//! glyphs, so only the primary font is used here; the resolved fallbacks are
//! carried but unused (per-run fallback is a future enhancement). `trim_each_line`
//! is likewise not honored (fontdue includes whitespace glyphs).

use std::rc::Rc;

use fontdue::Font;
use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};

use super::{FontHandle, Glyph, TextBlock, TextLine, TextOptions, TextShaper};

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

pub(crate) const SHAPER: SoftwareShaper = SoftwareShaper;

pub struct SoftwareShaper;

impl TextShaper for SoftwareShaper {
    fn shape(&self, font: &FontHandle, text: &str, size_px: f32, options: &TextOptions) -> TextBlock {
        let fonts: &SwFont = &font.inner;

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

        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings {
            max_width: options.wrap_width,
            // Left align / Word wrap / hard line breaks are the fontdue defaults.
            ..LayoutSettings::default()
        });
        layout.append(fonts.as_slice(), &TextStyle::new(text, render_px, 0));

        let positions = layout.glyphs();
        let mut placed: Vec<PlacedGlyph> = Vec::new();
        let mut lines: Vec<TextLine> = Vec::new();
        let mut block_w = 0f32;

        if let Some(line_positions) = layout.lines() {
            for lp in line_positions {
                // `glyph_end` is inclusive; an empty line has glyph_start > glyph_end.
                let mut neutral: Vec<Glyph> = Vec::new();
                if lp.glyph_start < positions.len() && lp.glyph_start <= lp.glyph_end {
                    let line_x0 = positions[lp.glyph_start].x;
                    for gp in &positions[lp.glyph_start..=lp.glyph_end] {
                        let advance = fonts[gp.font_index]
                            .metrics_indexed(gp.key.glyph_index, gp.key.px)
                            .advance_width;
                        neutral.push(Glyph { position_x: gp.x - line_x0, advance_width: advance });
                        block_w = block_w.max(gp.x + advance);
                        // Whitespace glyphs have a zero-size bitmap; skip rasterizing them.
                        if gp.width > 0 && gp.height > 0 {
                            placed.push(PlacedGlyph {
                                font_index: gp.font_index,
                                font_hash: gp.key.font_hash,
                                glyph_index: gp.key.glyph_index,
                                px: gp.key.px,
                                x: gp.x,
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
            payload: SwBlock { fonts: Rc::clone(fonts), glyphs: placed },
        }
    }
}
