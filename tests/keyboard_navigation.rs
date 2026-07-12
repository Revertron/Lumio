//! Synthetic-dispatch tests for keyboard navigation: Tab/Shift+Tab focus
//! traversal, Space/Enter widget activation and TabView keyboard support.
//! Same harness as tests/event_system.rs — no window, just layout + dispatch.

use std::cell::RefCell;
use std::rc::Rc;

use include_dir::{include_dir, Dir};

use lumio::prelude::*;

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
    <Frame id="inner" width="max" height="min" direction="horizontal">
        <Button id="btn1" text="One"/>
        <CheckBox id="check1" text="Check"/>
    </Frame>
    <Label id="label1" text="not focusable"/>
    <Button id="btn2" text="Two"/>
</Frame>
"#;

fn build_ui(xml: &str) -> UI {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let mut ui = UI::from_xml(xml, 800, 600, default_typeface(), 1.0).unwrap();
    ui.layout(800, 600, 1.0);
    ui
}

fn focused_id(ui: &UI) -> Option<String> {
    ui.find_with(&|v: &dyn View| v.get_state().map(|s| s.focused).unwrap_or(false))
        .first()
        .map(|el| el.borrow().get_id())
}

fn press_tab(ui: &mut UI, shift: bool) {
    let mods = ModifiersState::new(false, false, shift, false);
    ui.on_key_down(Some(VirtualKeyCode::Tab), 0, mods.clone());
    ui.on_key_up(Some(VirtualKeyCode::Tab), 0, mods);
}

fn press_key(ui: &mut UI, code: VirtualKeyCode) {
    ui.on_key_down(Some(code), 0, ModifiersState::default());
    ui.on_key_up(Some(code), 0, ModifiersState::default());
}

#[test]
fn tab_walks_all_focusables_in_document_order_and_wraps() {
    let mut ui = build_ui(LAYOUT);
    assert_eq!(focused_id(&ui), None);

    // First Tab focuses the first focusable view; nested frames are entered
    // in document order and non-focusable views (Label) are skipped.
    for expected in ["edit1", "btn1", "check1", "btn2", "edit1"] {
        press_tab(&mut ui, false);
        assert_eq!(focused_id(&ui).as_deref(), Some(expected));
    }
}

#[test]
fn shift_tab_walks_backwards_and_wraps() {
    let mut ui = build_ui(LAYOUT);

    // From nothing, Shift+Tab focuses the last focusable view.
    for expected in ["btn2", "check1", "btn1", "edit1", "btn2"] {
        press_tab(&mut ui, true);
        assert_eq!(focused_id(&ui).as_deref(), Some(expected));
    }
}

#[test]
fn tab_skips_disabled_and_invisible_views() {
    let mut ui = build_ui(LAYOUT);
    ui.get_view("btn1").unwrap().borrow_mut().set_enabled(false);
    ui.get_view("check1").unwrap().borrow_mut().set_visibility(Visibility::Gone);

    for expected in ["edit1", "btn2", "edit1"] {
        press_tab(&mut ui, false);
        assert_eq!(focused_id(&ui).as_deref(), Some(expected));
    }
}

#[test]
fn space_and_enter_click_a_focused_button() {
    let mut ui = build_ui(LAYOUT);
    let clicks = Rc::new(RefCell::new(0));
    {
        let clicks = Rc::clone(&clicks);
        ui.get_view("btn1").unwrap().borrow_mut().on_event(EventType::Click, Box::new(move |_ui, _view, _data| {
            *clicks.borrow_mut() += 1;
            true
        }));
    }

    let btn1 = ui.get_view("btn1").unwrap();
    ui.set_focus_to(&btn1);

    press_key(&mut ui, VirtualKeyCode::Space);
    assert_eq!(*clicks.borrow(), 1, "Space must click the focused button");
    press_key(&mut ui, VirtualKeyCode::Return);
    assert_eq!(*clicks.borrow(), 2, "Enter must click the focused button");

    // A key-up without a preceding key-down on this view does nothing.
    ui.on_key_up(Some(VirtualKeyCode::Space), 0, ModifiersState::default());
    assert_eq!(*clicks.borrow(), 2);
}

#[test]
fn space_toggles_a_focused_checkbox() {
    let mut ui = build_ui(LAYOUT);
    let check = ui.get_view("check1").unwrap();
    ui.set_focus_to(&check);

    let is_checked = |ui: &UI| {
        let el = ui.get_view("check1").unwrap();
        let el = el.borrow();
        el.downcast_ref::<CheckBox>().unwrap().is_checked()
    };
    assert!(!is_checked(&ui));
    press_key(&mut ui, VirtualKeyCode::Space);
    assert!(is_checked(&ui), "Space must check the focused checkbox");
    press_key(&mut ui, VirtualKeyCode::Space);
    assert!(!is_checked(&ui), "Space must uncheck it again");
}

