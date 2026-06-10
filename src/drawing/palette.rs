use std::collections::HashMap;

/// Named color tokens resolved at draw time by the drawing engine
/// (for `color="@token"` references in drawable XML) and by views
/// (via `Theme::color`).
///
/// Every entry must carry an explicit alpha byte (`0xAARRGGBB`): token values
/// are used verbatim via `Color::from_hex_argb`, while literal 6-digit hex
/// colors in drawable XML get full alpha added by the parser. An entry without
/// alpha would render fully transparent.
pub struct Palette {
    colors: HashMap<String, u32>,
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
            ("table_selection".to_string(), 0xFFCCE0F5),
            ("table_separator".to_string(), 0xFFD0D0D0),
            ("progress_fill".to_string(), 0xFF000080),
        ]);
        Palette { colors }
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
            ("table_selection".to_string(), 0xFF2A4D6E),
            ("table_separator".to_string(), 0xFF454545),
            ("progress_fill".to_string(), 0xFF2D7DD2),
        ]);
        Palette { colors }
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
