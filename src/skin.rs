//! A [`Skin`] is the swappable visual bundle a window is painted with: the
//! [`Palette`] (colors, dimensions, typefaces) together with the drawable
//! *forms* ([`DrawableRegistry`]). The two built-in skins — [`Skin::light`] and
//! [`Skin::dark`] — share the same classic (Win95-style) forms and differ only
//! in their palette. That split is the whole point: a skin overrides only what
//! it changes, so "dark mode" is a recolor of one form set, not a second set.
//!
//! The window loop holds one `Skin` per window and paints through its palette
//! and drawables. A runtime palette swap (`UI::set_palette`) replaces the skin's
//! palette while keeping its forms.
//!
//! Phase 1 introduces the type and folds the window's palette + drawable set
//! into it; selecting or registering skins from app code arrives later.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::drawing::parser::DrawableParser;
use crate::drawing::{base_drawables, DrawableRegistry, Palette};
use crate::ninepatch::NinePatchBackground;
use crate::themes::{default_font_name, FontStyle, Typeface};

/// The visual bundle a window is painted with: a [`Palette`] plus the drawable
/// forms ([`DrawableRegistry`]). Cheap to clone — the form set is shared behind
/// an `Rc`.
#[derive(Clone)]
pub struct Skin {
    name: String,
    palette: Palette,
    drawables: Rc<DrawableRegistry>,
}

impl Skin {
    /// The built-in light skin: the classic (Win95-style) forms and palette.
    pub fn light() -> Self {
        Self::from_named_palette("light", Palette::classic())
    }

    /// The built-in dark skin: the same classic forms as [`light`](Self::light),
    /// recolored by the dark palette.
    pub fn dark() -> Self {
        Self::from_named_palette("dark", Palette::dark())
    }

    /// Start building a custom skin. It inherits the light palette and the
    /// shared classic form set; override the palette and individual drawable
    /// roles, then [`build`](SkinBuilder::build). Roles left unset fall back to
    /// the base forms, so a skin only carries what it changes.
    ///
    /// ```no_run
    /// use lumio::skin::{Skin, BuiltinSkin};
    /// let flat = Skin::builder("flat")
    ///     .base(BuiltinSkin::Dark)
    ///     .drawable("button.back", r#"<selector><item><layer-list><item>
    ///         <shape type="rect"><solid color="@progress_fill"/></shape>
    ///     </item></layer-list></item></selector>"#)
    ///     .build();
    /// ```
    pub fn builder(name: impl Into<String>) -> SkinBuilder {
        SkinBuilder {
            name: name.into(),
            palette: Palette::classic(),
            colors: Vec::new(),
            dimensions: Vec::new(),
            typefaces: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Wrap an arbitrary palette in a skin over the classic form set. Used by
    /// the window loop to build a skin from a `WindowConfig` palette until the
    /// public skin-selection API lands.
    pub(crate) fn from_palette(palette: Palette) -> Self {
        Self::from_named_palette("default", palette)
    }

    fn from_named_palette(name: &str, palette: Palette) -> Self {
        Skin {
            name: name.to_string(),
            palette,
            drawables: base_drawables(),
        }
    }

    /// This skin's name (e.g. `"light"`, `"dark"`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The palette this skin resolves colors, dimensions and typefaces against.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The drawable form set this skin paints widgets with.
    pub fn drawables(&self) -> &DrawableRegistry {
        &self.drawables
    }

    /// Replace the palette while keeping the form set. Backs the runtime palette
    /// swap (`UI::set_palette`) so a recolor doesn't rebuild the drawables.
    pub(crate) fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }
}

/// Which built-in skin a custom skin inherits its palette from. Both share the
/// classic form set; they differ only in palette.
#[derive(Clone, Copy)]
pub enum BuiltinSkin {
    Light,
    Dark,
}

impl BuiltinSkin {
    fn palette(self) -> Palette {
        match self {
            BuiltinSkin::Light => Palette::classic(),
            BuiltinSkin::Dark => Palette::dark(),
        }
    }
}

/// Builder for a custom [`Skin`]; see [`Skin::builder`]. Overridden drawable
/// roles overlay the shared base (classic) form set — everything not overridden
/// falls back to it.
pub struct SkinBuilder {
    name: String,
    palette: Palette,
    colors: Vec<(String, u32)>,
    dimensions: Vec<(String, f32)>,
    typefaces: Vec<(String, Typeface)>,
    overrides: Vec<(String, String)>,
}

impl SkinBuilder {
    /// Inherit the palette of a built-in skin (both share the classic forms).
    pub fn base(mut self, base: BuiltinSkin) -> Self {
        self.palette = base.palette();
        self
    }

