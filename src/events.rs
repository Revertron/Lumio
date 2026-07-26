use crate::input::{ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};
use crate::traits::View;
use crate::ui::UI;

#[allow(dead_code)]
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum EventType {
    Click,
    CheckedChanged,
    MouseDown,
    MouseMove,
    MouseUp,
    SelectionChanged,
    TextChanged,
    LeftIconClick,
    RightIconClick,
    FocusGained,
    FocusLost,
    HoverEnter,
    HoverExit,
    DoubleClick,
    KeyDown,
    KeyChar,
    MouseWheel,
    ContextMenu,
    ValueChanged,
    Expanded,
    Collapsed,
    TabClose,
}

/// Payload passed to every event listener. Variants are keyed by payload
/// shape, not by event type: `Click` and `TextChanged` carry `None` (read
/// the view for its text), `CheckedChanged` carries `Checked`, selection
/// events carry `Selected`, pointer events (`HoverEnter`, `DoubleClick`,
/// `ContextMenu`, `MouseMove`) carry `Position` in absolute window
/// coordinates and `MouseDown`/`MouseUp` carry the same plus the button as
/// `Mouse`, `KeyDown` carries `Key`, `KeyChar` carries
/// `Char`, `MouseWheel` carries `Wheel`, `ValueChanged` (Slider) carries
/// the new numeric `Value`, `TabClose` (TabView) carries `Selected` with the
/// tab index, and `Expanded`/`Collapsed` (TreeView) carry `Selected` with the
/// visible-row index (read the node key via `TreeView::expanded_key()`).
#[derive(Clone, Debug, PartialEq)]
pub enum EventData {
    None,
    Checked(bool),
    Selected(usize),
    Value(f32),
    Position { x: i32, y: i32 },
    /// A button press/release: the pointer in absolute window coordinates plus
    /// which button it was.
    Mouse { x: i32, y: i32, button: MouseButton },
    Key { code: Option<VirtualKeyCode>, modifiers: ModifiersState },
    /// A character produced by the keyboard layout (dead keys and IME already
    /// applied) — what `Key` cannot give, since a virtual key code says nothing
    /// about the active layout.
    Char { ch: char, modifiers: ModifiersState },
    /// A wheel event: the pointer in absolute window coordinates plus the raw
    /// scroll distance, passed through unconverted so a listener can tell
    /// line-wise wheels from pixel-wise touchpads.
    Wheel { x: i32, y: i32, distance: MouseScrollDistance },
}

/// The universal listener type registered via `View::on_event`.
/// The dispatcher may hold the firing element's immutable `borrow()` while
/// the handler runs — handlers must NOT `borrow_mut` the firing view; they
/// mutate it through the `&dyn View` argument and `&self` setters.
pub type EventCallback = Box<dyn FnMut(&mut UI, &dyn View, &EventData) -> bool>;
