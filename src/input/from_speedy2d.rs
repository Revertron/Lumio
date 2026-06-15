//! Conversions between speedy2d's input types and Lumio's. This is the speedy2d
//! window-backend boundary: `win.rs` calls these `.into()` at each
//! `WindowHandler` callback (speedy2d→Lumio) and for `set_cursor`
//! (Lumio→speedy2d). When the window loop is later abstracted behind a feature,
//! this file moves behind that feature. (`VirtualKeyCode`'s conversion lives in
//! `mod.rs`, generated from the same variant list as the enum.)

use speedy2d::window as sp;

use super::{ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance};

impl From<sp::MouseButton> for MouseButton {
    fn from(b: sp::MouseButton) -> Self {
        match b {
            sp::MouseButton::Left => MouseButton::Left,
            sp::MouseButton::Middle => MouseButton::Middle,
            sp::MouseButton::Right => MouseButton::Right,
            sp::MouseButton::Back => MouseButton::Back,
            sp::MouseButton::Forward => MouseButton::Forward,
            sp::MouseButton::Other(n) => MouseButton::Other(n),
            // `sp::MouseButton` is #[non_exhaustive]; treat any future variant as Other.
            _ => MouseButton::Other(0),
        }
    }
}

impl From<sp::MouseScrollDistance> for MouseScrollDistance {
    fn from(d: sp::MouseScrollDistance) -> Self {
        match d {
            sp::MouseScrollDistance::Lines { x, y, z } => MouseScrollDistance::Lines { x, y, z },
            sp::MouseScrollDistance::Pixels { x, y, z } => MouseScrollDistance::Pixels { x, y, z },
            sp::MouseScrollDistance::Pages { x, y, z } => MouseScrollDistance::Pages { x, y, z },
        }
    }
}

impl From<sp::ModifiersState> for ModifiersState {
    fn from(m: sp::ModifiersState) -> Self {
        ModifiersState::new(m.ctrl(), m.alt(), m.shift(), m.logo())
    }
}

/// Outbound: Lumio cursor → speedy2d cursor, for `WindowHelper::set_cursor`.
impl From<MouseCursorType> for sp::MouseCursorType {
    fn from(c: MouseCursorType) -> Self {
        match c {
            MouseCursorType::Default => sp::MouseCursorType::Default,
            MouseCursorType::Pointer => sp::MouseCursorType::Pointer,
            MouseCursorType::Crosshair => sp::MouseCursorType::Crosshair,
            MouseCursorType::Text => sp::MouseCursorType::Text,
            MouseCursorType::VerticalText => sp::MouseCursorType::VerticalText,
            MouseCursorType::Move => sp::MouseCursorType::Move,
            MouseCursorType::Grab => sp::MouseCursorType::Grab,
            MouseCursorType::Grabbing => sp::MouseCursorType::Grabbing,
            MouseCursorType::Wait => sp::MouseCursorType::Wait,
            MouseCursorType::Progress => sp::MouseCursorType::Progress,
            MouseCursorType::Cell => sp::MouseCursorType::Cell,
            MouseCursorType::Alias => sp::MouseCursorType::Alias,
            MouseCursorType::Copy => sp::MouseCursorType::Copy,
            MouseCursorType::NoDrop => sp::MouseCursorType::NoDrop,
            MouseCursorType::NotAllowed => sp::MouseCursorType::NotAllowed,
            MouseCursorType::ColResize => sp::MouseCursorType::ColResize,
            MouseCursorType::RowResize => sp::MouseCursorType::RowResize,
            MouseCursorType::EwResize => sp::MouseCursorType::EwResize,
            MouseCursorType::NsResize => sp::MouseCursorType::NsResize,
            MouseCursorType::NeswResize => sp::MouseCursorType::NeswResize,
            MouseCursorType::NwseResize => sp::MouseCursorType::NwseResize,
            MouseCursorType::ZoomIn => sp::MouseCursorType::ZoomIn,
            MouseCursorType::ZoomOut => sp::MouseCursorType::ZoomOut,
        }
    }
}