    /// Use an explicit palette.
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    /// Override a single palette color token (e.g. `"selection"`) on top of the
    /// base/palette. Applied at [`build`](Self::build), so it wins regardless of
    /// whether [`base`](Self::base)/[`palette`](Self::palette) is set before or
    /// after it.
    pub fn color(mut self, token: impl Into<String>, argb: u32) -> Self {
        self.colors.push((token.into(), argb));
        self
    }

    /// Override a single palette dimension token (dips, e.g.
    /// `"scrollbar.thickness"`) on top of the base/palette.
    pub fn dimension(mut self, token: impl Into<String>, value: f32) -> Self {
        self.dimensions.push((token.into(), value));
        self
    }

    /// Override the typeface for a role (e.g. `"button"`) on top of the
    /// base/palette.
    pub fn typeface(mut self, role: impl Into<String>, typeface: Typeface) -> Self {
        self.typefaces.push((role.into(), typeface));
        self
    }

    /// Override the form for one role (e.g. `"button.back"`). `value` is either
    /// inline shape drawable XML (a `<selector>…</selector>` string) **or** a
    /// 9-patch asset path — a single `foo.9.png`, or a `<selector>` XML file
    /// (`bar.xml`) whose items reference `.9.png`s. Roles left unset fall back to
    /// the base (classic) form; an invalid value is logged and skipped.
    pub fn drawable(mut self, role: impl Into<String>, value: impl Into<String>) -> Self {
        self.overrides.push((role.into(), value.into()));
        self
    }

    /// Finish and produce the [`Skin`].
    pub fn build(self) -> Skin {
        // Layer per-token palette overrides on top of the base/palette.
        let mut palette = self.palette;
        for (token, argb) in self.colors {
            palette = palette.with_color(token, argb);
        }
        for (token, value) in self.dimensions {
            palette = palette.with_dimension(token, value);
        }
        for (role, typeface) in self.typefaces {
            palette = palette.with_typeface(role, typeface);
        }

        let drawables = if self.overrides.is_empty() {
            base_drawables()
        } else {
            let mut registry = DrawableRegistry::with_base(base_drawables());
            for (role, value) in &self.overrides {
                let v = value.trim();
                let lower = v.to_ascii_lowercase();
                if !v.starts_with('<') && lower.ends_with(".9.png") {
                    // Single 9-patch for all states.
                    registry.insert_ninepatch(role, NinePatchBackground::from_png(v));
                } else if !v.starts_with('<') && lower.ends_with(".xml") {
                    // Android-style 9-patch <selector> file (references .9.pngs).
                    match NinePatchBackground::from_selector(v) {
                        Ok(np) => registry.insert_ninepatch(role, np),
                        Err(e) => {
                            log::warn!("skin drawable '{role}': 9-patch selector '{v}' failed: {e}")
                        }
                    }
                } else {
                    // Inline shape drawable XML.
                    registry.load_drawable(role, v);
                }
            }
            Rc::new(registry)
        };
        Skin {
            name: self.name,
            palette,
            drawables,
        }
    }
}

impl Skin {
    /// Build a skin from a manifest XML document — one `<skin>` element with a
    /// required `name` and optional `base` (`"light"` or `"dark"`), containing
    /// any number of:
    ///
    /// - `<color token="selection" value="#8000FF"/>` (`#RRGGBB` or `#AARRGGBB`)
    /// - `<dimension token="scrollbar.thickness" value="12"/>`
    /// - `<typeface role="button" font="Segoe UI" style="Bold" size="18"/>`
    /// - `<drawable role="button.back" src="button.9.png"/>` — a 9-patch
    ///   (`.9.png`, or a `<selector>` `.xml` referencing per-state `.9.png`s)
    /// - `<drawable role="edit.back"><selector>…</selector></drawable>` — inline
    ///   shape drawable XML
    ///
    /// Sugar over [`Skin::builder`]. Errors on malformed XML, a missing `name`,
    /// or an unknown `base`; unknown child elements are ignored with a warning.
    ///
    /// ```
    /// use lumio::skin::Skin;
    /// // `r##"…"##` so the `"#` inside `value="#..."` doesn't end the string.
    /// let skin = Skin::from_xml(r##"
    ///     <skin name="flat" base="dark">
    ///         <color token="selection" value="#8000FF"/>
    ///         <drawable role="button.back" src="button.9.png"/>
    ///     </skin>
    /// "##).unwrap();
    /// assert_eq!(skin.name(), "flat");
    /// ```
    pub fn from_xml(xml: &str) -> Result<Skin, String> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut name: Option<String> = None;
        let mut base: Option<BuiltinSkin> = None;
        let mut colors: Vec<(String, u32)> = Vec::new();
        let mut dimensions: Vec<(String, f32)> = Vec::new();
        let mut typefaces: Vec<(String, Typeface)> = Vec::new();
        let mut drawables: Vec<(String, String)> = Vec::new();
        let mut seen_root = false;

