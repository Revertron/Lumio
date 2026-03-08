mod classic;
mod utils;

use speedy2d::font::FormattedTextBlock;
use super::styles::selector::MainSelector;
use super::drawing::{Drawable, DrawableRegistry};
pub use self::classic::Classic;
pub use self::classic::ImageCache;
use super::types::Rect;

pub trait Theme {
    fn clear_screen(&mut self);
    fn typeface() -> Typeface where Self: Sized;
    fn get_back_color(&self, state: ViewState, selector: &MainSelector) -> u32;
    fn get_text_color(&self, state: ViewState, selector: &MainSelector) -> u32;
    fn set_clip(&mut self, rect: Rect<i32>);
    fn clip_rect(&mut self, rect: Rect<i32>) -> Rect<i32>;
    fn push_clip(&mut self);
    fn pop_clip(&mut self);

    // Legacy drawing methods (will be deprecated)
    fn draw_button_back(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_button_body(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_button_text(&mut self, rect: Rect<i32>, state: ViewState, size: usize, text: &str);
    fn draw_edit_back(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_edit_body(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_edit_caret(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_checkbox_back(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_checkbox_body(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_checkbox_checkmark(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_radiobutton_back(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_radiobutton_body(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_radiobutton_indicator(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_combobox_arrow(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_list_back(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_list_body(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_panel_back(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_panel_body(&mut self, rect: Rect<i32>, state: ViewState);
    fn draw_text(&mut self, x: f32, y: f32, color: u32, text: &FormattedTextBlock);
    fn draw_rect(&mut self, rect: Rect<i32>, color: u32);

    // New drawable-based methods
    /// Draw a drawable at the specified rectangle
    fn draw_drawable(&mut self, drawable: &Drawable, rect: Rect<i32>);

    /// Get the drawable registry for this theme
    fn get_drawable_registry(&self) -> &DrawableRegistry;

    /// Draw a component using a drawable from the registry
    /// This method has a default implementation that can be overridden
    fn draw_component(&mut self, drawable_name: &str, rect: Rect<i32>, state: ViewState);

    /// Draw an image from raw file bytes, scaled to fit the given rect.
    /// The image is cached by the byte slice pointer for efficiency.
    fn draw_image(&mut self, rect: Rect<i32>, image_bytes: &[u8]);

    // Scrollbar drawing methods
    fn draw_scrollbar_track(&mut self, rect: Rect<i32>, direction: super::views::Direction);
    fn draw_scrollbar_thumb(&mut self, rect: Rect<i32>, state: ViewState, direction: super::views::Direction);
    fn draw_scrollbar_arrow_button(&mut self, rect: Rect<i32>, state: ViewState, toward_start: bool, direction: super::views::Direction);
}

#[allow(unused)]
#[derive(Debug, Clone)]
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
    pub font_style: FontStyle
}

impl Default for Typeface {
    fn default() -> Self {
        Typeface { font_name: String::from("NotoSans"), font_style: FontStyle::Regular }
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