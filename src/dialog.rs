//! Window-based dialogs built on the multi-window system.
//!
//! A [`Dialog`] is a builder that assembles a small layout (an optional icon
//! and message, an optional text input, and a row of buttons), sizes a window
//! to fit it, wires the buttons to a single result callback, and opens it as
//! an application-modal child window via [`UI::open_window`].
//!
//! ```no_run
//! # use lumio::prelude::*;
//! # fn demo(ui: &mut UI) {
//! Dialog::new("Delete file")
//!     .icon("icons/warning.png")
//!     .message("Are you sure you want to delete this file?")
//!     .button("Yes").button("No")
//!     .default_button("Yes").cancel_button("No")
//!     .on_result(|_ui, pressed| println!("Pressed: {pressed}"))
//!     .show(ui);
//! # }
//! ```
//!
//! For the common cases, see [`UI::show_message`], [`UI::show_confirm`] and
//! [`UI::show_input`].

use std::cell::RefCell;
use std::rc::Rc;

use crate::containers::Frame;
use crate::events::EventType;
use crate::themes::{Typeface, default_typeface};
use crate::traits::{Element, View};
use crate::types::rect;
use crate::ui::{UI, WindowRequest};
use crate::views::{Button, Dimension, Edit, ImageView, Label};

// Layout geometry in device-independent pixels. Text sizes are NOT constants
// here: they come from the palette typeface roles (`current_text_size`).
const MIN_WIDTH: i32 = 280;
const MAX_WIDTH: i32 = 480;
const MIN_HEIGHT: i32 = 90;
const PADDING: i32 = 12;
/// Vertical gap between the message, the input and the button bar.
const GAP: i32 = 12;
/// Margin around every button (adjacent buttons end up `2 * BUTTON_MARGIN` apart).
const BUTTON_MARGIN: i32 = 4;
const ICON_SIZE: i32 = 32;
const ICON_GAP: i32 = 12;
/// A large bound used to measure content at its natural (unconstrained) size.
const MEASURE_BOUND: i32 = 100_000;
/// Fallback size when a dialog cannot be auto-measured (e.g. custom content
/// without an explicit `.size()`).
const DEFAULT_WIDTH: u32 = 420;
const DEFAULT_HEIGHT: u32 = 170;

/// Which side of the button bar a button sits on. Right-side buttons pack
/// against the right edge (primary actions); left-side buttons stay on the
/// left (auxiliary actions like "Help").
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ButtonSide {
    #[default]
    Right,
    Left,
}

struct DialogButton {
    id: String,
    label: String,
    side: ButtonSide,
}

/// Called with the pressed button's id when the dialog is dismissed.
type ResultCallback = Box<dyn FnMut(&mut UI, &str)>;

/// The shared result callback. A single dialog fires exactly one result, so it
/// is stored in an `Option` and `take`n by whichever button (or shortcut) fires
/// first — every other handler then finds it empty and only closes the window.
type ResultSlot = Rc<RefCell<Option<ResultCallback>>>;

/// A builder for an application-modal dialog window.
///
/// Add a [`message`](Dialog::message)/[`icon`](Dialog::icon)/[`input`](Dialog::input)
/// (or fully custom [`content`](Dialog::content)), one or more
/// [`button`](Dialog::button)s, an [`on_result`](Dialog::on_result) callback,
/// then [`show`](Dialog::show) it. The window auto-sizes to its content unless
/// [`size`](Dialog::size) is set.
pub struct Dialog {
    title: String,
    icon: Option<String>,
    message: Option<String>,
    input: Option<(String, String)>,
    content: Option<UI>,
    buttons: Vec<DialogButton>,
    default_id: Option<String>,
    cancel_id: Option<String>,
    typeface: Typeface,
    size: Option<(u32, u32)>,
    resizable: bool,
    on_result: Option<ResultCallback>,
}

impl Dialog {
    /// Starts a dialog with the given window title.
    pub fn new(title: &str) -> Self {
        Dialog {
            title: title.to_owned(),
            icon: None,
            message: None,
            input: None,
            content: None,
            buttons: Vec::new(),
            default_id: None,
            cancel_id: None,
            typeface: default_typeface(),
            size: None,
            resizable: false,
            on_result: None,
        }
    }

    /// Sets the message text shown above the buttons.
    pub fn message(mut self, text: &str) -> Self {
        self.message = Some(text.to_owned());
        self
    }

    /// Sets a leading icon, loaded from the given asset path (PNG or SVG).
    pub fn icon(mut self, asset_path: &str) -> Self {
        self.icon = Some(asset_path.to_owned());
        self
    }