        loop {
            let (e, is_start) = match reader.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) => (e, true),
                Event::Empty(e) => (e, false),
                Event::Eof => break,
                _ => continue,
            };

            if !seen_root {
                if e.name().as_ref() != b"skin" {
                    return Err(format!(
                        "skin manifest: expected root <skin>, found <{}>",
                        tag_str(&e)
                    ));
                }
                seen_root = true;
                name = Some(req_attr(&e, "name")?);
                if let Some(b) = DrawableParser::get_attr_opt(&e, "base") {
                    base = Some(parse_base(&b)?);
                }
                continue;
            }

            match e.name().as_ref() {
                b"color" => {
                    let token = req_attr(&e, "token")?;
                    let value = req_attr(&e, "value")?;
                    colors.push((token, DrawableParser::parse_color(value.trim())?));
                }
                b"dimension" => {
                    let token = req_attr(&e, "token")?;
                    let raw = req_attr(&e, "value")?;
                    let value: f32 = raw
                        .trim()
                        .parse()
                        .map_err(|_| format!("skin manifest: bad dimension value '{raw}'"))?;
                    dimensions.push((token, value));
                }
                b"typeface" => {
                    let role = req_attr(&e, "role")?;
                    let font = DrawableParser::get_attr_opt(&e, "font")
                        .unwrap_or_else(|| default_font_name().to_string());
                    let style = DrawableParser::get_attr_opt(&e, "style")
                        .map(FontStyle::from)
                        .unwrap_or(FontStyle::Regular);
                    let size = match DrawableParser::get_attr_opt(&e, "size") {
                        Some(s) => Some(
                            s.trim()
                                .parse::<f32>()
                                .map_err(|_| format!("skin manifest: bad typeface size '{s}'"))?,
                        ),
                        None => None,
                    };
                    typefaces.push((role, Typeface { font_name: font, font_style: style, font_size: size }));
                }
                b"drawable" => {
                    let role = req_attr(&e, "role")?;
                    if let Some(src) = DrawableParser::get_attr_opt(&e, "src") {
                        // A `src` 9-patch. If this is a Start tag it also has a
                        // body — skip to </drawable> so the body isn't reparsed
                        // as top-level elements.
                        if is_start {
                            reader.read_to_end(e.name()).map_err(|e| e.to_string())?;
                        }
                        drawables.push((role, src));
                    } else if is_start {
                        // Capture the inline shape drawable XML up to </drawable>.
                        let mut writer = Writer::new(Vec::new());
                        let mut depth = 0usize;
                        loop {
                            match reader.read_event().map_err(|e| e.to_string())? {
                                Event::Start(se) => {
                                    depth += 1;
                                    writer.write_event(Event::Start(se)).map_err(|e| e.to_string())?;
                                }
                                Event::End(ee) => {
                                    if depth == 0 {
                                        break;
                                    }
                                    depth -= 1;
                                    writer.write_event(Event::End(ee)).map_err(|e| e.to_string())?;
                                }
                                Event::Empty(se) => {
                                    writer.write_event(Event::Empty(se)).map_err(|e| e.to_string())?;
                                }
                                Event::Eof => return Err("skin manifest: unclosed <drawable>".to_string()),
                                _ => {}
                            }
                        }
                        let inner = String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())?;
                        drawables.push((role, inner));
                    } else {
                        return Err(format!(
                            "skin manifest: <drawable role=\"{role}\"> needs a src or inline content"
                        ));
                    }
                }
                other => log::warn!(
                    "skin manifest: ignoring unknown element <{}>",
                    String::from_utf8_lossy(other)
                ),
            }
        }

        let name = name.ok_or("skin manifest: no <skin> element")?;
        let mut builder = Skin::builder(name);
        if let Some(base) = base {
            builder = builder.base(base);
        }
        for (token, argb) in colors {
            builder = builder.color(token, argb);
        }
        for (token, value) in dimensions {
            builder = builder.dimension(token, value);
        }
        for (role, typeface) in typefaces {
            builder = builder.typeface(role, typeface);
        }
        for (role, value) in drawables {
            builder = builder.drawable(role, value);
        }
        Ok(builder.build())
    }
}

