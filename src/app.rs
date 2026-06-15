//! Backend-neutral window launcher.
//!
//! [`run`] opens a window and drives the UI event loop using whichever
//! rendering backend is compiled in — GL (speedy2d) under `backend-gl`, or the
//! tiny-skia software renderer under `backend-software`. App code calls the
//! same `lumio::run(ui, config)` either way, so switching backends is a
//! Cargo-feature change with no source edits. [`WindowConfig`] is the neutral
//! superset of the window options both backends understand.

use crate::drawing::Palette;
use crate::ui::UI;

/// Backend-neutral window configuration passed to [`run`]. Build it fluently:
///
/// ```no_run
/// use lumio::prelude::*;
/// # fn demo(ui: UI) {
/// lumio::run(ui, WindowConfig::new("My App", 800, 520).center());
/// # }
/// ```
#[derive(Clone)]
pub struct WindowConfig {
    /// Window title.
    pub title: String,
    /// Initial width, in logical or physical pixels per [`logical_size`](Self::logical_size).
    pub width: u32,
    /// Initial height.
    pub height: u32,
    /// When `true`, `width`/`height` are logical (HiDPI-scaled) pixels; when
    /// `false` (the default) they are physical device pixels.
    pub logical_size: bool,
    /// Center the window on the primary monitor at creation (default `false`).
    pub center: bool,
    /// Whether the window is shown on creation (default `true`). Start hidden
    /// for tray apps that boot minimized.
    pub visible: bool,
    /// The window's close button hides it instead of closing the app (tray
    /// apps). Default `false`; only affects the main window.
    pub hide_on_close: bool,
    /// Whether the user can resize the window by dragging its edges (default
    /// `true`). A non-resizable window also can't be maximized on most platforms.
    pub resizable: bool,
    /// Whether the window shows an enabled minimize button (default `true`).
    pub minimizable: bool,
    /// Whether the window shows an enabled maximize button (default `true`).
    pub maximizable: bool,
    /// Palette the window starts with (default [`Palette::classic`]).
    pub palette: Palette,
}

impl WindowConfig {
    /// A config with the given title and initial size (physical pixels); every
    /// other option at its default.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        WindowConfig {
            title: title.into(),
            width,
            height,
            logical_size: false,
            center: false,
            visible: true,
            hide_on_close: false,
            resizable: true,
            minimizable: true,
            maximizable: true,
            palette: Palette::classic(),
        }
    }

    /// Interpret `width`/`height` as logical (HiDPI-scaled) pixels.
    pub fn logical_size(mut self) -> Self {
        self.logical_size = true;
        self
    }

    /// Center the window on the primary monitor at creation.
    pub fn center(mut self) -> Self {
        self.center = true;
        self
    }

    /// Set whether the window is visible on creation.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Make the close button hide the window instead of closing the app (tray
    /// apps). Only affects the main window.
    pub fn hide_on_close(mut self, value: bool) -> Self {
        self.hide_on_close = value;
        self
    }

    /// Set whether the user can resize the window. A non-resizable window also
    /// can't be maximized on most platforms.
    pub fn resizable(mut self, value: bool) -> Self {
        self.resizable = value;
        self
    }

    /// Set whether the window shows an enabled minimize button.
    pub fn minimizable(mut self, value: bool) -> Self {
        self.minimizable = value;
        self
    }

    /// Set whether the window shows an enabled maximize button.
    pub fn maximizable(mut self, value: bool) -> Self {
        self.maximizable = value;
        self
    }

    /// Choose the palette the window starts with.
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }
}

/// Open a window for `ui` and run the event loop until the app exits, using the
/// compiled-in rendering backend (GL). Blocks until the last window closes.
#[cfg(feature = "backend-gl")]
pub fn run(ui: UI, config: WindowConfig) {
    crate::win::run_gl(ui, config)
}

/// Open a window for `ui` and run the event loop until the app exits, using the
/// compiled-in rendering backend (software). Blocks until the last window closes.
#[cfg(feature = "backend-software")]
pub fn run(ui: UI, config: WindowConfig) {
    if let Err(e) = crate::software_window::run_with_config(ui, config) {
        panic!("software window event loop failed: {e}");
    }
}