    /// Adds a single-line text input with the given view id and initial text.
    /// Read it back in [`on_result`](Dialog::on_result) via
    /// `ui.get_view(id)` (the callback runs against the dialog's own `UI`).
    pub fn input(mut self, id: &str, initial: &str) -> Self {
        self.input = Some((id.to_owned(), initial.to_owned()));
        self
    }

    /// Uses a fully custom body `UI` (e.g. from [`UI::from_xml`]) instead of the
    /// standard icon/message/input layout. The buttons added via
    /// [`button`](Dialog::button) must already exist in this UI (matched by id);
    /// provide an explicit [`size`](Dialog::size) (custom content is not
    /// auto-measured).
    pub fn content(mut self, ui: UI) -> Self {
        self.content = Some(ui);
        self
    }

    /// Sets an explicit window size in dips, opting out of auto-sizing.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = Some((width, height));
        self
    }

    /// Sets whether the dialog window can be resized. Dialogs are fixed by
    /// default (non-resizable, with no minimize/maximize buttons); pass `true`
    /// to open a normal resizable window instead.
    pub fn resizable(mut self, value: bool) -> Self {
        self.resizable = value;
        self
    }

    /// Overrides the typeface (defaults to [`default_typeface`]).
    pub fn typeface(mut self, typeface: Typeface) -> Self {
        self.typeface = typeface;
        self
    }

    /// Adds a right-side button whose id equals its label.
    pub fn button(self, label: &str) -> Self {
        self.button_id(label, label, ButtonSide::Right)
    }

    /// Adds a button with an explicit id, label and side.
    pub fn button_id(mut self, id: &str, label: &str, side: ButtonSide) -> Self {
        self.buttons.push(DialogButton {
            id: id.to_owned(),
            label: label.to_owned(),
            side,
        });
        self
    }

    /// Marks the button with this id as the default: pressing Enter fires it.
    pub fn default_button(mut self, id: &str) -> Self {
        self.default_id = Some(id.to_owned());
        self
    }

    /// Marks the button with this id as the cancel button: pressing Esc fires
    /// it (and closes the window). Without a cancel button, Esc just closes the
    /// window with no result.
    pub fn cancel_button(mut self, id: &str) -> Self {
        self.cancel_id = Some(id.to_owned());
        self
    }

    /// Sets the callback invoked with the pressed button's id when the dialog
    /// is dismissed by a button (or by Enter/Esc mapped to one). It runs
    /// against the dialog's own `UI`, just before the window closes.
    pub fn on_result(mut self, f: impl FnMut(&mut UI, &str) + 'static) -> Self {
        self.on_result = Some(Box::new(f));
        self
    }

    /// Builds and opens the dialog as an application-modal child window of `ui`.
    pub fn show(self, ui: &mut UI) {
        // Dialogs are fixed by default: a fixed dialog disables resize and the
        // minimize/maximize buttons; `.resizable(true)` opens a normal window.
        let resizable = self.resizable;
        let (dialog_ui, title, width, height) = self.build_window();
        ui.open_window(WindowRequest {
            title,
            width,
            height,
            ui: dialog_ui,
            modal: true,
            resizable,
            minimizable: resizable,
            maximizable: resizable,
        });
    }

    /// Builds the wired body `UI` and its size, without opening a window.
    /// Returns `(ui, title, width, height)`. Used by [`show`](Dialog::show) and
    /// by tests that drive the dialog without a real window.
    pub(crate) fn build_window(self) -> (UI, String, u32, u32) {
        let title = self.title.clone();
        let slot: ResultSlot = Rc::new(RefCell::new(self.on_result));

        // Custom content: use it verbatim, wire by id, no auto-measure.
        if let Some(content) = self.content {
            let (w, h) = self.size.unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
            let mut ui = content;
            wire(&mut ui, &self.buttons, &self.default_id, &self.cancel_id, &slot);
            return (ui, title, w, h);
        }

        let typeface = self.typeface.clone();

        // Standard body. Everything starts at its natural (Min) size so the
        // root measures to its content; we then flip to Max to fill the window.
        let root = make_frame("vertical", "min");
        root.borrow_mut().set_any("padding", &PADDING.to_string());

        let mut message_el: Option<Element> = None;
        let mut row_el: Option<Element> = None;

        match (&self.icon, &self.message) {
            (Some(icon_path), message) => {
                let row = make_frame("horizontal", "min");
                let icon = make_icon(icon_path);
                add_child(&row, icon);
                if let Some(text) = message {
                    let msg = make_message(text);
                    msg.borrow_mut().set_any("margin_left", &ICON_GAP.to_string());
                    msg.borrow_mut().set_any("gravity", "center_vertical");
                    add_child(&row, Rc::clone(&msg));
                    message_el = Some(msg);
                }
                add_child(&root, Rc::clone(&row));
                row_el = Some(row);
            }
            (None, Some(text)) => {
                let msg = make_message(text);
                add_child(&root, Rc::clone(&msg));
                message_el = Some(msg);
            }
            (None, None) => {}
        }

        let edit_el = self.input.as_ref().map(|(id, initial)| {
            let edit = make_input(id, initial);
            edit.borrow_mut().set_any("margin_top", &GAP.to_string());
            add_child(&root, Rc::clone(&edit));
            edit
        });

        let bar = make_frame("horizontal", "min");
        bar.borrow_mut().set_any("margin_top", &GAP.to_string());
        // Render left-side buttons first, then right-side. The first right-side
        // button gets `gravity=right`, so the leftover row space is inserted
        // before it, pushing it and following buttons against the right edge.
        let mut first_right = true;
        for button in self.buttons.iter().filter(|b| b.side == ButtonSide::Left) {
            add_child(&bar, make_button(&button.id, &button.label));
        }
        for button in self.buttons.iter().filter(|b| b.side == ButtonSide::Right) {
            let b = make_button(&button.id, &button.label);
            if first_right {
                b.borrow_mut().set_any("gravity", "right");
                first_right = false;
            }
            add_child(&bar, b);
        }
        add_child(&root, Rc::clone(&bar));

        // Size the window: explicit or auto-measured.
        let (width, height) = match self.size {
            Some((w, h)) => (w, h),
            None => {
                let nat_w = measure(&root, MEASURE_BOUND, &typeface).0;
                let window_w = nat_w.clamp(MIN_WIDTH, MAX_WIDTH);
                // Flip widths to Max so content fills the window; the message
                // now wraps to the window width.
                root.borrow_mut().set_any("width", "max");
                if let Some(row) = &row_el {
                    row.borrow_mut().set_any("width", "max");
                }
                if let Some(msg) = &message_el {
                    msg.borrow_mut().set_any("width", "max");
                }
                if let Some(edit) = &edit_el {
                    edit.borrow_mut().set_any("width", "max");
                }
                bar.borrow_mut().set_any("width", "max");
                let window_h = measure(&root, window_w, &typeface).1.max(MIN_HEIGHT);
                (window_w as u32, window_h as u32)
            }
        };

        // Fill the window in both axes (covers the explicit-size branch too).
        root.borrow_mut().set_any("width", "max");
        root.borrow_mut().set_any("height", "max");
        if let Some(row) = &row_el {
            row.borrow_mut().set_any("width", "max");
        }
        if let Some(msg) = &message_el {
            msg.borrow_mut().set_any("width", "max");
        }
        if let Some(edit) = &edit_el {
            edit.borrow_mut().set_any("width", "max");
        }
        bar.borrow_mut().set_any("width", "max");

        let mut ui = UI::new(width, height, typeface, 1.0);
        ui.add_view(root);
        wire(&mut ui, &self.buttons, &self.default_id, &self.cancel_id, &slot);
        (ui, title, width, height)
    }
}