fn req_attr(e: &BytesStart, name: &str) -> Result<String, String> {
    DrawableParser::get_attr_opt(e, name)
        .ok_or_else(|| format!("skin manifest: <{}> missing '{}' attribute", tag_str(e), name))
}

fn tag_str(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn parse_base(s: &str) -> Result<BuiltinSkin, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "light" => Ok(BuiltinSkin::Light),
        "dark" => Ok(BuiltinSkin::Dark),
        other => Err(format!(
            "skin manifest: unknown base '{other}' (expected 'light' or 'dark')"
        )),
    }
}

thread_local! {
    /// App-registered skins by name. The built-in `"light"`/`"dark"` are
    /// resolved separately (see [`skin_by_name`]), so this starts empty.
    static SKIN_REGISTRY: RefCell<HashMap<String, Skin>> = RefCell::new(HashMap::new());
}

/// Register a skin so it can be selected by name via
/// [`WindowConfig::skin`](crate::WindowConfig::skin) and
/// [`UI::set_skin`](crate::ui::UI::set_skin). Keyed by [`Skin::name`]; a second
/// registration under the same name replaces the first. The registry is
/// app-global (per thread) — register skins at startup, before opening the
/// windows that reference them.
///
/// The names `"light"`, `"dark"`, and `"default"` are reserved (the two
/// built-in skins and the unnamed-window fallback); registering under one of
/// them is ignored with a warning.
///
/// ```no_run
/// use lumio::skin::{register_skin, Skin, BuiltinSkin};
/// register_skin(Skin::builder("flat").base(BuiltinSkin::Dark).build());
/// ```
pub fn register_skin(skin: Skin) {
    let name = skin.name().to_string();
    if matches!(name.as_str(), "light" | "dark" | "default") {
        log::warn!("register_skin: '{name}' is a reserved skin name; registration ignored");
        return;
    }
    SKIN_REGISTRY.with(|r| r.borrow_mut().insert(name, skin));
}

