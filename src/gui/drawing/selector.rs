use super::primitives::Drawable;
use crate::gui::themes::ViewState;

/// Android StateListDrawable-style selector
/// Maps view states to drawables
pub struct StateSelector {
    states: Vec<(StateMatcher, Drawable)>,
}

impl StateSelector {
    pub fn new() -> Self {
        StateSelector {
            states: Vec::new(),
        }
    }

    pub fn add_state(&mut self, matcher: StateMatcher, drawable: Drawable) {
        self.states.push((matcher, drawable));
    }

    /// Get the drawable that matches the current view state
    /// Returns the first matching state, or the default (last item with no conditions)
    pub fn get_drawable(&self, current_state: &ViewState) -> Option<&Drawable> {
        for (matcher, drawable) in &self.states {
            if matcher.matches(current_state) {
                return Some(drawable);
            }
        }

        // Return the last item as default if no specific match
        if let Some((_, drawable)) = self.states.last() {
            Some(drawable)
        } else {
            None
        }
    }
}

impl Default for StateSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// State matcher for selector items
/// Only defined fields are checked - undefined fields are wildcards
#[derive(Debug, Clone, Default)]
pub struct StateMatcher {
    pub enabled: Option<bool>,
    pub focusable: Option<bool>,
    pub focused: Option<bool>,
    pub hovered: Option<bool>,
    pub pressed: Option<bool>,
    pub checked: Option<bool>,
}

impl StateMatcher {
    pub fn new() -> Self {
        StateMatcher::default()
    }

    /// Check if this matcher matches the given state
    pub fn matches(&self, state: &ViewState) -> bool {
        if let Some(enabled) = self.enabled {
            if enabled != state.enabled {
                return false;
            }
        }

        if let Some(focusable) = self.focusable {
            if focusable != state.focusable {
                return false;
            }
        }

        if let Some(focused) = self.focused {
            if focused != state.focused {
                return false;
            }
        }

        if let Some(hovered) = self.hovered {
            if hovered != state.hovered {
                return false;
            }
        }

        if let Some(pressed) = self.pressed {
            if pressed != state.pressed {
                return false;
            }
        }

        if let Some(checked) = self.checked {
            if checked != state.checked {
                return false;
            }
        }

        true
    }
}

impl From<ViewState> for StateMatcher {
    fn from(state: ViewState) -> Self {
        StateMatcher {
            enabled: Some(state.enabled),
            focusable: Some(state.focusable),
            focused: Some(state.focused),
            hovered: Some(state.hovered),
            pressed: Some(state.pressed),
            checked: Some(state.checked),
        }
    }
}
