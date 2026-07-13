use crate::input::{ModifiersState, VirtualKeyCode};
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
    ContextMenu,
    ValueChanged,
    Expanded,
    Collapsed,
}

/// Payload passed to every event listener. Variants are keyed by payload
/// shape, not by event type: `Click` and `TextChanged` carry `None` (read
/// the view for its text), `CheckedChanged` carries `Checked`, selection
/// events carry `Selected`, pointer events (`HoverEnter`, `DoubleClick`,
/// `ContextMenu`, `MouseMove`) carry `Position` in absolute window
/// coordinates, `KeyDown` carries `Key`, `ValueChanged` (Slider) carries
/// the new numeric `Value`, and `Expanded`/`Collapsed` (TreeView) carry
/// `Selected` with the visible-row index (read the node key via
/// `TreeView::expanded_key()`).
#[derive(Clone, Debug, PartialEq)]
pub enum EventData {
    None,
    Checked(bool),
    Selected(usize),
    Value(f32),
    Position { x: i32, y: i32 },
    Key { code: Option<VirtualKeyCode>, modifiers: ModifiersState },
}

/// The universal listener type registered via `View::on_event`.
/// The dispatcher may hold the firing element's immutable `borrow()` while
/// the handler runs — handlers must NOT `borrow_mut` the firing view; they
/// mutate it through the `&dyn View` argument and `&self` setters.
pub type EventCallback = Box<dyn FnMut(&mut UI, &dyn View, &EventData) -> bool>;
