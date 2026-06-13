use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::themes::Typeface;

/// The theme's resource bundle: named colors, dimensions and typefaces.
///
/// Colors are resolved at draw time by the drawing engine (for
/// `color="@token"` references in drawable XML) and by views (via
/// `Theme::color`). Every color entry must carry an explicit alpha byte
/// (`0xAARRGGBB`): token values are used verbatim via `Color::from_hex_argb`,
/// while literal 6-digit hex colors in drawable XML get full alpha added by
/// the parser. An entry without alpha would render fully transparent.
///
/// Dimensions are in dips (callers multiply by scale). Because layout runs
/// before any per-frame `Theme` exists, views read them through
/// [`current_dimension`], which queries the thread-local palette kept in sync
/// by the window handler.
#[derive(Clone)]
pub struct Palette {
    colors: HashMap<String, u32>,
    dimensions: HashMap<String, f32>,
    typefaces: HashMap<String, Typeface>,
}

thread_local! {
    /// The palette currently in effect, for resolution outside paint (layout
    /// code, app startup). The window handler replaces it on palette change.
    static CURRENT: RefCell<Rc<Palette>> = RefCell::new(Rc::new(Palette::classic()));
}

/// Make `palette` the one returned by the thread-local accessors below.
/// Called by the window handler whenever the active palette changes.
pub fn set_current_palette(palette: Palette) {
    CURRENT.with(|current| *current.borrow_mut() = Rc::new(palette));
}

/// Resolve a dimension token (dips) against the currently active palette.
/// Usable from layout code, where no `Theme` instance exists yet.
pub fn current_dimension(token: &str) -> f32 {
    CURRENT.with(|current| current.borrow().dimension(token))
}

/// Resolve a color token against the currently active palette. For code that
/// runs outside paint (e.g. the tooltip is assembled in `UI` before any
/// `Theme` instance exists); paint code should prefer `Theme::color`.
pub fn current_color(token: &str) -> u32 {
    CURRENT.with(|current| current.borrow().color(token))
}

/// Resolve a typeface role against the currently active palette.
pub fn current_typeface(role: &str) -> Typeface {
    CURRENT.with(|current| current.borrow().typeface(role))
}

/// The font size (dips) of a typeface role in the currently active palette.
/// Views use this as the fallback when neither they nor an ancestor set an
/// explicit `font_size`.
pub fn current_text_size(role: &str) -> f32 {
    current_typeface(role).font_size.unwrap_or(crate::common::DEFAULT_TEXT_SIZE)
}

impl Palette {
    /// The palette of the Classic (Win95-style) theme.
    pub fn classic() -> Self {
        let colors = HashMap::from([
            ("background".to_string(), 0xFFD4D0C8),
            ("background_hover".to_string(), 0xFFE4E0D8),
            ("surface".to_string(), 0xFFFFFFFF),
            ("highlight".to_string(), 0xFFFFFFFF),
            ("border_light".to_string(), 0xFF808080),
            ("border_dark".to_string(), 0xFF404040),
            ("text".to_string(), 0xFF000000),
            ("text_hint".to_string(), 0xFF808080),
            ("selection".to_string(), 0xFF000080),
            ("item_highlight".to_string(), 0xFF0000C0),
            ("item_highlight_text".to_string(), 0xFFFFFFFF),
            ("menu_highlight".to_string(), 0xFFBAB6AE),
            ("menu_highlight_text".to_string(), 0xFF000000),
            ("table_selection".to_string(), 0xFFCCE0F5),
            ("table_separator".to_string(), 0xFFD0D0D0),
            ("progress_fill".to_string(), 0xFF000080),
            ("outline".to_string(), 0xFF808080),
            ("focus".to_string(), 0xFF303030),
            ("tooltip_back".to_string(), 0xFFFFFFDD),
            ("tooltip_border".to_string(), 0xFF808080),
            ("tooltip_text".to_string(), 0xFF000000),
            ("link".to_string(), 0xFF3273DC),
            ("mark".to_string(), 0xFFFFF59D),
            ("error".to_string(), 0xFFD83A3A),
            ("icon_tint".to_string(), 0xFFFFFFFF),
        ]);
        Palette { colors, dimensions: Self::default_dimensions(), typefaces: Self::default_typefaces() }
    }

