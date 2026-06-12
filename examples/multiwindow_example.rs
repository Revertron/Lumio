#![windows_subsystem = "windows"]

use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 700;
const HEIGHT: u32 = 420;
const TITLE: &str = "Multi-window Demo";

const MAIN_XML: &str = include_str!("multiwindow_example.xml");
const DIALOG_XML: &str = include_str!("multiwindow_dialog.xml");
const INFO_XML: &str = include_str!("multiwindow_info.xml");

fn main() {
    let mut ui = UI::from_xml(MAIN_XML, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();
    wire_main(&mut ui);

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}

fn wire_main(ui: &mut UI) {
    if let Some(b) = ui.get_view("btn_dialog") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _, _| {
            open_name_dialog(ui);
            true
        }));
    }

    if let Some(b) = ui.get_view("btn_window") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _, _| {
            let info = build_info_ui("This window is not modal:\nthe main window stays interactive.");
            ui.open_window(WindowRequest {
                title: "Normal window".to_string(),
                width: 380,
                height: 160,
                ui: info,
                modal: false,
            });
            true
        }));
    }
}

/// Opens an application-modal dialog asking for a name. The result is
/// written into the main window's `result_label`.
fn open_name_dialog(ui: &mut UI) {
    // Captured into the OK handler below: cross-window view access works
    // because all windows run on the same (event loop) thread.
    let result_label = ui.get_view("result_label");

    let dlg = UI::from_xml(DIALOG_XML, 420, 170, Classic::typeface(), 1.0).unwrap();

    if let Some(b) = dlg.get_view("ok_btn") {
        b.borrow_mut().on_event(EventType::Click, Box::new(move |dlg_ui, _, _| {
            let mut name = String::new();
            if let Some(edit) = dlg_ui.get_view("name_edit") {
                if let Some(e) = edit.borrow_mut().downcast_mut::<Edit>() {
                    name = e.get_text();
                }
            }
            if name.is_empty() {
                name = "stranger".to_string();
            }
            if let Some(label) = &result_label {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text(&format!("Hello, {}!", name));
                }
            }
            dlg_ui.close_window();
            true
        }));
    }

    if let Some(b) = dlg.get_view("cancel_btn") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|dlg_ui, _, _| {
            dlg_ui.close_window();
            true
        }));
    }

    // Modal windows stack: this one blocks the dialog (and the main window)
    // until dismissed.
    if let Some(b) = dlg.get_view("nested_btn") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|dlg_ui, _, _| {
            let info = build_info_ui("A second modal on top of the first.\nOnly this window accepts input now.");
            dlg_ui.open_window(WindowRequest {
                title: "Nested modal".to_string(),
                width: 380,
                height: 160,
                ui: info,
                modal: true,
            });
            true
        }));
    }

    ui.open_window(WindowRequest {
        title: "Enter your name".to_string(),
        width: 420,
        height: 170,
        ui: dlg,
        modal: true,
    });
}

fn build_info_ui(message: &str) -> UI {
    let ui = UI::from_xml(INFO_XML, 380, 160, Classic::typeface(), 1.0).unwrap();

    if let Some(label) = ui.get_view("info_label") {
        if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
            l.set_text(message);
        }
    }

    if let Some(b) = ui.get_view("info_ok") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _, _| {
            ui.close_window();
            true
        }));
    }

    ui
}
