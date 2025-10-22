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
        // Classic theme drawables
        self.load_drawable("button_classic", include_str!("../../../res/drawables/button_classic.xml"));
        self.load_drawable("button_classic_back", include_str!("../../../res/drawables/button_classic_back.xml"));
        self.load_drawable("button_classic_body", include_str!("../../../res/drawables/button_classic_body.xml"));
        self.load_drawable("edit_field_classic_back", include_str!("../../../res/drawables/edit_field_classic_back.xml"));
        self.load_drawable("edit_field_classic_body", include_str!("../../../res/drawables/edit_field_classic_body.xml"));
        self.load_drawable("edit_caret_classic", include_str!("../../../res/drawables/edit_caret_classic.xml"));
        self.load_drawable("checkbox_classic", include_str!("../../../res/drawables/checkbox_classic.xml"));
        self.load_drawable("panel_classic", include_str!("../../../res/drawables/panel_classic.xml"));

        // Future: Add more themes here
        // self.load_drawable("button_modern", include_str!("../../../res/drawables/button_modern.xml"));
        // self.load_drawable("button_material", include_str!("../../../res/drawables/button_material.xml"));
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

        // Check that all classic drawables are loaded
        assert!(registry.contains("button_classic"));
        assert!(registry.contains("edit_field_classic"));
        assert!(registry.contains("checkbox_classic"));
        assert!(registry.contains("panel_classic"));
    }

    #[test]
    fn test_registry_get_drawable() {
        let registry = DrawableRegistry::new();

        let button = registry.get("button_classic");
        assert!(button.is_some());

        let nonexistent = registry.get("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_list_drawables() {
        let registry = DrawableRegistry::new();

        let drawables = registry.list_drawables();
        assert!(drawables.len() >= 4);
        assert!(drawables.contains(&"button_classic".to_string()));
    }
}
