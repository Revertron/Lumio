use std::cell::RefCell;
use std::collections::HashMap;

#[cfg(feature = "system-fonts")]
use font_kit::family_name::FamilyName;
#[cfg(feature = "system-fonts")]
use font_kit::properties::{Properties, Style as FkStyle, Weight};
#[cfg(feature = "system-fonts")]
use font_kit::source::SystemSource;

use crate::text::FontHandle;
use crate::themes::FontStyle;

pub trait AssetsProvider {
    fn get_file(&self, path: &str) -> Option<&[u8]>;
}

thread_local! {
    static PROVIDER: RefCell<Option<Box<dyn AssetsProvider>>> = const { RefCell::new(None) };
    static FALLBACKS: RefCell<Vec<(String, FontStyle)>> = const { RefCell::new(Vec::new()) };
    #[cfg(feature = "system-fonts")]
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
    // Absolute paths the provider doesn't know fall through to the filesystem,
    // so apps can display user files (avatars, downloads) without bundling them.
    if result.is_none() {
        let p = std::path::Path::new(path);
        if p.is_absolute() {
            result = std::fs::read(p).ok();
        }
    }
    result
}

/// Configures the global font fallback chain. Each entry is resolved through
/// the same algorithm as the primary font (system → assets → last-resort), so
/// fallback entries can themselves be system-named, generic, or asset-only.
/// Calling this clears the assembled-family cache so the next layout rebuilds
/// every family with the new chain.
pub fn set_font_fallbacks(chain: Vec<(String, FontStyle)>) {
    FALLBACKS.with(|c| *c.borrow_mut() = chain);
    clear_family_cache();
}

// ---------------------------------------------------------------------------
// Backend-neutral font BYTE resolution. Both text backends build their own font
// objects (speedy2d `Font` / fontdue `Font`) from these raw bytes.
// ---------------------------------------------------------------------------

/// System → bundled-asset → last-resort `sans-serif` (for the default/Noto
/// typeface) raw font bytes.
fn resolve_font_bytes(name: &str, style: FontStyle) -> Option<Vec<u8>> {
    system_font_bytes(name, style)
        .or_else(|| asset_font_bytes(name, style))
        .or_else(|| {
            // Last-resort so a freshly bootstrapped app always renders text.
            if name == crate::themes::default_font_name() || name == "NotoSans" {
                system_font_bytes("sans-serif", style)
            } else {
                None
            }
        })
}

#[cfg(feature = "system-fonts")]
fn system_font_bytes(name: &str, style: FontStyle) -> Option<Vec<u8>> {
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
        Some(bytes.as_ref().clone())
    })
}

// No native system-font source in the headless `software` core: fonts come solely
// from the `AssetsProvider` bundle (and the configured fallback chain).
#[cfg(not(feature = "system-fonts"))]
fn system_font_bytes(_name: &str, _style: FontStyle) -> Option<Vec<u8>> {
    None
}

fn asset_font_bytes(name: &str, style: FontStyle) -> Option<Vec<u8>> {
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
        Some(provider.get_file(&path)?.to_vec())
    })
}

#[cfg(feature = "system-fonts")]
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

#[cfg(feature = "system-fonts")]
fn family_name_for(name: &str) -> FamilyName {
    match name {
        "sans-serif" => FamilyName::SansSerif,
        "serif" => FamilyName::Serif,
        "monospace" => FamilyName::Monospace,
        other => FamilyName::Title(other.to_owned()),
    }
}

// ---------------------------------------------------------------------------
// speedy2d text backend: build `FontFamily` from the resolved bytes.
// ---------------------------------------------------------------------------
#[cfg(feature = "text-speedy2d")]
mod backend {
    use super::*;
    use speedy2d::font::{Font, FontFamily};

    thread_local! {
        static FONTS: RefCell<HashMap<(String, FontStyle), Font>> = RefCell::new(HashMap::new());
        static FAMILIES: RefCell<HashMap<(String, FontStyle), FontFamily>> = RefCell::new(HashMap::new());
    }

    pub(super) fn clear_family_cache() {
        FAMILIES.with(|c| c.borrow_mut().clear());
    }

    pub(super) fn get_font_family(name: &str, style: FontStyle) -> Option<FontHandle> {
        let key = (name.to_owned(), style);
        if let Some(fam) = FAMILIES.with(|c| c.borrow().get(&key).cloned()) {
            return Some(FontHandle::new(fam));
        }

        let mut chain = vec![resolve_primary(name, style)?];
        for (n, s) in FALLBACKS.with(|c| c.borrow().clone()) {
            if (n.as_str(), s) == (name, style) {
                continue;
            }
            if let Some(f) = resolve_primary(&n, s) {
                chain.push(f);
            }
        }

        let fam = FontFamily::new(chain);
        FAMILIES.with(|c| c.borrow_mut().insert(key, fam.clone()));
        Some(FontHandle::new(fam))
    }

    fn resolve_primary(name: &str, style: FontStyle) -> Option<Font> {
        let key = (name.to_owned(), style);
        if let Some(f) = FONTS.with(|c| c.borrow().get(&key).cloned()) {
            return Some(f);
        }
        let font = Font::new(&resolve_font_bytes(name, style)?).ok()?;
        FONTS.with(|c| c.borrow_mut().insert(key, font.clone()));
        Some(font)
    }
}

// ---------------------------------------------------------------------------
// fontdue software text backend: build `Rc<Vec<fontdue::Font>>` from the bytes.
// ---------------------------------------------------------------------------
#[cfg(feature = "text-software")]
mod backend {
    use super::*;
    use std::rc::Rc;
    use fontdue::{Font, FontSettings};

    thread_local! {
        static FAMILIES: RefCell<HashMap<(String, FontStyle), Rc<Vec<Font>>>> = RefCell::new(HashMap::new());
    }

    pub(super) fn clear_family_cache() {
        FAMILIES.with(|c| c.borrow_mut().clear());
    }

    pub(super) fn get_font_family(name: &str, style: FontStyle) -> Option<FontHandle> {
        let key = (name.to_owned(), style);
        if let Some(fam) = FAMILIES.with(|c| c.borrow().get(&key).cloned()) {
            return Some(FontHandle::new(fam));
        }

        let mut chain = vec![build_font(name, style)?];
        for (n, s) in FALLBACKS.with(|c| c.borrow().clone()) {
            if (n.as_str(), s) == (name, style) {
                continue;
            }
            if let Some(f) = build_font(&n, s) {
                chain.push(f);
            }
        }

        let fam = Rc::new(chain);
        FAMILIES.with(|c| c.borrow_mut().insert(key, Rc::clone(&fam)));
        Some(FontHandle::new(fam))
    }

    fn build_font(name: &str, style: FontStyle) -> Option<Font> {
        let bytes = resolve_font_bytes(name, style)?;
        Font::from_bytes(bytes, FontSettings::default()).ok()
    }
}

fn clear_family_cache() {
    backend::clear_family_cache();
}

/// Returns a [`FontHandle`] whose first entry is the requested font and whose
/// tail is the configured fallback chain (missing fonts silently dropped).
/// Returns `None` only when the primary font cannot be resolved through any
/// path. The wrapped object is the active text backend's font (speedy2d
/// `FontFamily` or `Rc<Vec<fontdue::Font>>`) — an implementation detail.
pub fn get_font_family(name: &str, style: FontStyle) -> Option<FontHandle> {
    backend::get_font_family(name, style)
}
