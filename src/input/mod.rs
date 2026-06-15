//! Backend-neutral input/event types.
//!
//! The `View` trait, the `UI` dispatch layer, `events.rs` and `shortcut.rs`
//! depend only on the types here. They mirror the corresponding
//! `speedy2d::window` types (same variant and method names) so view-side
//! handler bodies, `match` arms and shortcut parsing read identically
//! regardless of the windowing backend.
//!
//! Mouse positions are not defined here — they use the existing
//! [`crate::types::Point<i32>`].
//!
//! Both backends now run on winit; the winit→Lumio conversions live at the
//! window-loop boundary in [`crate::window`] (`window/input_winit.rs`).

/// A platform-specific opaque key scancode. Never inspected by Lumio; passed
/// through from the window backend to views. Mirrors `speedy2d`'s alias.
pub type KeyScancode = u32;

/// Identifies a mouse button. Mirrors `speedy2d::window::MouseButton`.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
    Other(u16),
}

/// A difference in the mouse scroll wheel position. Mirrors
/// `speedy2d::window::MouseScrollDistance` (distances are `f64`; `y` is the
/// typical vertical wheel axis).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MouseScrollDistance {
    Lines { x: f64, y: f64, z: f64 },
    Pixels { x: f64, y: f64, z: f64 },
    Pages { x: f64, y: f64, z: f64 },
}

/// The shape of the mouse cursor displayed over the window. Mirrors
/// `speedy2d::window::MouseCursorType`.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub enum MouseCursorType {
    #[default]
    Default,
    Pointer,
    Crosshair,
    Text,
    VerticalText,
    Move,
    Grab,
    Grabbing,
    Wait,
    Progress,
    Cell,
    Alias,
    Copy,
    NoDrop,
    NotAllowed,
    ColResize,
    RowResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ZoomIn,
    ZoomOut,
}

/// Decide whether a cursor change should be pushed to the OS. Returns
/// `Some(current)` (and records it in `last`) only when it differs from the last
/// cursor pushed, else `None`. Both window backends call this so the
/// apply-on-transition guard isn't duplicated; each then applies the returned
/// cursor with its own windowing API (avoids per-event `set_cursor` churn).
pub fn cursor_transition(
    current: MouseCursorType,
    last: &mut Option<MouseCursorType>,
) -> Option<MouseCursorType> {
    if *last == Some(current) {
        None
    } else {
        *last = Some(current);
        Some(current)
    }
}

/// The state of the modifier keys. Mirrors `speedy2d::window::ModifiersState`.
#[derive(Debug, Hash, PartialEq, Eq, Clone, Default)]
pub struct ModifiersState {
    ctrl: bool,
    alt: bool,
    shift: bool,
    logo: bool,
}

impl ModifiersState {
    /// Construct a modifier state from the four flags. Useful for synthetic
    /// dispatch in tests and for the backend conversion layer.
    #[inline]
    pub const fn new(ctrl: bool, alt: bool, shift: bool, logo: bool) -> Self {
        ModifiersState { ctrl, alt, shift, logo }
    }
    /// True if CTRL is pressed.
    #[inline]
    pub fn ctrl(&self) -> bool {
        self.ctrl
    }
    /// True if ALT is pressed.
    #[inline]
    pub fn alt(&self) -> bool {
        self.alt
    }
    /// True if SHIFT is pressed.
    #[inline]
    pub fn shift(&self) -> bool {
        self.shift
    }
    /// True if the logo key (normally the Windows key) is pressed.
    #[inline]
    pub fn logo(&self) -> bool {
        self.logo
    }
}

/// Defines [`VirtualKeyCode`], whose variants mirror `speedy2d::window::VirtualKeyCode`
/// verbatim (the names also line up with the winit `KeyEvent` mapping in
/// `window/input_winit.rs`). A macro keeps the variant list a single source of truth.
macro_rules! virtual_key_codes {
    ($($variant:ident),+ $(,)?) => {
        /// A virtual key code. Variant names mirror `speedy2d::window::VirtualKeyCode`.
        #[allow(missing_docs)]
        #[derive(Debug, Hash, Ord, PartialOrd, PartialEq, Eq, Clone, Copy)]
        pub enum VirtualKeyCode {
            $($variant),+
        }
    };
}

virtual_key_codes!(
    Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9, Key0,
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Escape,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24,
    PrintScreen, ScrollLock, PauseBreak,
    Insert, Home, Delete, End, PageDown, PageUp,
    Left, Up, Right, Down,
    Backspace, Return, Space,
    Compose,
    Caret,
    Numlock,
    Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
    NumpadAdd, NumpadDivide, NumpadDecimal, NumpadComma, NumpadEnter, NumpadEquals,
    NumpadMultiply, NumpadSubtract,
    AbntC1, AbntC2, Apostrophe, Apps, Asterisk, At, Ax, Backslash, Calculator, Capital,
    Colon, Comma, Convert, Equals, Grave, Kana, Kanji, LAlt, LBracket, LControl, LShift, LWin,
    Mail, MediaSelect, MediaStop, Minus, Mute, MyComputer, NavigateForward, NavigateBackward,
    NextTrack, NoConvert, OEM102, Period, PlayPause, Plus, Power, PrevTrack,
    RAlt, RBracket, RControl, RShift, RWin,
    Semicolon, Slash, Sleep, Stop, Sysrq, Tab, Underline, Unlabeled, VolumeDown, VolumeUp,
    Wake, WebBack, WebFavorites, WebForward, WebHome, WebRefresh, WebSearch, WebStop, Yen,
    Copy, Paste, Cut,
);
