use std::collections::HashMap;
use super::selector::StateSelector;
use super::parser::DrawableParser;

/// Registry of embedded drawables loaded via include_str!()
pub struct DrawableRegistry {
    selectors: HashMap<String, StateSelector>,
}

impl DrawableRegistry {
    /// Create a new registry and load all embedded drawables
    pub fn new() -> Self {
        let mut registry = DrawableRegistry {
            selectors: HashMap::new(),
        };

        registry.load_embedded_drawables();
        registry
    }

    /// Load all embedded drawable XML files
    fn load_embedded_drawables(&mut self) {
        println!("Loading drawables");
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
    }

    /// Load a single drawable from XML string
    fn load_drawable(&mut self, name: &str, xml: &str) {
        match DrawableParser::parse_selector(xml) {
            Ok(selector) => {
                self.selectors.insert(name.to_string(), selector);
            }
            Err(e) => {
                eprintln!("Failed to load drawable '{}': {}", name, e);
            }
        }
    }

    /// Get a drawable selector by name
    pub fn get(&self, name: &str) -> Option<&StateSelector> {
        self.selectors.get(name)
    }

    /// Check if a drawable exists
    pub fn contains(&self, name: &str) -> bool {
        self.selectors.contains_key(name)
    }

    /// Get list of all drawable names
    pub fn list_drawables(&self) -> Vec<String> {
        self.selectors.keys().cloned().collect()
    }
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
}
