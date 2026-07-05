//! Thin clipboard wrapper. Real (arboard) under the `clipboard` feature; a no-op
//! stub otherwise (e.g. the headless `software` build), so the text widgets compile
//! and run without a native clipboard dependency. Public so applications can put
//! text on the clipboard themselves (e.g. "Copy message" context-menu actions).

/// Put `text` on the system clipboard. A no-op when the `clipboard` feature is off.
#[cfg(feature = "clipboard")]
pub fn set_text(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_owned());
    }
}

/// Read UTF-8 text from the system clipboard, or `None`. Always `None` when the
/// `clipboard` feature is off.
#[cfg(feature = "clipboard")]
pub fn get_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Read an image from the system clipboard as `(width, height, RGBA bytes)`.
/// `None` when the clipboard holds no image or the feature is off.
#[cfg(feature = "clipboard")]
pub fn get_image() -> Option<(u32, u32, Vec<u8>)> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let img = cb.get_image().ok()?;
    Some((img.width as u32, img.height as u32, img.bytes.into_owned()))
}

#[cfg(not(feature = "clipboard"))]
pub fn set_text(_text: &str) {}

#[cfg(not(feature = "clipboard"))]
pub fn get_text() -> Option<String> {
    None
}

#[cfg(not(feature = "clipboard"))]
pub fn get_image() -> Option<(u32, u32, Vec<u8>)> {
    None
}
