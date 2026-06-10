use rand::Rng;

pub const DEFAULT_TEXT_SIZE: f32 = 24_f32;

/// Gets current UNIX timestamp in UTC
#[allow(unused)]
pub fn get_utc_time() -> u128 {
    let sys_time = std::time::SystemTime::now();
    let elapsed = sys_time.duration_since(std::time::UNIX_EPOCH).unwrap();
    elapsed.as_millis()
}

/// Generates random string of given length
pub fn random_string(length: usize) -> String {
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!?".chars().collect();
    let mut rng = rand::thread_rng();
    let mut result = String::with_capacity(length);
    for _ in 0..length {
        let position: usize = rng.r#gen::<usize>() % chars.len();
        let c: char = *chars.get(position).unwrap();
        result.push(c);
    }
    result
}

/// Inserts a character into given position of String, taking into account char boundaries
pub fn insert_char(text: &str, pos: usize, ch: char) -> String {
    if pos > text.len() {
        panic!("Pos {} is higher then string length!", pos);
    }
    let mut part1 = text.chars().take(pos).collect::<String>();
    let part2 = text.chars().skip(pos).collect::<String>();
    part1.push(ch);
    part1.push_str(&part2);
    part1
}

/// Deletes a range of characters from char-index `start` to `end` (exclusive)
pub fn delete_range(text: &str, start: usize, end: usize) -> String {
    let mut result = text.chars().take(start).collect::<String>();
    let rest = text.chars().skip(end).collect::<String>();
    result.push_str(&rest);
    result
}

/// Inserts a string at the given char position
pub fn insert_str(text: &str, pos: usize, s: &str) -> String {
    let mut result = text.chars().take(pos).collect::<String>();
    result.push_str(s);
    let rest = text.chars().skip(pos).collect::<String>();
    result.push_str(&rest);
    result
}

/// Deletes one character from the string given it's position
pub fn delete_char(text: &str, pos: usize) -> String {
    if pos > text.len() {
        panic!("Pos {} is higher then string length!", pos);
    }
    if pos == 0 {
        return text.chars().skip(1).collect::<String>()
    }
    let mut part1 = text.chars().take(pos).collect::<String>();
    let part2 = text.chars().skip(pos + 1).collect::<String>();
    part1.push_str(&part2);
    part1
}
/// Maximum number of undo entries kept per text field.
pub(crate) const UNDO_LIMIT: usize = 100;

/// One undo/redo entry of a text field: the full state before a mutation.
#[derive(Clone, PartialEq)]
pub(crate) struct TextSnapshot {
    pub text: String,
    pub caret: usize,
    pub anchor: Option<usize>,
}

/// Kind of mutating operation, used to coalesce runs: consecutive typing
/// (or consecutive deleting) collapses into a single undo entry.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TextEditOp {
    Typing,
    Deleting,
    Other,
}