    /// A dark counterpart of the Classic palette: same raised/sunken 3D
    /// language, dark gray faces, light text.
    pub fn dark() -> Self {
        let colors = HashMap::from([
            ("background".to_string(), 0xFF3C3C3C),
            ("background_hover".to_string(), 0xFF4A4A4A),
            ("surface".to_string(), 0xFF252525),
            ("highlight".to_string(), 0xFF5F5F5F),
            ("border_light".to_string(), 0xFF2B2B2B),
            ("border_dark".to_string(), 0xFF161616),
            ("text".to_string(), 0xFFE0E0E0),
            ("text_hint".to_string(), 0xFF6A6A6A),
            ("selection".to_string(), 0xFF264F78),
            ("item_highlight".to_string(), 0xFF3060A8),
            ("item_highlight_text".to_string(), 0xFFFFFFFF),
            ("menu_highlight".to_string(), 0xFF505050),
            ("menu_highlight_text".to_string(), 0xFFE0E0E0),
            ("table_selection".to_string(), 0xFF2A4D6E),
            ("table_separator".to_string(), 0xFF454545),
            ("progress_fill".to_string(), 0xFF2D7DD2),
            ("outline".to_string(), 0xFF6A6A6A),
            ("focus".to_string(), 0xFF808080),
            ("tooltip_back".to_string(), 0xFF202020),
            ("tooltip_border".to_string(), 0xFF5F5F5F),
            ("tooltip_text".to_string(), 0xFFE0E0E0),
            ("link".to_string(), 0xFF6CA9F0),
            ("mark".to_string(), 0xFF6B5E1F),
            ("error".to_string(), 0xFFE5484D),
            ("icon_tint".to_string(), 0xFFFFFFFF),
        ]);
        Palette { colors, dimensions: Self::default_dimensions(), typefaces: Self::default_typefaces() }
    }

    /// Dimension tokens (dips) shared by both built-in palettes.
    fn default_dimensions() -> HashMap<String, f32> {
        HashMap::from([
            ("scrollbar.thickness".to_string(), 16.0),
            ("caret.width".to_string(), 1.0),
            ("checkbox.box_size".to_string(), 16.0),
            ("radio.box_size".to_string(), 16.0),
            ("radio.left_inset".to_string(), 4.0),
            ("menu.min_width".to_string(), 120.0),
        ])
    }

    /// Typeface roles shared by both built-in palettes. Unknown roles fall
    /// back to "default", so themes only need to override the roles they
    /// care about. Every role carries the font size for its kind of view;
    /// the OS UI font is used for all of them (see `default_font_name`).
    fn default_typefaces() -> HashMap<String, Typeface> {
        let sized = |size: f32| Typeface { font_size: Some(size), ..Typeface::default() };
        HashMap::from([
            ("default".to_string(), sized(16.0)),
            ("text".to_string(), sized(16.0)),
            ("label".to_string(), sized(16.0)),
            ("button".to_string(), sized(16.0)),
            ("menu".to_string(), sized(16.0)),
        ])
    }

    /// Resolve a token to an ARGB color. An unknown token is a bug in the
    /// theme or drawable: it panics in debug builds and renders magenta in
    /// release builds so it stays visible.
    pub fn color(&self, token: &str) -> u32 {
        match self.colors.get(token) {
            Some(color) => *color,
            None => {
                debug_assert!(false, "Unknown color token: {}", token);
                0xFFFF00FF
            }
        }
    }

    /// Resolve a dimension token to dips. An unknown token is a bug in the
    /// theme: it panics in debug builds and collapses to 0 in release builds
    /// so it stays visible.
    pub fn dimension(&self, token: &str) -> f32 {
        match self.dimensions.get(token) {
            Some(value) => *value,
            None => {
                debug_assert!(false, "Unknown dimension token: {}", token);
                0.0
            }
        }
    }

    /// Resolve a typeface role; unknown roles fall back to "default".
    pub fn typeface(&self, role: &str) -> Typeface {
        match self.typefaces.get(role) {
            Some(typeface) => typeface.clone(),
            None => self.typefaces.get("default").cloned().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classic_tokens_resolve() {
        let palette = Palette::classic();
        assert_eq!(palette.color("background"), 0xFFD4D0C8);
        assert_eq!(palette.color("text"), 0xFF000000);
        assert_eq!(palette.color("selection"), 0xFF000080);
    }

    #[test]
    fn test_all_classic_tokens_have_alpha() {
        let palette = Palette::classic();
        for (token, color) in &palette.colors {
            assert_eq!(color >> 24, 0xFF, "token '{}' must carry explicit FF alpha", token);
        }
    }

    #[test]
    fn test_dimension_tokens_resolve() {
        let palette = Palette::classic();
        assert_eq!(palette.dimension("scrollbar.thickness"), 16.0);
        assert_eq!(palette.dimension("caret.width"), 1.0);
        // dark palette covers the same dimension tokens
        let dark = Palette::dark();
        for token in palette.dimensions.keys() {
            assert!(dark.dimensions.contains_key(token), "dark missing dimension '{}'", token);
        }
    }

    #[test]
    fn test_typeface_roles_fall_back_to_default() {
        let palette = Palette::classic();
        let default = palette.typeface("default");
        let unknown = palette.typeface("no_such_role");
        assert_eq!(default.font_name, unknown.font_name);
    }

    #[test]
    fn test_dark_covers_same_tokens_as_classic() {
        let classic = Palette::classic();
        let dark = Palette::dark();
        let mut classic_tokens: Vec<&String> = classic.colors.keys().collect();
        let mut dark_tokens: Vec<&String> = dark.colors.keys().collect();
        classic_tokens.sort();
        dark_tokens.sort();
        assert_eq!(classic_tokens, dark_tokens);
        for (token, color) in &dark.colors {
            assert_eq!(color >> 24, 0xFF, "token '{}' must carry explicit FF alpha", token);
        }
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_unknown_token_is_magenta() {
        let palette = Palette::classic();
        assert_eq!(palette.color("no_such_token"), 0xFFFF00FF);
    }
}
