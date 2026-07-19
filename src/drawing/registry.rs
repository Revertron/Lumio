use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use log::{debug, error};
use super::selector::StateSelector;
use super::parser::DrawableParser;
use crate::ninepatch::NinePatchBackground;

/// A set of drawables keyed by role name (e.g. `"button.back"`). A registry may
/// carry a `base`: roles it doesn't define itself resolve against the base, so a
/// skin overrides only the roles it changes (see [`with_base`](Self::with_base)).
pub struct DrawableRegistry {
    selectors: HashMap<String, StateSelector>,
    /// Roles painted with a 9-patch instead of a shape drawable (skin overrides
    /// only; the base set is all shapes). `RefCell` because a 9-patch caches its
    /// rasterization on paint, yet the registry is shared behind `&`.
    ninepatch_roles: HashMap<String, RefCell<NinePatchBackground>>,
    /// Fallback for roles not present here; `None` for the base set.
    base: Option<Rc<DrawableRegistry>>,
}

impl DrawableRegistry {
    /// Create the base registry and load all embedded (classic) drawables.
    pub fn new() -> Self {
        let mut registry = DrawableRegistry {
            selectors: HashMap::new(),
            ninepatch_roles: HashMap::new(),
            base: None,
        };

        registry.load_embedded_drawables();
        registry
    }

    /// An overlay registry whose misses fall back to `base`. Skins with form
    /// overrides use this: only the overridden roles live here, everything else
    /// resolves against the shared base set.
    pub(crate) fn with_base(base: Rc<DrawableRegistry>) -> Self {
        DrawableRegistry {
            selectors: HashMap::new(),
            ninepatch_roles: HashMap::new(),
            base: Some(base),
        }
    }

    /// Override `role` with a 9-patch. Wins over any shape drawable for the same
    /// role at paint time (see the theme's `draw_component`).
    pub(crate) fn insert_ninepatch(&mut self, role: &str, ninepatch: NinePatchBackground) {
        self.ninepatch_roles.insert(role.to_string(), RefCell::new(ninepatch));
    }

    /// The 9-patch overriding `role`, if any (this registry, then its base).
    pub(crate) fn get_ninepatch(&self, role: &str) -> Option<&RefCell<NinePatchBackground>> {
        // Skip hashing `role` when there are no 9-patch overrides here — the
        // common case (built-in skins and shape-only overlays have none), and
        // `draw_component` calls this for every widget every frame.
        if !self.ninepatch_roles.is_empty()
            && let Some(np) = self.ninepatch_roles.get(role)
        {
            return Some(np);
        }
        self.base.as_deref().and_then(|base| base.get_ninepatch(role))
    }

    /// Load all embedded drawable XML files
    fn load_embedded_drawables(&mut self) {
        debug!("Loading drawables");
        // Drawables are registered under role names ("button.back"); the XML
        // files hold the Classic theme's skin for each role.
        self.load_drawable("button", include_str!("../drawables/button_classic.xml"));
        self.load_drawable("button.back", include_str!("../drawables/button_classic_back.xml"));
        self.load_drawable("button.body", include_str!("../drawables/button_classic_body.xml"));
        self.load_drawable("edit.back", include_str!("../drawables/edit_field_classic_back.xml"));
        self.load_drawable("edit.body", include_str!("../drawables/edit_field_classic_body.xml"));
        self.load_drawable("edit.caret", include_str!("../drawables/edit_caret_classic.xml"));
        self.load_drawable("checkbox.box", include_str!("../drawables/checkbox_classic.xml"));
        self.load_drawable("panel", include_str!("../drawables/panel_classic.xml"));
        self.load_drawable("radio.back", include_str!("../drawables/radio_classic_back.xml"));
        self.load_drawable("radio.body", include_str!("../drawables/radio_classic_body.xml"));
        self.load_drawable("radio.indicator", include_str!("../drawables/radio_classic_indicator.xml"));
        self.load_drawable("checkbox.checkmark", include_str!("../drawables/checkbox_classic_checkmark.xml"));
        self.load_drawable("combo.arrow", include_str!("../drawables/combo_classic_arrow.xml"));
        self.load_drawable("combo.focus", include_str!("../drawables/combo_classic_focus.xml"));
        self.load_drawable("tab.active", include_str!("../drawables/tab_classic_active.xml"));
        self.load_drawable("tab.inactive", include_str!("../drawables/tab_classic_inactive.xml"));
        self.load_drawable("tab.content", include_str!("../drawables/tab_classic_content.xml"));
        self.load_drawable("separator.h", include_str!("../drawables/separator_classic_h.xml"));
        self.load_drawable("separator.v", include_str!("../drawables/separator_classic_v.xml"));
        self.load_drawable("panel.back", include_str!("../drawables/panel_classic_back.xml"));
        self.load_drawable("progress.fill", include_str!("../drawables/progress_classic_fill.xml"));
        self.load_drawable("scrollbar.track", include_str!("../drawables/scrollbar_classic_track.xml"));
        self.load_drawable("scrollbar.arrow.up", include_str!("../drawables/scrollbar_classic_arrow_up.xml"));
        self.load_drawable("scrollbar.arrow.down", include_str!("../drawables/scrollbar_classic_arrow_down.xml"));
        self.load_drawable("scrollbar.arrow.left", include_str!("../drawables/scrollbar_classic_arrow_left.xml"));
        self.load_drawable("scrollbar.arrow.right", include_str!("../drawables/scrollbar_classic_arrow_right.xml"));
        self.load_drawable("menu.arrow", include_str!("../drawables/menu_classic_arrow.xml"));
        self.load_drawable("popup.body", include_str!("../drawables/popup_classic_body.xml"));
    }

