#[allow(dead_code)]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum EventType {
    Click,
    CheckedChanged,
    MouseDown,
    MouseMove,
    MouseUp,
    SelectionChanged,
    TextChanged,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum EventData {
    Click,
    CheckedChanged { checked: bool },
    MouseDown { x: u32, y: u32 },
    MouseMove { x: u32, y: u32 },
    MouseUp { x: u32, y: u32 },
}