#[test]
fn tab_between_views_sharing_an_id_is_still_consumed() {
    // Two views with the same id: sync_focus's id-based diff sees "no
    // change", but Tab must still report consumed so the window redraws.
    let mut ui = build_ui(r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <Button id="twin" text="One"/>
    <Button id="twin" text="Two"/>
</Frame>
"#);
    press_tab(&mut ui, false);
    let mods = ModifiersState::default();
    let consumed = ui.on_key_down(Some(VirtualKeyCode::Tab), 0, mods.clone());
    ui.on_key_up(Some(VirtualKeyCode::Tab), 0, mods);
    assert!(consumed, "moving focus between same-id views must consume Tab");
    // And focus really is on the second twin now.
    let focused = ui.find_with(&|v: &dyn View| v.get_state().map(|s| s.focused).unwrap_or(false));
    assert_eq!(focused.len(), 1);
    let el = focused[0].borrow();
    assert_eq!(el.downcast_ref::<Button>().unwrap().get_text(), "Two");
}

const TAB_LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <Edit id="edit1" text="before" width="200"/>
    <TabView id="tabs" width="max" height="200">
        <Frame id="First" width="max" height="max" direction="vertical">
            <Button id="tab1_btn" text="On tab 1"/>
        </Frame>
        <Frame id="Second" width="max" height="max" direction="vertical">
            <Button id="tab2_btn" text="On tab 2"/>
        </Frame>
    </TabView>
    <Button id="after" text="After"/>
</Frame>
"#;

#[test]
fn tab_traversal_stops_at_tab_strip_and_skips_inactive_tabs() {
    let mut ui = build_ui(TAB_LAYOUT);

    // The strip is a focus stop between edit1 and the active tab's content;
    // tab2_btn (inactive tab) is not reachable.
    for expected in ["edit1", "tabs", "tab1_btn", "after", "edit1"] {
        press_tab(&mut ui, false);
        assert_eq!(focused_id(&ui).as_deref(), Some(expected));
    }
}

#[test]
fn arrows_switch_tabs_while_strip_is_focused() {
    let mut ui = build_ui(TAB_LAYOUT);
    let selections = Rc::new(RefCell::new(Vec::new()));
    {
        let selections = Rc::clone(&selections);
        ui.get_view("tabs").unwrap().borrow_mut().on_event(EventType::SelectionChanged, Box::new(move |_ui, _view, data| {
            if let EventData::Selected(i) = data {
                selections.borrow_mut().push(*i);
            }
            true
        }));
    }

    let tabs = ui.get_view("tabs").unwrap();
    ui.set_focus_to(&tabs);

    let active = |ui: &UI| {
        let el = ui.get_view("tabs").unwrap();
        let el = el.borrow();
        el.downcast_ref::<TabView>().unwrap().get_active_tab()
    };
    assert_eq!(active(&ui), 0);

    press_key(&mut ui, VirtualKeyCode::Right);
    assert_eq!(active(&ui), 1, "Right must switch to the next tab");
    press_key(&mut ui, VirtualKeyCode::Right);
    assert_eq!(active(&ui), 1, "Right at the last tab stays put");
    press_key(&mut ui, VirtualKeyCode::Left);
    assert_eq!(active(&ui), 0, "Left must switch back");
    press_key(&mut ui, VirtualKeyCode::Left);
    assert_eq!(active(&ui), 0, "Left at the first tab stays put");
    assert_eq!(*selections.borrow(), vec![1, 0], "each real switch fires SelectionChanged once");

    // After switching tabs from the keyboard, Tab now descends into the
    // newly active tab's content.
    press_key(&mut ui, VirtualKeyCode::Right);
    press_tab(&mut ui, false);
    assert_eq!(focused_id(&ui).as_deref(), Some("tab2_btn"));
}

#[test]
fn switching_tabs_keeps_keys_away_from_hidden_children() {
    let mut ui = build_ui(TAB_LAYOUT);
    let clicks = Rc::new(RefCell::new(0));
    {
        let clicks = Rc::clone(&clicks);
        ui.get_view("tab1_btn").unwrap().borrow_mut().on_event(EventType::Click, Box::new(move |_ui, _view, _data| {
            *clicks.borrow_mut() += 1;
            true
        }));
    }

    // Focus the button on tab 1, then switch to tab 2 via the strip.
    let btn = ui.get_view("tab1_btn").unwrap();
    ui.set_focus_to(&btn);
    let tabs = ui.get_view("tabs").unwrap();
    ui.set_focus_to(&tabs);
    press_key(&mut ui, VirtualKeyCode::Right);

    // Space must not click the now-hidden button on tab 1.
    press_key(&mut ui, VirtualKeyCode::Space);
    assert_eq!(*clicks.borrow(), 0);
}
