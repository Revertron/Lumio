//! `TreeView::key_at` maps a window position to the row under it, so a
//! right-click can act on that row, and a right press selects that row the way
//! it does everywhere on Windows: press to pick, release to ask what to do.

use lumio::input::MouseButton;
use lumio::prelude::*;

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="horizontal" padding="20">
    <TreeView id="tree" width="max" height="max"/>
</Frame>
"#;

fn tree_of(ui: &UI) -> Element {
    ui.get_view("tree").unwrap()
}

#[test]
fn a_position_maps_to_the_row_under_it() {
    let mut ui = UI::from_xml(LAYOUT, 400, 400, Typeface::default(), 1.0).unwrap();
    {
        let element = tree_of(&ui);
        let view = element.borrow();
        let tree = view.as_any().downcast_ref::<TreeView>().unwrap();
        let mut group = TreeNode::new("Local", "Local", true);
        group.expanded = true;
        group.children = vec![
            TreeNode::new("baboon", "Local/baboon", false),
            TreeNode::new("gateway", "Local/gateway", false),
        ];
        tree.set_roots(vec![group, TreeNode::new("scratch", "scratch", false)]);
    }
    ui.layout(400, 400, 1.0);

    let element = tree_of(&ui);
    let view = element.borrow();
    let tree = view.as_any().downcast_ref::<TreeView>().unwrap();
    let origin = view.get_absolute_position();
    let rect = view.get_rect();
    assert!(rect.height() > 0, "the tree was measured");

    // Four rows are on screen; walk down through them and collect what each
    // vertical band reports.
    let expected = ["Local", "Local/baboon", "Local/gateway", "scratch"];
    let mut seen: Vec<String> = Vec::new();
    for y in 0..rect.height() {
        if let Some(key) = tree.key_at(Point::new(origin.x + 4, origin.y + y))
            && seen.last() != Some(&key)
        {
            seen.push(key);
        }
    }
    assert_eq!(seen, expected, "rows come back top to bottom");

    // Outside the tree entirely.
    assert_eq!(tree.key_at(Point::new(origin.x - 5, origin.y + 4)), None);
    assert_eq!(tree.key_at(Point::new(origin.x + 4, origin.y - 5)), None);
    // Below the last row: inside the widget, but on nothing.
    assert_eq!(
        tree.key_at(Point::new(origin.x + 4, origin.y + rect.height() - 1)),
        None,
        "empty space under the rows belongs to no row"
    );
}

#[test]
fn a_right_press_selects_the_row_under_it() {
    let mut ui = UI::from_xml(LAYOUT, 400, 400, Typeface::default(), 1.0).unwrap();
    {
        let element = tree_of(&ui);
        let view = element.borrow();
        let tree = view.as_any().downcast_ref::<TreeView>().unwrap();
        let mut group = TreeNode::new("Local", "Local", true);
        group.expanded = true;
        group.children = vec![
            TreeNode::new("baboon", "Local/baboon", false),
            TreeNode::new("gateway", "Local/gateway", false),
        ];
        tree.set_roots(vec![group]);
    }
    ui.layout(400, 400, 1.0);

    // Find a point on the last row rather than assuming the row geometry.
    let element = tree_of(&ui);
    let on_gateway = {
        let view = element.borrow();
        let tree = view.as_any().downcast_ref::<TreeView>().unwrap();
        let origin = view.get_absolute_position();
        let dy = (0..view.get_rect().height())
            .find(|dy| {
                tree.key_at(Point::new(origin.x + 40, origin.y + dy))
                    .as_deref()
                    == Some("Local/gateway")
            })
            .expect("gateway is on screen");
        Point::new(origin.x + 40, origin.y + dy + 1)
    };
    ui.on_mouse_button_down(on_gateway, MouseButton::Right);

    let view = element.borrow();
    let tree = view.as_any().downcast_ref::<TreeView>().unwrap();
    assert_eq!(
        tree.selected_key().as_deref(),
        Some("Local/gateway"),
        "the press did not move the selection to the row under it"
    );
    // And the release is still on the same row, which is what lets the caller
    // decide the menu is about it.
    assert_eq!(tree.key_at(on_gateway).as_deref(), Some("Local/gateway"));
}
