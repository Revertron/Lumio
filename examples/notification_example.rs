#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 700;
const TITLE: &str = "Lumio — Notification stack demo";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider { dir: Dir<'static> }
impl Provider { fn new(dir: Dir<'static>) -> Self { Self { dir } } }
impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

const COLOR_INFO:    u32 = 0xFFE6F2FF;
const COLOR_WARN:    u32 = 0xFFFFF8E1;
const COLOR_ERROR:   u32 = 0xFFFFE5E5;
const COLOR_SUCCESS: u32 = 0xFFE5FFE9;
const BORDER_INFO:    u32 = 0xFF4A90E2;
const BORDER_WARN:    u32 = 0xFFE8A33D;
const BORDER_ERROR:   u32 = 0xFFD83A3A;
const BORDER_SUCCESS: u32 = 0xFF3FAA56;

thread_local! {
    static NEXT_ID: std::cell::Cell<u32> = std::cell::Cell::new(0);
    static BG_CLICKS: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

fn next_id() -> String {
    NEXT_ID.with(|c| { let v = c.get(); c.set(v + 1); format!("toast-{}", v) })
}

/// Build a generic toast view: a Frame holding a message Label and a close-X Button.
/// The Frame's id is set by `show_notification`. The close button calls
/// `ui.dismiss_notification_for(view)` which walks parents to find the toast id.
fn make_toast(message: &str, bg: u32, border: u32) -> Element {
    let frame: Element = Rc::new(RefCell::new(Frame::new(
        lumio::types::rect((0, 0), (340, 40)),
        Dimension::Max,
        Dimension::Min,
    )));
    {
        let mut f = frame.borrow_mut();
        f.set_padding(8, 12, 8, 12);
        f.set_background(Some(bg));
        f.set_border_color(Some(border));
    }

    let label: Element = Rc::new(RefCell::new(Label::default()));
    {
        let mut l = label.borrow_mut();
        l.set_any("text", message);
        l.set_any("font_size", "16");
        l.set_width(Dimension::Max);
        l.set_height(Dimension::Min);
    }

    let close: Element = Rc::new(RefCell::new(Button::default()));
    {
        let mut b = close.borrow_mut();
        b.set_any("text", "x");
        b.set_width(Dimension::Dip(28));
        b.set_height(Dimension::Dip(28));
        b.set_margin(0, 8, 0, 0);
        b.on_event(EventType::Click, Box::new(|ui, view| {
            ui.dismiss_notification_for(view);
            true
        }));
    }

    {
        let mut f = frame.borrow_mut();
        let container = f.as_container_mut().unwrap();
        label.borrow_mut().set_parent(Some(Rc::downgrade(&frame)));
        close.borrow_mut().set_parent(Some(Rc::downgrade(&frame)));
        container.add_view(label);
        container.add_view(close);
    }

    frame
}

fn push_toast(ui: &mut UI, message: &str, bg: u32, border: u32, timeout: Option<Duration>) {
    let toast = make_toast(message, bg, border);
    ui.show_notification(toast, &next_id(), timeout);
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("notification_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    if let Some(b) = ui.get_view("btn_info") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            push_toast(ui, "Heads up: this is an info notification.", COLOR_INFO, BORDER_INFO, Some(Duration::from_secs(4)));
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_warn") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            push_toast(ui, "Warning: something might be off here.", COLOR_WARN, BORDER_WARN, Some(Duration::from_secs(5)));
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_error") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            push_toast(ui, "Error: that didn't work — please retry.", COLOR_ERROR, BORDER_ERROR, Some(Duration::from_secs(6)));
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_success") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            push_toast(ui, "Success: action completed.", COLOR_SUCCESS, BORDER_SUCCESS, Some(Duration::from_secs(3)));
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_persist") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            push_toast(ui, "I won't auto-dismiss — click my X.", COLOR_INFO, BORDER_INFO, None);
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_burst") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            for i in 1..=5 {
                push_toast(ui, &format!("Burst notification #{}", i), COLOR_INFO, BORDER_INFO, Some(Duration::from_secs(2 + i)));
            }
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_clear") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            ui.dismiss_all_notifications();
            true
        }));
    }
    if let Some(b) = ui.get_view("btn_bg") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _| {
            BG_CLICKS.with(|c| c.set(c.get() + 1));
            let n = BG_CLICKS.with(|c| c.get());
            if let Some(label) = ui.get_view("count") {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text(&format!("Background button click count: {}", n));
                }
            }
            true
        }));
    }

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}
