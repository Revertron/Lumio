//! Synthetic-dispatch tests for the event system: the UI's input entry points
//! are plain method calls, so focus/hover/double-click/context-menu/shortcut
//! behavior can be verified without opening a window. Rendering is never
//! invoked; only layout (which needs the bundled fonts) and event dispatch.

use std::cell::RefCell;
use std::rc::Rc;

use include_dir::{include_dir, Dir};

use lumio::prelude::*;
use lumio::types::Rect;

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <Edit id="edit1" text="hello" width="200"/>
    <Edit id="edit2" text="world" width="200"/>
    <Label id="label1" text="Double-click me"/>
    <Button id="btn1" text="Click"/>
</Frame>
"#;

/// A laid-out UI plus a log that event handlers append to.
fn build_ui() -> (UI, Rc<RefCell<Vec<String>>>) {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let mut ui = UI::from_xml(LAYOUT, 800, 600, default_typeface(), 1.0).unwrap();
    ui.layout(800, 600, 1.0);
    (ui, Rc::new(RefCell::new(Vec::new())))
}

/// Center of a view's rect. The test root sits at the window origin and the
/// tested views are its direct children, so child coords are window coords.
fn center(ui: &UI, id: &str) -> Point<i32> {
    let view = ui.get_view(id).unwrap_or_else(|| panic!("no view {}", id));
    let rect: Rect<i32> = view.borrow().get_rect();
    assert!(rect.width() > 0 && rect.height() > 0, "{} has no size — layout failed", id);
    Point::new((rect.min.x + rect.max.x) / 2, (rect.min.y + rect.max.y) / 2)
}

fn log_event(log: &Rc<RefCell<Vec<String>>>, entry: &str) -> Box<dyn FnMut(&mut UI, &dyn View, &EventData) -> bool> {
    let log = Rc::clone(log);
    let entry = entry.to_owned();
    Box::new(move |_ui, _view, _data| {
        log.borrow_mut().push(entry.clone());
        true
    })
}

#[test]
fn focus_events_fire_on_click_and_in_order() {
    let (mut ui, log) = build_ui();
    for id in ["edit1", "edit2"] {
        let view = ui.get_view(id).unwrap();
        view.borrow_mut().on_event(EventType::FocusGained, log_event(&log, &format!("gained {}", id)));
        view.borrow_mut().on_event(EventType::FocusLost, log_event(&log, &format!("lost {}", id)));
    }

    let p1 = center(&ui, "edit1");
    ui.on_mouse_button_down(p1, MouseButton::Left);
    ui.on_mouse_button_up(p1, MouseButton::Left);
    assert_eq!(*log.borrow(), vec!["gained edit1"]);

    let p2 = center(&ui, "edit2");
    ui.on_mouse_button_down(p2, MouseButton::Left);
    ui.on_mouse_button_up(p2, MouseButton::Left);
    // FocusLost on the old owner fires before FocusGained on the new one.
    assert_eq!(*log.borrow(), vec!["gained edit1", "lost edit1", "gained edit2"]);
}

#[test]
fn hover_events_fire_on_enter_and_exit() {
    let (mut ui, log) = build_ui();
    let btn = ui.get_view("btn1").unwrap();
    btn.borrow_mut().on_event(EventType::HoverEnter, log_event(&log, "enter"));
    btn.borrow_mut().on_event(EventType::HoverExit, log_event(&log, "exit"));

    ui.on_mouse_move(center(&ui, "btn1"));
    assert_eq!(*log.borrow(), vec!["enter"]);
    // Moving within the same view fires nothing new.
    let mut inside = center(&ui, "btn1");
    inside.x += 1;
    ui.on_mouse_move(inside);
    assert_eq!(*log.borrow(), vec!["enter"]);

    ui.on_mouse_move(Point::new(790, 590));
    assert_eq!(*log.borrow(), vec!["enter", "exit"]);
}

#[test]
fn double_click_fires_once_with_position() {
    let (mut ui, log) = build_ui();
    let label = ui.get_view("label1").unwrap();
    {
        let log = Rc::clone(&log);
        label.borrow_mut().on_event(EventType::DoubleClick, Box::new(move |_ui, _view, data| {
            log.borrow_mut().push(format!("double {:?}", data));
            true
        }));
    }

    let p = center(&ui, "label1");
    ui.on_mouse_button_down(p, MouseButton::Left);
    ui.on_mouse_button_up(p, MouseButton::Left);
    assert!(log.borrow().is_empty(), "single click must not fire DoubleClick");
    ui.on_mouse_button_down(p, MouseButton::Left);
    ui.on_mouse_button_up(p, MouseButton::Left);
    assert_eq!(log.borrow().len(), 1, "second click fires exactly one DoubleClick");
    assert_eq!(log.borrow()[0], format!("double {:?}", EventData::Position { x: p.x, y: p.y }));

    // A third click immediately after must NOT fire a second event
    // (the detector resets after a double).
    ui.on_mouse_button_down(p, MouseButton::Left);
    ui.on_mouse_button_up(p, MouseButton::Left);
    assert_eq!(log.borrow().len(), 1);
}

