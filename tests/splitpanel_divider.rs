//! The `SplitPanel` seam is grabbable and says so. The divider is not painted —
//! the gap between the panes is the divider — so the only cues a user gets are
//! the drag zone reaching past the gap into both panes and the resize cursor
//! over it. Regression test: the zone used to be exactly the gap and no cursor
//! was ever requested, which made the split near-impossible to drag.
//!
//! The layout sets `divider_size` explicitly and probes one pixel past each
//! edge of the gap, so the test tracks the *behaviour* and does not have to be
//! retuned along with the default gap and grab constants.

use include_dir::{Dir, include_dir};
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

/// Left edge of the gap, matching `split_pos` below.
const SPLIT: i32 = 300;
/// Width of the gap, matching `divider_size` below.
const GAP: i32 = 4;

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <SplitPanel id="split" width="max" height="max" direction="horizontal" split_pos="300" divider_size="4">
        <Frame id="left" width="max" height="max" direction="vertical"/>
        <Frame id="right" width="max" height="max" direction="vertical"/>
    </SplitPanel>
</Frame>
"#;

fn build() -> UI {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let mut ui = UI::from_xml(LAYOUT, 800, 600, default_typeface(), 1.0).unwrap();
    ui.layout(800, 600, 1.0);
    ui
}

fn split_dip(ui: &UI) -> i32 {
    let view = ui.get_view("split").unwrap();
    let view = view.borrow();
    view.as_any().downcast_ref::<SplitPanel>().unwrap().split_dip()
}

#[test]
fn resize_cursor_over_the_seam_and_its_grab_margin() {
    let mut ui = build();

    // Inside the gap itself.
    ui.on_mouse_move(Point::new(SPLIT + GAP / 2, 100));
    assert_eq!(ui.current_cursor(), MouseCursorType::EwResize, "gap should show the resize cursor");

    // Just left of the gap — inside the left pane, within the grab margin.
    ui.on_mouse_move(Point::new(SPLIT - 1, 100));
    assert_eq!(ui.current_cursor(), MouseCursorType::EwResize, "grab margin reaches into the left pane");

    // Just right of the gap — inside the right pane, within the grab margin.
    ui.on_mouse_move(Point::new(SPLIT + GAP + 1, 100));
    assert_eq!(ui.current_cursor(), MouseCursorType::EwResize, "grab margin reaches into the right pane");

    // Well away from the seam: no resize cursor.
    ui.on_mouse_move(Point::new(SPLIT - 60, 100));
    assert_eq!(ui.current_cursor(), MouseCursorType::Default, "away from the seam it is the plain arrow");
}

#[test]
fn dragging_from_the_grab_margin_moves_the_split() {
    let mut ui = build();
    assert_eq!(split_dip(&ui), SPLIT);

    // Press one pixel *left* of the gap — outside it, inside the grab zone.
    assert!(ui.on_mouse_button_down(Point::new(SPLIT - 1, 100), MouseButton::Left), "grab margin should start a drag");
    ui.on_mouse_move(Point::new(SPLIT + 99, 100));
    assert_eq!(ui.current_cursor(), MouseCursorType::EwResize, "cursor stays a resize cursor while dragging");
    ui.on_mouse_button_up(Point::new(SPLIT + 99, 100), MouseButton::Left);

    assert_eq!(split_dip(&ui), SPLIT + 100, "split should follow the pointer");
}

#[test]
fn right_click_near_the_seam_does_not_start_a_drag() {
    let mut ui = build();

    // The grab zone overlaps both panes, so a right-click there must fall
    // through to the child rather than being swallowed as a drag.
    assert!(!ui.on_mouse_button_down(Point::new(SPLIT - 1, 100), MouseButton::Right));
    ui.on_mouse_move(Point::new(SPLIT + 99, 100));
    assert_eq!(split_dip(&ui), SPLIT, "right button must not drag the split");
}
