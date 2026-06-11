use std::cell::RefCell;
use std::collections::HashMap;

use font_kit::family_name::FamilyName;
use font_kit::properties::{Properties, Style as FkStyle, Weight};
use font_kit::source::SystemSource;
use speedy2d::font::{Font, FontFamily};

use crate::themes::FontStyle;

pub trait AssetsProvider {
    fn get_file(&self, path: &str) -> Option<&[u8]>;
}

thread_local! {
    static PROVIDER: RefCell<Option<Box<dyn AssetsProvider>>> = const { RefCell::new(None) };
    static FONTS: RefCell<HashMap<(String, FontStyle), Font>> = RefCell::new(HashMap::new());
    static FAMILIES: RefCell<HashMap<(String, FontStyle), FontFamily>> = RefCell::new(HashMap::new());
    static FALLBACKS: RefCell<Vec<(String, FontStyle)>> = const { RefCell::new(Vec::new()) };
    static SYSTEM: RefCell<Option<SystemSource>> = const { RefCell::new(None) };
}

pub fn set_provider(value: Box<impl AssetsProvider + 'static>) {
    PROVIDER.with(|cell| {
        *cell.borrow_mut() = Some(value);
    });
}

pub fn get_asset(path: &str) -> Option<Vec<u8>> {
    let mut result = None;
    PROVIDER.with(|provider| {
        if let Some(p) = provider.borrow().as_ref() {
            if let Some(bytes) = p.get_file(path) {
                result = Some(bytes.to_vec());
            }
        }
    });
    result
}

/// Configures the global font fallback chain. Each entry is resolved through
/// the same algorithm as the primary font (system → assets → last-resort), so
/// fallback entries can themselves be system-named, generic, or asset-only.
/// Calling this clears the assembled-family cache so the next layout rebuilds
/// every family with the new chain.
pub fn set_font_fallbacks(chain: Vec<(String, FontStyle)>) {
    FALLBACKS.with(|c| *c.borrow_mut() = chain);
    FAMILIES.with(|c| c.borrow_mut().clear());
}

/// Returns a `FontFamily` whose first entry is the requested font and whose
/// tail is the configured fallback chain (with missing fonts silently dropped).
/// Returns `None` only when the primary font cannot be resolved through any
/// path.
pub fn get_font_family(name: &str, style: FontStyle) -> Option<FontFamily> {
    let key = (name.to_owned(), style);
    if let Some(fam) = FAMILIES.with(|c| c.borrow().get(&key).cloned()) {
        return Some(fam);
    }

    let primary = resolve_primary(name, style)?;
    let mut chain = vec![primary];

    let fallbacks = FALLBACKS.with(|c| c.borrow().clone());
    for (n, s) in fallbacks {
        if (n.as_str(), s) == (name, style) {
            continue;
        }
        if let Some(f) = resolve_primary(&n, s) {
            chain.push(f);
        }
    }

    let fam = FontFamily::new(chain);
    FAMILIES.with(|c| c.borrow_mut().insert(key, fam.clone()));
    Some(fam)
}

fn resolve_primary(name: &str, style: FontStyle) -> Option<Font> {
    let cache_key = (name.to_owned(), style);
    if let Some(f) = FONTS.with(|c| c.borrow().get(&cache_key).cloned()) {
        return Some(f);
    }

    let font = try_system(name, style)
        .or_else(|| try_assets(name, style))
        .or_else(|| {
            // Last-resort fallback for the default typeface so a freshly
            // bootstrapped app always renders text.
            if name == crate::themes::default_font_name() || name == "NotoSans" {
                try_system("sans-serif", style)
            } else {
                None
            }
        })?;

    FONTS.with(|c| c.borrow_mut().insert(cache_key, font.clone()));
    Some(font)
}

fn try_system(name: &str, style: FontStyle) -> Option<Font> {
    SYSTEM.with(|s| {
        if s.borrow().is_none() {
            *s.borrow_mut() = Some(SystemSource::new());
        }
        let src_ref = s.borrow();
        let src = src_ref.as_ref()?;
        let handle = src
            .select_best_match(&[family_name_for(name)], &properties_for(style))
            .ok()?;
        let fk_font = handle.load().ok()?;
        let bytes = fk_font.copy_font_data()?;
        Font::new(bytes.as_ref()).ok()
    })
}

fn try_assets(name: &str, style: FontStyle) -> Option<Font> {
    let normalized_name = name.replace(' ', "");
    let style_str = format!("{:?}", style);
    let path = format!(
        "fonts{}{}-{}.ttf",
        std::path::MAIN_SEPARATOR,
        normalized_name,
        style_str
    );
    PROVIDER.with(|p| {
        let provider = p.borrow();
        let provider = provider.as_ref()?;
        let bytes = provider.get_file(&path)?;
        Font::new(bytes).ok()
    })
}

fn properties_for(style: FontStyle) -> Properties {
    let mut p = Properties::new();
    match style {
        FontStyle::Regular => {}
        FontStyle::Bold => {
            p.weight = Weight::BOLD;
        }
        FontStyle::Italic => {
            p.style = FkStyle::Italic;
        }
        FontStyle::BoldItalic => {
            p.weight = Weight::BOLD;
            p.style = FkStyle::Italic;
        }
    }
    p
}

fn family_name_for(name: &str) -> FamilyName {
    match name {
        "sans-serif" => FamilyName::SansSerif,
        "serif" => FamilyName::Serif,
        "monospace" => FamilyName::Monospace,
        other => FamilyName::Title(other.to_owned()),
    }
}
