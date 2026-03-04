use std::collections::HashMap;
use super::super::themes::ViewState;

#[allow(unused)]
pub enum DrawState {
    Transparent,
    Color(u32),
    Image(String),
    Gradient(u32, u32, u32, u32)
}

#[allow(unused)]
pub enum Font {
    Color(u32)
}

/// A selector for element styles.
pub struct Selector<T> {
    states: HashMap<ViewState, T>
}

#[allow(unused)]
impl<T> Selector<T> {
    pub fn new() -> Self {
        Selector { states: HashMap::new() }
    }

    pub fn add_state(&mut self, state: ViewState, data: T) {
        self.states.insert(state, data);
    }

    pub fn get_state(&self, state: &ViewState) -> Option<&T> {
        self.states.get(state)
    }
}

pub type MainSelector = Selector<DrawState>;
pub type FontSelector = Selector<Font>;