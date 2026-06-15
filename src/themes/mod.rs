#[cfg(feature = "backend-gl")]
mod classic;
#[cfg(feature = "backend-gl")]
mod utils;
#[cfg(feature = "backend-software")]
mod software;

use super::styles::selector::{DrawState, MainSelector};
use super::drawing::{Drawable, DrawableRegistry, Palette};
use super::text::TextBlock;
#[cfg(feature = "backend-gl")]
pub use self::classic::Classic;
#[cfg(feature = "backend-gl")]
pub use self::classic::ImageCache;
#[cfg(feature = "backend-software")]
pub use self::software::{GlyphCache, SoftwareImageCache, SoftwareTheme};
use super::types::Rect;

/// The default root typeface: the palette's "default" role with its size
/// stripped (so per-role palette sizes win when they cascade into views).
/// Backend-neutral; used at app startup to seed `UI::from_xml`. `Classic` and
/// the software backend both expose this.
pub fn default_typeface() -> Typeface {
    let mut typeface = crate::drawing::current_typeface("default");
    typeface.font_size = None;
    typeface
}

pub trait Theme {
    fn clear_screen(&mut self);

    /// The palette this theme resolves colors, dimensions and typefaces against.
    /// Backs the default `typeface`/`color`/`dimension`/`get_back_color`/
    /// `get_text_color` implementations below, which are identical across
    /// backends.
    fn palette(&self) -> &Palette;

    /// Resolve a typeface role (e.g. "default") against the theme's palette.
    /// Unknown roles fall back to "default".
    fn typeface(&self, role: &str) -> Typeface {
        self.palette().typeface(role)
    }

    fn get_back_color(&self, state: ViewState, selector: Option<&MainSelector>) -> u32 {
        if let Some(selector) = selector {
            if let Some(s) = selector.get_state(&state) {
                match s {
                    DrawState::Transparent => return 0x00000000,
                    DrawState::Color(c) => return *c,
                    DrawState::Token(t) => return self.palette().color(t),
                    _ => {}
                }
            }
        }
        self.palette().color("background")
    }

    fn get_text_color(&self, state: ViewState, selector: Option<&MainSelector>) -> u32 {
        if let Some(selector) = selector {
            if let Some(s) = selector.get_state(&state) {
                match s {
                    DrawState::Transparent => return 0x00000000,
                    DrawState::Color(c) => return *c,
                    DrawState::Token(t) => return self.palette().color(t),
                    _ => {}
                }
            }
        }
        self.palette().color("text")
    }

    /// Resolve a named palette color token (e.g. "selection") to an ARGB color.
    fn color(&self, token: &str) -> u32 {
        self.palette().color(token)
    }

    /// Resolve a named palette dimension token (e.g. "scrollbar.thickness")
    /// to dips. Layout code without a `Theme` instance uses
    /// `drawing::current_dimension` instead.
    fn dimension(&self, token: &str) -> f32 {
        self.palette().dimension(token)
    }

    fn set_clip(&mut self, rect: Rect<i32>);
    fn clip_rect(&mut self, rect: Rect<i32>) -> Rect<i32>;
    fn push_clip(&mut self);
    fn pop_clip(&mut self);

    fn draw_text(&mut self, x: f32, y: f32, color: u32, text: &TextBlock);

    /// Like `draw_text`, but only glyphs inside `crop` are drawn (partial glyphs cropped).
    fn draw_text_cropped(&mut self, x: f32, y: f32, crop: Rect<i32>, color: u32, text: &TextBlock) {
        self.push_clip();
        self.clip_rect(crop);
        self.draw_text(x, y, color, text);
        self.pop_clip();
    }

    fn draw_rect(&mut self, rect: Rect<i32>, color: u32);

    /// Filled rectangle with rounded corners. `radius` is in physical pixels —
    /// callers are expected to pre-multiply by scale. Default falls back to a
    /// square `draw_rect` for themes that don't implement rounding.
    fn draw_rounded_rect(&mut self, rect: Rect<i32>, color: u32, _radius: i32) {
        self.draw_rect(rect, color);
    }

    // New drawable-based methods
    /// Draw a drawable at the specified rectangle
    fn draw_drawable(&mut self, drawable: &Drawable, rect: Rect<i32>);

