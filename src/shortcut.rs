use std::fmt::{Display, Formatter};
use std::str::FromStr;

use crate::input::{ModifiersState, VirtualKeyCode};

/// A keyboard accelerator: a key plus modifier state, usable as a `HashMap`
/// key in the [`crate::ui::UI`] shortcut registry. Parse one from a string
/// like `"Ctrl+Shift+S"`, `"F5"` or `"Alt+Enter"` (case-insensitive), or
/// construct it directly. `Display` round-trips the canonical form
/// (`Ctrl+Shift+Alt+Key`), so the same syntax can later serve as menu-item
/// accelerator text. The logo/meta key is deliberately not matched.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: VirtualKeyCode,
}

impl Shortcut {
    pub fn new(ctrl: bool, shift: bool, alt: bool, key: VirtualKeyCode) -> Self {
        Self { ctrl, shift, alt, key }
    }

    /// The shortcut matching `key` pressed under the given modifier state.
    pub fn from_state(key: VirtualKeyCode, modifiers: &ModifiersState) -> Self {
        Self {
            ctrl: modifiers.ctrl(),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
            key,
        }
    }
}

impl FromStr for Shortcut {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key = None;
        for token in s.split('+') {
            let token = token.trim();
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" => alt = true,
                lower => {
                    if key.is_some() {
                        return Err(format!("More than one key in shortcut '{}'", s));
                    }
                    key = Some(parse_key(lower).ok_or_else(|| format!("Unknown key '{}' in shortcut '{}'", token, s))?);
                }
            }
        }
        match key {
            Some(key) => Ok(Shortcut { ctrl, shift, alt, key }),
            None => Err(format!("No key in shortcut '{}'", s)),
        }
    }
}

impl Display for Shortcut {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.ctrl {
            write!(f, "Ctrl+")?;
        }
        if self.shift {
            write!(f, "Shift+")?;
        }
        if self.alt {
            write!(f, "Alt+")?;
        }
        write!(f, "{}", key_name(self.key))
    }
}

/// Parses a single (already lowercased) key token.
fn parse_key(token: &str) -> Option<VirtualKeyCode> {
    use VirtualKeyCode::*;
    let key = match token {
        "a" => A, "b" => B, "c" => C, "d" => D, "e" => E, "f" => F,
        "g" => G, "h" => H, "i" => I, "j" => J, "k" => K, "l" => L,
        "m" => M, "n" => N, "o" => O, "p" => P, "q" => Q, "r" => R,
        "s" => S, "t" => T, "u" => U, "v" => V, "w" => W, "x" => X,
        "y" => Y, "z" => Z,
        "0" => Key0, "1" => Key1, "2" => Key2, "3" => Key3, "4" => Key4,
        "5" => Key5, "6" => Key6, "7" => Key7, "8" => Key8, "9" => Key9,
        "f1" => F1, "f2" => F2, "f3" => F3, "f4" => F4, "f5" => F5, "f6" => F6,
        "f7" => F7, "f8" => F8, "f9" => F9, "f10" => F10, "f11" => F11, "f12" => F12,
        "enter" | "return" => Return,
        "esc" | "escape" => Escape,
        "tab" => Tab,
        "space" => Space,
        "backspace" => Backspace,
        "delete" | "del" => Delete,
        "insert" => Insert,
        "home" => Home,
        "end" => End,
        "pageup" => PageUp,
        "pagedown" => PageDown,
        "up" => Up,
        "down" => Down,
        "left" => Left,
        "right" => Right,
        "minus" => Minus,
        "plus" => Plus,
        "comma" => Comma,
        "period" => Period,
        _ => return None,
    };
    Some(key)
}

/// Canonical display name of a key; inverse of `parse_key`.
pub fn key_name(key: VirtualKeyCode) -> &'static str {
    use VirtualKeyCode::*;
    match key {
        A => "A", B => "B", C => "C", D => "D", E => "E", F => "F",
        G => "G", H => "H", I => "I", J => "J", K => "K", L => "L",
        M => "M", N => "N", O => "O", P => "P", Q => "Q", R => "R",
        S => "S", T => "T", U => "U", V => "V", W => "W", X => "X",
        Y => "Y", Z => "Z",
        Key0 => "0", Key1 => "1", Key2 => "2", Key3 => "3", Key4 => "4",
        Key5 => "5", Key6 => "6", Key7 => "7", Key8 => "8", Key9 => "9",
        F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5", F6 => "F6",
        F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10", F11 => "F11", F12 => "F12",
        Return => "Enter",
        Escape => "Esc",
        Tab => "Tab",
        Space => "Space",
        Backspace => "Backspace",
        Delete => "Delete",
        Insert => "Insert",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Up => "Up",
        Down => "Down",
        Left => "Left",
        Right => "Right",
        Minus => "Minus",
        Plus => "Plus",
        Comma => "Comma",
        Period => "Period",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_key() {
        let s: Shortcut = "Ctrl+Shift+S".parse().unwrap();
        assert_eq!(s, Shortcut::new(true, true, false, VirtualKeyCode::S));
        let s: Shortcut = "alt+enter".parse().unwrap();
        assert_eq!(s, Shortcut::new(false, false, true, VirtualKeyCode::Return));
        let s: Shortcut = "F5".parse().unwrap();
        assert_eq!(s, Shortcut::new(false, false, false, VirtualKeyCode::F5));
        let s: Shortcut = " Control + Del ".parse().unwrap();
        assert_eq!(s, Shortcut::new(true, false, false, VirtualKeyCode::Delete));
    }

    #[test]
    fn rejects_bad_input() {
        assert!("Ctrl+".parse::<Shortcut>().is_err());
        assert!("Ctrl+Shift".parse::<Shortcut>().is_err());
        assert!("Ctrl+Foo".parse::<Shortcut>().is_err());
        assert!("Ctrl+S+D".parse::<Shortcut>().is_err());
        assert!("".parse::<Shortcut>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for accel in ["Ctrl+Shift+S", "Alt+Enter", "F5", "Ctrl+Alt+Delete", "Shift+Tab"] {
            let s: Shortcut = accel.parse().unwrap();
            assert_eq!(s.to_string(), accel);
            let again: Shortcut = s.to_string().parse().unwrap();
            assert_eq!(s, again);
        }
    }
}
