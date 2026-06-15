#![windows_subsystem = "windows"]

use lumio::prelude::*;

const WIDTH: u32 = 700;
const HEIGHT: u32 = 420;
const TITLE: &str = "Multi-window Demo";

const MAIN_XML: &str = include_str!("multiwindow_example.xml");
const INFO_XML: &str = include_str!("multiwindow_info.xml");

fn main() {
    let mut ui = UI::from_xml(MAIN_XML, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();
    wire_main(&mut ui);

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
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
                resizable: true,
                minimizable: true,
                maximizable: true,
            });
            true
        }));
    }

    if let Some(b) = ui.get_view("btn_fixed") {
        b.borrow_mut().on_event(EventType::Click, Box::new(|ui, _, _| {
            let info = build_info_ui("This window is fixed:\nit can't be resized, minimized or maximized.");
            ui.open_window(WindowRequest {
                title: "Fixed window".to_string(),
                width: 380,
                height: 160,
                ui: info,
                modal: false,
                resizable: false,
                minimizable: false,
                maximizable: false,
            });
            true
        }));
    }
}

/// Opens an application-modal input dialog asking for a name, using the
/// `Dialog` builder. The result is written into the main window's
/// `result_label`. Compare this with the hand-rolled `build_info_ui` below to
/// see how much window/button boilerplate the builder removes.
fn open_name_dialog(ui: &mut UI) {
    // Captured into the result handler below: cross-window view access works
    // because all windows run on the same (event loop) thread.
    let result_label = ui.get_view("result_label");

    Dialog::new("Enter your name")
        .message("Enter your name:")
        .input("name_edit", "")
        .button("OK")
        .button("Cancel")
        .default_button("OK")
        .cancel_button("Cancel")
        .on_result(move |dlg_ui, pressed| {
            if pressed != "OK" {
                return;
            }
            let mut name = dlg_ui
                .get_view("name_edit")
                .and_then(|e| e.borrow().downcast_ref::<Edit>().map(|e| e.get_text()))
                .unwrap_or_default();
            if name.is_empty() {
                name = "stranger".to_string();
            }
            if let Some(label) = &result_label {
                if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
                    l.set_text(&format!("Hello, {}!", name));
                }
            }
        })
        .show(ui);
}

fn build_info_ui(message: &str) -> UI {
    let ui = UI::from_xml(INFO_XML, 380, 160, default_typeface(), 1.0).unwrap();

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