    /// Get the drawable registry for this theme
    fn get_drawable_registry(&self) -> &DrawableRegistry;

    /// Draw a widget visual by role name (e.g. "button.back"), resolved to
    /// the theme's drawable for that role and the given state.
    fn draw_component(&mut self, role: &str, rect: Rect<i32>, state: ViewState);

    /// Draw an image from raw file bytes, scaled to fit the given rect.
    /// `cache_key` is a caller-supplied stable key (an `ImageSource` id) used to
    /// upload the decoded texture once and reuse it on subsequent frames.
    fn draw_image(&mut self, _rect: Rect<i32>, _image_bytes: &[u8], _cache_key: u64) {}

    /// Draw a pre-decoded RGBA8 image of the given pixel size into `rect`.
    /// `cache_key` is a caller-supplied stable key used to avoid re-uploading
    /// the same buffer on subsequent frames.
    fn draw_raw_image(&mut self, _rect: Rect<i32>, _rgba: &[u8], _size: (u32, u32), _cache_key: u64) {}

    /// Draw an image from raw file bytes, multiplied by an ARGB tint colour.
    /// `0xFFFFFFFF` means "no change". `0x80FFFFFF` halves opacity. `0xFFFF0000`
    /// multiplies the image by red full-alpha. Default falls back to plain `draw_image`.
    fn draw_image_tinted(&mut self, rect: Rect<i32>, image_bytes: &[u8], cache_key: u64, _tint_argb: u32) {
        self.draw_image(rect, image_bytes, cache_key);
    }

    /// Tinted variant of `draw_raw_image`. See `draw_image_tinted` for tint semantics.
    fn draw_raw_image_tinted(&mut self, rect: Rect<i32>, rgba: &[u8], size: (u32, u32), cache_key: u64, _tint_argb: u32) {
        self.draw_raw_image(rect, rgba, size, cache_key);
    }

    // Opacity stack for disabled views
    fn push_opacity(&mut self, _opacity: f32) {}
    fn pop_opacity(&mut self) {}
}

/// Contrast color for text drawn over a selection highlight: white on a dark
/// selection background, black on a light one (by perceived luminance).
pub fn selection_text_color(selection_background: u32) -> u32 {
    let r = (selection_background >> 16) & 0xff;
    let g = (selection_background >> 8) & 0xff;
    let b = selection_background & 0xff;
    let lum = (299 * r + 587 * g + 114 * b) / 1000;
    if lum >= 128 { 0xff000000 } else { 0xffffffff }
}

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic
}

impl ToString for FontStyle {
    fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

impl From<&str> for FontStyle {
    fn from(s: &str) -> Self {
        match s {
            "Bold" => FontStyle::Bold,
            "Italic" => FontStyle::Italic,
            "BoldItalic" => FontStyle::BoldItalic,
            &_ => FontStyle::Regular
        }
    }
}

impl From<String> for FontStyle {
    fn from(s: String) -> Self {
        FontStyle::from(s.as_str())
    }
}

#[derive(Clone)]
pub struct Typeface {
    pub font_name: String,
    pub font_style: FontStyle,
    pub font_size: Option<f32>
}

/// The standard UI font of the current OS. Resolved through the system font
/// source at load time, so nothing needs to be bundled with the app.
pub fn default_font_name() -> &'static str {
    #[cfg(target_os = "windows")]
    { "Segoe UI" }
    #[cfg(target_os = "macos")]
    { "Helvetica Neue" }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    // Generic family — fontconfig resolves it to the distro's default sans.
    { "sans-serif" }
}

impl Default for Typeface {
    fn default() -> Self {
        Typeface { font_name: String::from(default_font_name()), font_style: FontStyle::Regular, font_size: None }
    }
}

#[allow(unused)]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct ViewState {
    pub enabled: bool,
    pub focusable: bool,
    pub focused: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub checked: bool
}

#[allow(unused)]
impl ViewState {
    pub fn no_focus() -> Self {
        ViewState {
            enabled: true,
            focusable: false,
            focused: false,
            hovered: false,
            pressed: false,
            checked: false
        }
    }
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            enabled: true,
            focusable: true,
            focused: false,
            hovered: false,
            pressed: false,
            checked: false
        }
    }
}