/// Lays the root out at `avail_w` (height unconstrained) and returns its rect
/// size — the same offscreen-measure trick `UI::show_popup`/`show_tooltip` use.
fn measure(root: &Element, avail_w: i32, typeface: &Typeface) -> (i32, i32) {
    let r = root
        .borrow_mut()
        .layout_content(0, 0, avail_w, MEASURE_BOUND, typeface, 1.0);
    (r.width(), r.height())
}

/// Wires every button's `Click` (and the Enter/Esc shortcuts) to fire the
/// shared result with that button's id and close the window.
fn wire(
    ui: &mut UI,
    buttons: &[DialogButton],
    default_id: &Option<String>,
    cancel_id: &Option<String>,
    slot: &ResultSlot,
) {
    for button in buttons {
        if let Some(view) = ui.get_view(&button.id) {
            let slot = Rc::clone(slot);
            let id = button.id.clone();
            view.borrow_mut().on_event(
                EventType::Click,
                Box::new(move |dlg_ui, _, _| {
                    fire_and_close(&slot, dlg_ui, &id);
                    true
                }),
            );
        }
    }

    if let Some(id) = default_id {
        let slot = Rc::clone(slot);
        let id = id.clone();
        ui.add_shortcut(
            "Enter",
            Box::new(move |dlg_ui| {
                fire_and_close(&slot, dlg_ui, &id);
                true
            }),
        );
    }

    if let Some(id) = cancel_id {
        let slot = Rc::clone(slot);
        let id = id.clone();
        // Consuming Esc here keeps the window handler from also closing it.
        ui.add_shortcut(
            "Escape",
            Box::new(move |dlg_ui| {
                fire_and_close(&slot, dlg_ui, &id);
                true
            }),
        );
    }
}

