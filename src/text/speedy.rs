//! speedy2d text backend. Shapes text with `FontFamily::layout_text` and copies
//! the resulting geometry into the backend-neutral [`TextBlock`], keeping the
//! original `FormattedTextBlock` as the draw payload for `Classic`.

use speedy2d::font::{TextAlignment as SpAlignment, TextLayout, TextOptions as SpOptions};

use super::{FontHandle, Glyph, TextAlignment, TextBlock, TextLine, TextOptions, TextShaper};

/// The compile-time-selected shaper instance used by [`FontHandle::layout_text`].
pub(crate) const SHAPER: SpeedyShaper = SpeedyShaper;

pub struct SpeedyShaper;

impl TextShaper for SpeedyShaper {
    fn shape(&self, font: &FontHandle, text: &str, size_px: f32, options: &TextOptions) -> TextBlock {
        let mut sp = SpOptions::new();
        if let Some(width) = options.wrap_width {
            let align = match options.align {
                TextAlignment::Left => SpAlignment::Left,
                TextAlignment::Center => SpAlignment::Center,
                TextAlignment::Right => SpAlignment::Right,
            };
            sp = sp.with_wrap_to_width(width, align);
        }
        sp = sp.with_trim_each_line(options.trim_each_line);

        let block = font.inner.layout_text(text, size_px, sp);

        let lines = block
            .iter_lines()
            .map(|line| TextLine {
                ascent: line.ascent(),
                descent: line.descent(),
                glyphs: line
                    .iter_glyphs()
                    .map(|g| Glyph { position_x: g.position_x(), advance_width: g.advance_width() })
                    .collect(),
            })
            .collect();

        TextBlock { width: block.width(), height: block.height(), lines, payload: block }
    }
}
