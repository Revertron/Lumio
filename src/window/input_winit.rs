//! winit 0.30 → Lumio input conversions (the winit boundary, analog of
//! `src/input/from_speedy2d.rs`). The `KeyEvent → VirtualKeyCode` table mirrors
//! the vendored speedy2d's mapping verbatim; the variant names match because
//! `crate::input::VirtualKeyCode` mirrors speedy2d's enum.

use winit::event::{KeyEvent, MouseButton as WMouseButton, MouseScrollDelta};
use winit::keyboard::{Key, KeyLocation, ModifiersState as WModifiersState, NamedKey};
use winit::window::CursorIcon;

use crate::input::{ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance, VirtualKeyCode};

impl From<WMouseButton> for MouseButton {
    fn from(b: WMouseButton) -> Self {
        match b {
            WMouseButton::Left => MouseButton::Left,
            WMouseButton::Right => MouseButton::Right,
            WMouseButton::Middle => MouseButton::Middle,
            WMouseButton::Back => MouseButton::Back,
            WMouseButton::Forward => MouseButton::Forward,
            WMouseButton::Other(n) => MouseButton::Other(n),
        }
    }
}

impl From<MouseScrollDelta> for MouseScrollDistance {
    fn from(d: MouseScrollDelta) -> Self {
        match d {
            MouseScrollDelta::LineDelta(x, y) => MouseScrollDistance::Lines { x: x as f64, y: y as f64, z: 0.0 },
            MouseScrollDelta::PixelDelta(p) => MouseScrollDistance::Pixels { x: p.x, y: p.y, z: 0.0 },
        }
    }
}

impl From<WModifiersState> for ModifiersState {
    fn from(s: WModifiersState) -> Self {
        ModifiersState::new(s.control_key(), s.alt_key(), s.shift_key(), s.super_key())
    }
}

/// Map a winit cursor request to winit's `CursorIcon`.
pub(crate) fn to_cursor_icon(c: MouseCursorType) -> CursorIcon {
    match c {
        MouseCursorType::Default => CursorIcon::Default,
        MouseCursorType::Pointer => CursorIcon::Pointer,
        MouseCursorType::Crosshair => CursorIcon::Crosshair,
        MouseCursorType::Text => CursorIcon::Text,
        MouseCursorType::VerticalText => CursorIcon::VerticalText,
        MouseCursorType::Move => CursorIcon::Move,
        MouseCursorType::Grab => CursorIcon::Grab,
        MouseCursorType::Grabbing => CursorIcon::Grabbing,
        MouseCursorType::Wait => CursorIcon::Wait,
        MouseCursorType::Progress => CursorIcon::Progress,
        MouseCursorType::Cell => CursorIcon::Cell,
        MouseCursorType::Alias => CursorIcon::Alias,
        MouseCursorType::Copy => CursorIcon::Copy,
        MouseCursorType::NoDrop => CursorIcon::NoDrop,
        MouseCursorType::NotAllowed => CursorIcon::NotAllowed,
        MouseCursorType::ColResize => CursorIcon::ColResize,
        MouseCursorType::RowResize => CursorIcon::RowResize,
        MouseCursorType::EwResize => CursorIcon::EwResize,
        MouseCursorType::NsResize => CursorIcon::NsResize,
        MouseCursorType::NeswResize => CursorIcon::NeswResize,
        MouseCursorType::NwseResize => CursorIcon::NwseResize,
        MouseCursorType::ZoomIn => CursorIcon::ZoomIn,
        MouseCursorType::ZoomOut => CursorIcon::ZoomOut,
    }
}

