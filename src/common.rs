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
        let position: usize = rng.gen::<usize>() % chars.len();
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