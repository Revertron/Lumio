//! Thin clipboard wrapper. Real (arboard) under the `clipboard` feature; a no-op
//! stub otherwise (e.g. the headless `software` build), so the text widgets compile
//! and run without a native clipboard dependency.

/// Put `text` on the system clipboard. A no-op when the `clipboard` feature is off.
#[cfg(feature = "clipboard")]
pub(crate) fn set_text(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_owned());
    }
}

/// Read UTF-8 text from the system clipboard, or `None`. Always `None` when the
/// `clipboard` feature is off.
#[cfg(feature = "clipboard")]
pub(crate) fn get_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

#[cfg(not(feature = "clipboard"))]
pub(crate) fn set_text(_text: &str) {}

#[cfg(not(feature = "clipboard"))]
pub(crate) fn get_text() -> Option<String> {
    None
}