/// Fires the result (once) with `id`, then requests the window to close.
fn fire_and_close(slot: &ResultSlot, ui: &mut UI, id: &str) {
    let taken = slot.borrow_mut().take();
    if let Some(mut callback) = taken {
        callback(ui, id);
    }
    ui.close_window();
}

fn make_frame(direction: &str, width: &str) -> Element {
    let mut frame = Frame::new(rect((0, 0), (0, 0)), Dimension::Min, Dimension::Min);
    frame.set_any("direction", direction);
    frame.set_any("width", width);
    Rc::new(RefCell::new(frame))
}

fn make_message(text: &str) -> Element {
    let mut label = Label::default();
    label.set_any("text", text);
    label.set_any("width", "min");
    label.set_any("height", "min");
    Rc::new(RefCell::new(label))
}

fn make_icon(asset_path: &str) -> Element {
    let mut icon = ImageView::default();
    icon.set_any("image", asset_path);
    icon.set_any("width", &ICON_SIZE.to_string());
    icon.set_any("height", &ICON_SIZE.to_string());
    // Tint to the theme text color so monochrome (white-authored) icons stay
    // visible; resolved at build time, which is fine for a short-lived dialog.
    icon.set_tint(Some(crate::drawing::current_color("text")));
    Rc::new(RefCell::new(icon))
}

fn make_input(id: &str, initial: &str) -> Element {
    let mut edit = Edit::new(rect((0, 0), (0, 0)), initial, crate::drawing::current_text_size("text"));
    edit.set_any("id", id);
    // Min while measuring; flipped to Max to fill the window.
    edit.set_any("width", "min");
    Rc::new(RefCell::new(edit))
}

fn make_button(id: &str, label: &str) -> Element {
    let mut button = Button::new(rect((0, 0), (0, 0)), label, crate::drawing::current_text_size("button"));
    button.set_any("id", id);
    button.set_any("margin", &BUTTON_MARGIN.to_string());
    Rc::new(RefCell::new(button))
}

fn add_child(parent: &Element, child: Element) {
    child.borrow().set_parent(Some(Rc::downgrade(parent)));
    parent
        .borrow_mut()
        .as_container_mut()
        .unwrap()
        .add_view(child);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventData;

    /// A button click fires the result with its id and requests the window to
    /// close. Uses an explicit size to keep the test independent of fonts.
    #[test]
    fn button_click_fires_result_and_closes() {
        let pressed = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&pressed);
        let (mut ui, title, w, h) = Dialog::new("Confirm")
            .message("Sure?")
            .button("Yes")
            .button("No")
            .default_button("Yes")
            .cancel_button("No")
            .size(300, 140)
            .on_result(move |_ui, id| *sink.borrow_mut() = Some(id.to_owned()))
            .build_window();

        assert_eq!(title, "Confirm");
        assert_eq!((w, h), (300, 140));
        assert!(ui.get_view("Yes").is_some());
        assert!(ui.get_view("No").is_some());

        let no = ui.get_view("No").unwrap();
        let fired = no.borrow().fire_event(&mut ui, EventType::Click, &EventData::None);
        assert!(fired);
        assert_eq!(*pressed.borrow(), Some("No".to_owned()));
        assert!(ui.take_close_request());
    }

    /// The result fires only once: a second button press finds the slot empty
    /// but still closes the window.
    #[test]
    fn result_fires_once() {
        let count = Rc::new(RefCell::new(0));
        let sink = Rc::clone(&count);
        let (mut ui, _title, _w, _h) = Dialog::new("Once")
            .message("Sure?")
            .button("OK")
            .size(300, 140)
            .on_result(move |_ui, _id| *sink.borrow_mut() += 1)
            .build_window();

        let ok = ui.get_view("OK").unwrap();
        ok.borrow().fire_event(&mut ui, EventType::Click, &EventData::None);
        ok.borrow().fire_event(&mut ui, EventType::Click, &EventData::None);
        assert_eq!(*count.borrow(), 1);
    }
}