    /// Load a single drawable from XML string
    pub(crate) fn load_drawable(&mut self, name: &str, xml: &str) {
        match DrawableParser::parse_selector(xml) {
            Ok(selector) => {
                self.selectors.insert(name.to_string(), selector);
            }
            Err(e) => {
                error!("Failed to load drawable '{}': {}", name, e);
            }
        }
    }

    /// Get a drawable selector by role name, falling back to the base set.
    pub fn get(&self, name: &str) -> Option<&StateSelector> {
        match self.selectors.get(name) {
            Some(selector) => Some(selector),
            None => self.base.as_deref().and_then(|base| base.get(name)),
        }
    }

    /// Check if a drawable exists in this registry or its base.
    pub fn contains(&self, name: &str) -> bool {
        self.selectors.contains_key(name)
            || self.base.as_deref().is_some_and(|base| base.contains(name))
    }

    /// Get list of all drawable names, including those inherited from the base.
    pub fn list_drawables(&self) -> Vec<String> {
        let mut names: HashSet<String> = self.selectors.keys().cloned().collect();
        if let Some(base) = self.base.as_deref() {
            names.extend(base.list_drawables());
        }
        names.into_iter().collect()
    }
}

thread_local! {
    /// The shared base (classic) drawable set, built once per thread. Every
    /// plain skin uses it directly; skins with form overrides overlay on it.
    static BASE_DRAWABLES: Rc<DrawableRegistry> = Rc::new(DrawableRegistry::new());
}

/// The shared base (classic) drawable set for the current thread. Cloning the
/// returned `Rc` is cheap, so all skins share one parsed copy of the embedded
/// drawables.
pub(crate) fn base_drawables() -> Rc<DrawableRegistry> {
    BASE_DRAWABLES.with(Rc::clone)
}

impl Default for DrawableRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_loads_drawables() {
        let registry = DrawableRegistry::new();

        // Check that all role drawables are loaded
        assert!(registry.contains("button"));
        assert!(registry.contains("edit.back"));
        assert!(registry.contains("checkbox.box"));
        assert!(registry.contains("panel"));
    }

    #[test]
    fn test_registry_get_drawable() {
        let registry = DrawableRegistry::new();

        let button = registry.get("button");
        assert!(button.is_some());

        let nonexistent = registry.get("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_list_drawables() {
        let registry = DrawableRegistry::new();

        let drawables = registry.list_drawables();
        assert!(drawables.len() >= 4);
        assert!(drawables.contains(&"button".to_string()));
    }

    #[test]
    fn test_overlay_overrides_and_falls_back() {
        let base = Rc::new(DrawableRegistry::new());
        let mut overlay = DrawableRegistry::with_base(Rc::clone(&base));
        let xml = r#"<selector><item><layer-list><item>
            <shape type="rect"><solid color="@progress_fill"/></shape>
        </item></layer-list></item></selector>"#;
        // Override an existing base role, and add a role absent from the base.
        overlay.load_drawable("button.back", xml);
        overlay.load_drawable("custom.role", xml);

        // The overlay's own roles resolve here...
        assert!(overlay.get("button.back").is_some());
        assert!(overlay.get("custom.role").is_some());
        // ...a role only in the base resolves via fallback...
        assert!(overlay.get("panel").is_some());
        assert!(overlay.contains("checkbox.box"));
        // ...the custom role exists only in the overlay, not the base...
        assert!(base.get("custom.role").is_none());
        // ...and a role in neither is absent.
        assert!(overlay.get("no.such.role").is_none());
        // list_drawables unions overlay + base.
        let all = overlay.list_drawables();
        assert!(all.contains(&"custom.role".to_string()));
        assert!(all.contains(&"panel".to_string()));
    }

    #[test]
    fn test_ninepatch_override_resolves_and_falls_back() {
        let base = Rc::new(DrawableRegistry::new());
        let mut overlay = DrawableRegistry::with_base(Rc::clone(&base));
        // from_png is lazy — no asset load needed to register it.
        overlay.insert_ninepatch("button.back", NinePatchBackground::from_png("button.9.png"));

        // The 9-patch override resolves here...
        assert!(overlay.get_ninepatch("button.back").is_some());
        // ...roles without a 9-patch don't (base holds none)...
        assert!(overlay.get_ninepatch("panel").is_none());
        assert!(base.get_ninepatch("button.back").is_none());
        // ...and shape lookup still falls back to the base for other roles.
        assert!(overlay.get("panel").is_some());
    }
}