/// Resolve a skin by name: registered skins first, then the built-in
/// `"light"` / `"dark"`. Returns `None` for an unknown name.
pub fn skin_by_name(name: &str) -> Option<Skin> {
    if let Some(skin) = SKIN_REGISTRY.with(|r| r.borrow().get(name).cloned()) {
        return Some(skin);
    }
    match name {
        "light" => Some(Skin::light()),
        "dark" => Some(Skin::dark()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_resolve_and_registration_adds() {
        // Built-ins resolve without any registration; unknown names don't.
        assert_eq!(skin_by_name("light").unwrap().name(), "light");
        assert_eq!(skin_by_name("dark").unwrap().name(), "dark");
        assert!(skin_by_name("no_such_skin").is_none());

        // Registering makes a custom skin resolvable by its own name.
        register_skin(Skin::builder("unit_flat").base(BuiltinSkin::Dark).build());
        assert_eq!(skin_by_name("unit_flat").unwrap().name(), "unit_flat");
        // The dark base gave it the dark palette.
        assert_eq!(
            skin_by_name("unit_flat").unwrap().palette().color("background"),
            Palette::dark().color("background")
        );
    }

    #[test]
    fn builder_override_only_touches_that_role() {
        let skin = Skin::builder("t")
            .drawable("button.back", "<selector><item><layer-list><item><shape type=\"rect\"><solid color=\"@progress_fill\"/></shape></item></layer-list></item></selector>")
            .build();
        // Overridden + non-overridden roles both resolve (latter via base).
        assert!(skin.drawables().get("button.back").is_some());
        assert!(skin.drawables().get("checkbox.box").is_some());
    }

    #[test]
    fn builder_palette_token_overrides_apply() {
        let skin = Skin::builder("t")
            .base(BuiltinSkin::Dark)
            .color("selection", 0xFF8000FF)
            .dimension("scrollbar.thickness", 12.0)
            .build();
        assert_eq!(skin.palette().color("selection"), 0xFF8000FF);
        assert_eq!(skin.palette().dimension("scrollbar.thickness"), 12.0);
        // A non-overridden token keeps the dark base value.
        assert_eq!(
            skin.palette().color("background"),
            Palette::dark().color("background")
        );

        // Deferred apply: overrides win even when .base() is called AFTER them.
        let skin2 = Skin::builder("t2")
            .color("selection", 0xFF010203)
            .base(BuiltinSkin::Light)
            .build();
        assert_eq!(skin2.palette().color("selection"), 0xFF010203);
    }

    #[test]
    fn builder_routes_ninepatch_vs_shape() {
        let shape = "<selector><item><layer-list><item><shape type=\"rect\"><solid color=\"@surface\"/></shape></item></layer-list></item></selector>";
        let skin = Skin::builder("np")
            .drawable("button.back", "button.9.png") // 9-patch (lazy, no asset load)
            .drawable("edit.back", shape) // inline shape XML
            .build();
        // The .9.png routed to a 9-patch role; the inline XML to a shape.
        assert!(skin.drawables().get_ninepatch("button.back").is_some());
        assert!(skin.drawables().get_ninepatch("edit.back").is_none());
        assert!(skin.drawables().get("edit.back").is_some());
    }

    #[test]
    fn from_xml_parses_full_manifest() {
        let xml = r##"
            <skin name="manifest_test" base="dark">
                <color token="selection" value="#8000FF"/>
                <dimension token="scrollbar.thickness" value="10"/>
                <typeface role="button" font="Test Sans" size="18"/>
                <drawable role="button.back" src="button.9.png"/>
                <drawable role="edit.back">
                    <selector><item><layer-list><item>
                        <shape type="rect"><solid color="@surface"/></shape>
                    </item></layer-list></item></selector>
                </drawable>
            </skin>
        "##;
        let skin = Skin::from_xml(xml).expect("manifest parses");
        assert_eq!(skin.name(), "manifest_test");
        // Dark base + per-token overrides.
        assert_eq!(skin.palette().color("selection"), 0xFF8000FF);
        assert_eq!(skin.palette().dimension("scrollbar.thickness"), 10.0);
        assert_eq!(
            skin.palette().color("background"),
            Palette::dark().color("background")
        );
        let tf = skin.palette().typeface("button");
        assert_eq!(tf.font_name, "Test Sans");
        assert_eq!(tf.font_size, Some(18.0));
        // `src` → 9-patch role; inline `<selector>` → shape role (via capture).
        assert!(skin.drawables().get_ninepatch("button.back").is_some());
        assert!(skin.drawables().get_ninepatch("edit.back").is_none());
        assert!(skin.drawables().get("edit.back").is_some());
    }

    #[test]
    fn from_xml_reports_errors() {
        assert!(Skin::from_xml(r#"<skin base="dark"/>"#).is_err()); // missing name
        assert!(Skin::from_xml(r#"<theme name="x"/>"#).is_err()); // wrong root
        assert!(Skin::from_xml(r#"<skin name="x" base="neon"/>"#).is_err()); // bad base
    }

    #[test]
    fn from_xml_src_drawable_body_does_not_leak() {
        // A `src` <drawable> with a body containing a <color>: without consuming
        // the body the inner <color> would leak to top level and apply. It must
        // not — the body is skipped.
        let skin = Skin::from_xml(
            r##"
            <skin name="t">
                <drawable role="button.back" src="button.9.png">
                    <color token="selection" value="#FF0000"/>
                </drawable>
            </skin>
        "##,
        )
        .unwrap();
        assert!(skin.drawables().get_ninepatch("button.back").is_some());
        assert_eq!(
            skin.palette().color("selection"),
            Palette::classic().color("selection")
        );
    }

    #[test]
    fn from_xml_bad_typeface_size_errors() {
        assert!(
            Skin::from_xml(r#"<skin name="t"><typeface role="button" size="18px"/></skin>"#)
                .is_err()
        );
        // A valid size still parses.
        let skin =
            Skin::from_xml(r#"<skin name="t"><typeface role="button" size="18"/></skin>"#).unwrap();
        assert_eq!(skin.palette().typeface("button").font_size, Some(18.0));
    }

    #[test]
    fn register_skin_rejects_reserved_names() {
        // Registering under a reserved name is ignored, so the built-in still
        // resolves (the override is NOT applied) and the sentinel can't be hijacked.
        register_skin(Skin::builder("light").color("selection", 0xFF010203).build());
        register_skin(Skin::builder("default").color("selection", 0xFF040506).build());
        assert_eq!(
            skin_by_name("light").unwrap().palette().color("selection"),
            Palette::classic().color("selection")
        );
        // "default" is not a resolvable name (it's the unnamed-window fallback).
        assert!(skin_by_name("default").is_none());
    }
}