/// Map a winit `KeyEvent` to a Lumio `VirtualKeyCode`, or `None` for keys Lumio
/// does not mirror (dead/unidentified). Mirrors speedy2d's table.
pub(crate) fn key_event_to_vk(event: &KeyEvent) -> Option<VirtualKeyCode> {
    use VirtualKeyCode as K;
    let lr = |left: K, right: K| match event.location {
        KeyLocation::Standard | KeyLocation::Left => left,
        KeyLocation::Right | KeyLocation::Numpad => right,
    };
    let numpad = |normal: K, numpad: K| match event.location {
        KeyLocation::Standard | KeyLocation::Left | KeyLocation::Right => normal,
        KeyLocation::Numpad => numpad,
    };

    Some(match event.logical_key {
        Key::Named(named) => match named {
            NamedKey::Alt => lr(K::LAlt, K::RAlt),
            NamedKey::AltGraph => K::RAlt,
            NamedKey::ArrowDown => K::Down,
            NamedKey::ArrowLeft => K::Left,
            NamedKey::ArrowRight => K::Right,
            NamedKey::ArrowUp => K::Up,
            NamedKey::AudioVolumeDown => K::VolumeDown,
            NamedKey::AudioVolumeMute => K::Mute,
            NamedKey::AudioVolumeUp => K::VolumeUp,
            NamedKey::Backspace => K::Backspace,
            NamedKey::BrowserBack => K::WebBack,
            NamedKey::BrowserFavorites => K::WebFavorites,
            NamedKey::BrowserForward => K::WebForward,
            NamedKey::BrowserHome => K::WebHome,
            NamedKey::BrowserRefresh => K::WebRefresh,
            NamedKey::BrowserSearch => K::WebSearch,
            NamedKey::BrowserStop => K::WebStop,
            NamedKey::Compose => K::Compose,
            NamedKey::Control => lr(K::LControl, K::RControl),
            NamedKey::Convert => K::Convert,
            NamedKey::Copy => K::Copy,
            NamedKey::Cut => K::Cut,
            NamedKey::Delete => K::Delete,
            NamedKey::End => K::End,
            NamedKey::Enter => numpad(K::Return, K::NumpadEnter),
            NamedKey::Escape => K::Escape,
            NamedKey::F1 => K::F1,
            NamedKey::F2 => K::F2,
            NamedKey::F3 => K::F3,
            NamedKey::F4 => K::F4,
            NamedKey::F5 => K::F5,
            NamedKey::F6 => K::F6,
            NamedKey::F7 => K::F7,
            NamedKey::F8 => K::F8,
            NamedKey::F9 => K::F9,
            NamedKey::F10 => K::F10,
            NamedKey::F11 => K::F11,
            NamedKey::F12 => K::F12,
            NamedKey::F13 => K::F13,
            NamedKey::F14 => K::F14,
            NamedKey::F15 => K::F15,
            NamedKey::F16 => K::F16,
            NamedKey::F17 => K::F17,
            NamedKey::F18 => K::F18,
            NamedKey::F19 => K::F19,
            NamedKey::F20 => K::F20,
            NamedKey::F21 => K::F21,
            NamedKey::F22 => K::F22,
            NamedKey::F23 => K::F23,
            NamedKey::F24 => K::F24,
            NamedKey::GoBack => K::NavigateBackward,
            NamedKey::GoHome => K::Home,
            NamedKey::Home => K::Home,
            NamedKey::Insert => K::Insert,
            NamedKey::KanaMode => K::Kana,
            NamedKey::KanjiMode => K::Kanji,
            NamedKey::LaunchMail => K::Mail,
            NamedKey::MediaPlayPause => K::PlayPause,
            NamedKey::MediaStop => K::MediaStop,
            NamedKey::NavigatePrevious => K::NavigateBackward,
            NamedKey::NonConvert => K::NoConvert,
            NamedKey::NumLock => K::Numlock,
            NamedKey::PageDown => K::PageDown,
            NamedKey::PageUp => K::PageUp,
            NamedKey::Paste => K::Paste,
            NamedKey::Power => K::Power,
            NamedKey::PrintScreen => K::PrintScreen,
            NamedKey::ScrollLock => K::ScrollLock,
            NamedKey::Shift => lr(K::LShift, K::RShift),
            NamedKey::Space => K::Space,
            NamedKey::Tab => K::Tab,
            NamedKey::Super => lr(K::LWin, K::RWin),
            _ => return None,
        },
        Key::Character(ref c) => match c.chars().next().unwrap_or('\0') {
            'A' | 'a' => K::A,
            'B' | 'b' => K::B,
            'C' | 'c' => K::C,
            'D' | 'd' => K::D,
            'E' | 'e' => K::E,
            'F' | 'f' => K::F,
            'G' | 'g' => K::G,
            'H' | 'h' => K::H,
            'I' | 'i' => K::I,
            'J' | 'j' => K::J,
            'K' | 'k' => K::K,
            'L' | 'l' => K::L,
            'M' | 'm' => K::M,
            'N' | 'n' => K::N,
            'O' | 'o' => K::O,
            'P' | 'p' => K::P,
            'Q' | 'q' => K::Q,
            'R' | 'r' => K::R,
            'S' | 's' => K::S,
            'T' | 't' => K::T,
            'U' | 'u' => K::U,
            'V' | 'v' => K::V,
            'W' | 'w' => K::W,
            'X' | 'x' => K::X,
            'Y' | 'y' => K::Y,
            'Z' | 'z' => K::Z,
            '0' => numpad(K::Key0, K::Numpad0),
            '1' => numpad(K::Key1, K::Numpad1),
            '2' => numpad(K::Key2, K::Numpad2),
            '3' => numpad(K::Key3, K::Numpad3),
            '4' => numpad(K::Key4, K::Numpad4),
            '5' => numpad(K::Key5, K::Numpad5),
            '6' => numpad(K::Key6, K::Numpad6),
            '7' => numpad(K::Key7, K::Numpad7),
            '8' => numpad(K::Key8, K::Numpad8),
            '9' => numpad(K::Key9, K::Numpad9),
            '+' => numpad(K::Plus, K::NumpadAdd),
            '-' => numpad(K::Minus, K::NumpadSubtract),
            '*' => numpad(K::Asterisk, K::NumpadMultiply),
            '/' => numpad(K::Slash, K::NumpadDivide),
            ',' => numpad(K::Comma, K::NumpadComma),
            '.' => numpad(K::Period, K::NumpadDecimal),
            '=' => numpad(K::Equals, K::NumpadEquals),
            '^' => K::Caret,
            '\'' => K::Apostrophe,
            '\\' => K::Backslash,
            ':' => K::Colon,
            '`' => K::Grave,
            '(' => K::LBracket,
            ')' => K::RBracket,
            '\t' => K::Tab,
            ' ' => K::Space,
            _ => return None,
        },
        Key::Unidentified(_) | Key::Dead(_) => return None,
    })
}