#[test]
fn double_click_on_two_views_does_not_fire() {
    let (mut ui, log) = build_ui();
    for id in ["edit1", "edit2"] {
        let view = ui.get_view(id).unwrap();
        view.borrow_mut().on_event(EventType::DoubleClick, log_event(&log, id));
    }
    ui.on_mouse_button_down(center(&ui, "edit1"), MouseButton::Left);
    ui.on_mouse_button_down(center(&ui, "edit2"), MouseButton::Left);
    assert!(log.borrow().is_empty(), "clicks on different views are not a double-click");
}

#[test]
fn context_menu_listener_suppresses_builtin_menu() {
    let (mut ui, log) = build_ui();
    let edit = ui.get_view("edit1").unwrap();
    edit.borrow_mut().on_event(EventType::ContextMenu, log_event(&log, "context"));

    ui.on_mouse_button_down(center(&ui, "edit1"), MouseButton::Right);
    assert_eq!(*log.borrow(), vec!["context"]);
    assert!(!ui.has_popups(), "built-in Edit menu must be suppressed by a consuming handler");

    // edit2 has no listener: the built-in menu opens as before.
    ui.on_mouse_button_down(center(&ui, "edit2"), MouseButton::Right);
    assert!(ui.has_popups(), "built-in Edit menu must still open without a listener");
}

#[test]
fn keydown_listener_runs_before_view_and_can_intercept() {
    let (mut ui, log) = build_ui();
    let edit = ui.get_view("edit1").unwrap();
    {
        let log = Rc::clone(&log);
        edit.borrow_mut().on_event(EventType::KeyDown, Box::new(move |_ui, _view, data| {
            if let EventData::Key { code, .. } = data {
                log.borrow_mut().push(format!("key {:?}", code));
                return *code == Some(VirtualKeyCode::F2); // consume F2 only
            }
            false
        }));
    }

    // Focus edit1 first.
    let p = center(&ui, "edit1");
    ui.on_mouse_button_down(p, MouseButton::Left);
    ui.on_mouse_button_up(p, MouseButton::Left);

    ui.on_key_down(Some(VirtualKeyCode::F2), 0, ModifiersState::default());
    ui.on_key_down(Some(VirtualKeyCode::Home), 0, ModifiersState::default());
    assert_eq!(*log.borrow(), vec!["key Some(F2)", "key Some(Home)"]);
}

#[test]
fn shortcut_fires_when_key_not_consumed() {
    let (mut ui, log) = build_ui();
    {
        let log = Rc::clone(&log);
        ui.add_shortcut("F5", Box::new(move |_ui| {
            log.borrow_mut().push("f5".to_owned());
            true
        }));
    }
    let consumed = ui.on_key_down(Some(VirtualKeyCode::F5), 0, ModifiersState::default());
    assert!(consumed);
    assert_eq!(*log.borrow(), vec!["f5"]);

    // Still fires while an Edit is focused (Edit passes F5 through).
    let p = center(&ui, "edit1");
    ui.on_mouse_button_down(p, MouseButton::Left);
    ui.on_mouse_button_up(p, MouseButton::Left);
    ui.on_key_down(Some(VirtualKeyCode::F5), 0, ModifiersState::default());
    assert_eq!(*log.borrow(), vec!["f5", "f5"]);
}

// Dialogs are now real modal windows (see `lumio::dialog::Dialog`), which the
// synthetic-dispatch harness can't drive. This keeps the still-relevant
// regression: a modal *overlay* blocks global shortcuts until it is dismissed.
#[test]
fn shortcut_blocked_while_modal_popup_open() {
    let (mut ui, log) = build_ui();
    {
        let log = Rc::clone(&log);
        ui.add_shortcut("F5", Box::new(move |_ui| {
            log.borrow_mut().push("f5".to_owned());
            true
        }));
    }

    // A modal popup blocks all input to the root tree, including global
    // keyboard shortcuts.
    let popup: Element = Rc::new(RefCell::new(Label::default()));
    popup.borrow_mut().set_id("modal");
    ui.show_popup(popup, 400, 300, PopupDirection::Center, PopupMode::Modal);

    ui.on_key_down(Some(VirtualKeyCode::F5), 0, ModifiersState::default());
    assert!(log.borrow().is_empty(), "shortcuts must be blocked while a modal is open");

    // Dismissing the modal re-enables shortcuts.
    ui.close_popup("modal");
    assert!(!ui.has_popups());
    ui.on_key_down(Some(VirtualKeyCode::F5), 0, ModifiersState::default());
    assert_eq!(*log.borrow(), vec!["f5"]);
}
