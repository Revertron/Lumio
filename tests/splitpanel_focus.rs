//! Focusing a view in one `SplitPanel` panel must clear focus in the other, so
//! that exactly one view owns focus. Regression test: without this, a focused
//! view in the first panel stays focused alongside the newly clicked view in the
//! second panel and — being earlier in tree order — wins the `focus_owner`
//! sweep, so KeyDown listeners (e.g. Enter-to-send on a composer) never fire.

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
    <SplitPanel id="main_split" width="max" height="max" direction="horizontal" split_pos="300">
        <Frame id="left_panel" width="max" height="max" direction="vertical">
            <Edit id="left_input" text="" width="max" height="30"/>
        </Frame>
        <Frame id="right_panel" width="max" height="max" direction="vertical">
            <Frame id="input_area" width="max" height="min" direction="horizontal">
                <Memo id="message_input" text="" width="max" height="min" max_lines="3"/>
                <Button id="btn_send" text="Send" width="64" height="32"/>
            </Frame>
        </Frame>
    </SplitPanel>
</Frame>
"#;

/// Absolute window center of a deeply-nested view (get_rect is parent-relative).
fn abs_center(ui: &UI, chain: &[&str]) -> Point<i32> {
    let (mut ox, mut oy) = (0, 0);
    for id in chain {
        let r = ui.get_view(id).unwrap().borrow().get_rect();
        ox += r.min.x;
        oy += r.min.y;
    }
    let last = ui.get_view(chain[chain.len() - 1]).unwrap().borrow().get_rect();
    Point::new(ox + (last.max.x - last.min.x) / 2, oy + (last.max.y - last.min.y) / 2)
}

fn click(ui: &mut UI, p: Point<i32>) {
    ui.on_mouse_button_down(p, MouseButton::Left);
    ui.on_mouse_button_up(p, MouseButton::Left);
}

fn is_focused(ui: &UI, id: &str) -> bool {
    ui.get_view(id).unwrap().borrow().is_focused()
}

#[test]
fn focusing_across_split_panels_moves_focus_and_keydown_listener() {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let mut ui = UI::from_xml(LAYOUT, 800, 600, default_typeface(), 1.0).unwrap();
    ui.layout(800, 600, 1.0);

    // Enter fires the send listener on the composer memo.
    let fired = Rc::new(RefCell::new(0));
    let sink = Rc::clone(&fired);
    ui.get_view("message_input").unwrap().borrow_mut().on_event(
        EventType::KeyDown,
        Box::new(move |_ui, _v, data| {
            if let EventData::Key { code, modifiers } = data {
                if matches!(code, Some(VirtualKeyCode::Return)) && !modifiers.shift() {
                    *sink.borrow_mut() += 1;
                    return true;
                }
            }
            false
        }),
    );

    // Focus the left panel's Edit, then the right panel's Memo.
    let left = abs_center(&ui, &["main_split", "left_panel", "left_input"]);
    click(&mut ui, left);
    assert!(is_focused(&ui, "left_input"));

    let memo = abs_center(&ui, &["main_split", "right_panel", "input_area", "message_input"]);
    click(&mut ui, memo);

    // Focus moved wholly to the memo: the left panel is no longer focused.
    assert!(is_focused(&ui, "message_input"), "memo should be focused");
    assert!(!is_focused(&ui, "left_input"), "left panel focus should have been cleared");

    // And Enter now reaches the memo's KeyDown listener.
    let consumed = ui.on_key_down(Some(VirtualKeyCode::Return), 0, ModifiersState::default());
    assert!(consumed, "Enter should be consumed by the memo listener");
    assert_eq!(*fired.borrow(), 1, "memo Enter listener should have fired once");
}
