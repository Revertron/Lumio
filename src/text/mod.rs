//! Backend-neutral text layer.
//!
//! Views and the [`Theme`](crate::themes::Theme) trait depend only on the types
//! here; the actual shaping and measurement is delegated to a backend selected
//! at compile time (currently speedy2d, behind the `text-speedy2d` feature).
//!
//! The geometry accessors deliberately mirror speedy2d's
//! `FormattedTextBlock` / `FormattedTextLine` / `FormattedGlyph` method names
//! (`width`, `height`, `iter_lines`, `iter_glyphs`, `ascent`, `descent`,
//! `position_x`, `advance_width`) so view-side layout, caret and selection code
//! reads shaped text the same way regardless of the backend.

// --- speedy2d text backend (pulled in by `backend-gl`) ---
#[cfg(feature = "text-speedy2d")]
mod speedy;

#[cfg(feature = "text-speedy2d")]
use speedy::SHAPER as ACTIVE_SHAPER;

/// The backend-specific draw payload carried by every [`TextBlock`]. Only the
/// matching [`Theme`](crate::themes::Theme) backend reads it (via
/// [`TextBlock::payload`]).
#[cfg(feature = "text-speedy2d")]
pub(crate) type BackendBlock = speedy2d::font::FormattedTextBlock;

/// The backend-specific resolved font wrapped by [`FontHandle`].
#[cfg(feature = "text-speedy2d")]
type BackendFont = speedy2d::font::FontFamily;

// --- fontdue software text backend (pulled in by `backend-software`) ---
#[cfg(feature = "text-software")]
mod software;

#[cfg(feature = "text-software")]
use software::SHAPER as ACTIVE_SHAPER;

#[cfg(feature = "text-software")]
pub(crate) type BackendBlock = software::SwBlock;

#[cfg(feature = "text-software")]
type BackendFont = software::SwFont;

#[cfg(not(any(feature = "text-speedy2d", feature = "text-software")))]
compile_error!(
    "Enable exactly one rendering backend feature: `backend-gl` (default) or `backend-software`."
);

#[cfg(all(feature = "text-speedy2d", feature = "text-software"))]
compile_error!(
    "Enable only one rendering backend: `backend-gl` and `backend-software` are mutually exclusive."
);

/// A single laid-out glyph. `position_x` is the glyph's x offset within its
/// line; `advance_width` is how far the pen advances past it. Mirrors
/// speedy2d's `FormattedGlyph`.
#[derive(Clone, Copy)]
pub struct Glyph {
    position_x: f32,
    advance_width: f32,
}

impl Glyph {
    pub fn position_x(&self) -> f32 {
        self.position_x
    }
    pub fn advance_width(&self) -> f32 {
        self.advance_width
    }
}

/// One visual line of a [`TextBlock`]. Mirrors speedy2d's `FormattedTextLine`.
#[derive(Clone)]
pub struct TextLine {
    ascent: f32,
    descent: f32,
    glyphs: Vec<Glyph>,
}

impl TextLine {
    pub fn iter_glyphs(&self) -> impl Iterator<Item = &Glyph> {
        self.glyphs.iter()
    }
    pub fn ascent(&self) -> f32 {
        self.ascent
    }
    /// Negative, matching speedy2d's convention.
    pub fn descent(&self) -> f32 {
        self.descent
    }
}

/// A laid-out block of text: backend-neutral geometry (consumed by views for
/// sizing, caret placement and selection) plus an opaque backend payload used
/// only by the active theme to draw it. Mirrors speedy2d's `FormattedTextBlock`.
#[derive(Clone)]
pub struct TextBlock {
    width: f32,
    height: f32,
    lines: Vec<TextLine>,
    payload: BackendBlock,
}

impl TextBlock {
    pub fn width(&self) -> f32 {
        self.width
    }
    pub fn height(&self) -> f32 {
        self.height
    }
    pub fn iter_lines(&self) -> impl Iterator<Item = &TextLine> {
        self.lines.iter()
    }
    /// The backend draw payload. Only the matching theme backend reads this.
    pub(crate) fn payload(&self) -> &BackendBlock {
        &self.payload
    }
}

/// Horizontal alignment within a wrapped block. Mirrors speedy2d's
/// `TextAlignment`.
#[derive(Clone, Copy)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Layout options for shaping. Mirrors the subset of speedy2d's `TextOptions`
/// the codebase actually uses (`with_wrap_to_width`, `with_trim_each_line`).
#[derive(Clone)]
pub struct TextOptions {
    wrap_width: Option<f32>,
    align: TextAlignment,
    trim_each_line: bool,
}

impl TextOptions {
    pub fn new() -> Self {
        // Matches speedy2d's default: trim leading whitespace per line.
        TextOptions { wrap_width: None, align: TextAlignment::Left, trim_each_line: true }
    }

    pub fn with_wrap_to_width(mut self, width: f32, align: TextAlignment) -> Self {
        self.wrap_width = Some(width);
        self.align = align;
        self
    }

    pub fn with_trim_each_line(mut self, trim: bool) -> Self {
        self.trim_each_line = trim;
        self
    }
}

impl Default for TextOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Backend-neutral handle to a resolved font family. Returned by
/// [`crate::assets::get_font_family`] and shaped via [`FontHandle::layout_text`].
#[derive(Clone)]
pub struct FontHandle {
    inner: BackendFont,
}

impl FontHandle {
    /// Wrap a backend font family. Called by the font loader in `assets`.
    pub(crate) fn new(inner: BackendFont) -> Self {
        FontHandle { inner }
    }

    /// Lay out `text` at `size_px` (already scaled to physical pixels) with the
    /// given options. Same name/signature as speedy2d's
    /// `FontFamily::layout_text`, so existing call sites are unchanged.
    pub fn layout_text(&self, text: &str, size_px: f32, options: TextOptions) -> TextBlock {
        ACTIVE_SHAPER.shape(self, text, size_px, &options)
    }
}

/// Contract implemented by each text backend: turn a font + string into a
/// laid-out [`TextBlock`]. Exactly one implementation is selected at compile
/// time (see the `text-speedy2d` feature).
pub trait TextShaper {
    fn shape(&self, font: &FontHandle, text: &str, size_px: f32, options: &TextOptions) -> TextBlock;
}
