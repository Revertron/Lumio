//! Headless tests for the TreeView and IconList widgets: software render
//! smoke test plus synthetic-dispatch interaction tests (chevron lazy-load
//! handshake, multi-select with Ctrl/Shift via `UI::set_modifiers`).
#![cfg(feature = "backend-software")]

use std::cell::RefCell;
use std::rc::Rc;

use include_dir::{Dir, include_dir};

use lumio::drawing::{DrawableRegistry, Palette, set_current_palette};
use lumio::prelude::*;
use lumio::render::render_to_pixmap;

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
<Frame id="root" width="max" height="max" direction="horizontal" font="Noto Sans" font_style="Regular">
    <TreeView id="tree" width="250" height="max"/>
    <IconList id="files" width="max" height="max"/>
</Frame>
"#;

fn build_ui() -> UI {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let mut ui = UI::from_xml(LAYOUT, 800, 400, default_typeface(), 1.0).unwrap();

    if let Some(tree) = ui.get_view("tree") {
        let tree = tree.borrow();
        let tv = tree.as_any().downcast_ref::<TreeView>().unwrap();
        tv.set_roots(vec![
            TreeNode {
                text: "root".into(),
                key: "root".into(),
                icon: Some("icons/folder.svg".into()),
                has_children: true,
                expanded: true,
                children: vec![
                    TreeNode::new("branch", "branch", true), // children unknown (lazy)
                    TreeNode::new("leafless", "leafless", true), // will expand empty
                ],
                ..TreeNode::default()
            },
        ]);
    }
    if let Some(files) = ui.get_view("files") {
        let files = files.borrow();
        let il = files.as_any().downcast_ref::<IconList>().unwrap();
        let items = (0..40)
            .map(|i| IconListItem::new(&format!("file-{i:02}.txt"), "icons/file-outline.svg", 0xFFB0B0B0, &format!("f{i}")))
            .collect();
        il.set_items(items);
    }

    ui.layout(800, 400, 1.0);
    ui
}

/// Widget-rect points for every IconList item index, found through the public
/// hit-test (no private geometry needed).
fn icon_item_points(ui: &UI) -> Vec<Point<i32>> {
    let files = ui.get_view("files").unwrap();
    let files = files.borrow();
    let il = files.as_any().downcast_ref::<IconList>().unwrap();
    let rect = il.get_rect();
    let n = il.item_count();
    let mut points = vec![None; n];
    for y in (rect.min.y..rect.max.y).step_by(3) {
        for x in (rect.min.x..rect.max.x).step_by(3) {
            if let Some(idx) = il.item_at(x, y) {
                if points[idx].is_none() {
                    points[idx] = Some(Point::new(x, y));
                }
            }
        }
    }
    points.into_iter().map(|p| p.expect("item not hit-testable")).collect()
}

#[test]
fn renders_non_blank() {
    let palette = Palette::classic();
    set_current_palette(palette.clone());
    let registry = DrawableRegistry::new();

    let ui = build_ui();
    let pixmap = render_to_pixmap(&ui, 800, 400, 1.0, &palette, &registry).expect("pixmap");
    let data = pixmap.data();
    let first = [data[0], data[1], data[2], data[3]];
    let drew_something = data.chunks_exact(4).any(|px| px != first);
    assert!(drew_something, "rendered pixmap is uniformly blank — nothing drew");
}

#[test]
fn chevron_click_runs_lazy_load_handshake() {
    let mut ui = build_ui();
    let expanded_log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let log = Rc::clone(&expanded_log);
        let tree = ui.get_view("tree").unwrap();
        tree.borrow_mut().on_event(EventType::Expanded, Box::new(move |_ui, view, _data| {
            let tv = view.as_any().downcast_ref::<TreeView>().unwrap();
            let key = tv.expanded_key().unwrap();
            log.borrow_mut().push(key.clone());
            if key == "branch" {
                tv.set_children(&key, vec![
                    TreeNode::new("child-a", "child-a", false),
                    TreeNode::new("child-b", "child-b", false),
                ]);
            }
            true
        }));
    }

    let tree = ui.get_view("tree").unwrap();
    let (rect, row_h, before) = {
        let tree = tree.borrow();
        let tv = tree.as_any().downcast_ref::<TreeView>().unwrap();
        let rect = tv.get_rect();
        // Row height isn't public; recover it by hit-scanning is overkill —
        // the default is font line (16) vs icon (16) + 2*3 pad = 22 at scale 1.
        (rect, 22, tv.visible_count())
    };
    assert_eq!(before, 3); // root, branch, leafless

    // "branch" is flat row 1 at depth 1: chevron zone x = inset + 1*indent .. +indent.
    let click = Point::new(rect.min.x + 2 + 16 + 8, rect.min.y + 2 + row_h + row_h / 2);
    ui.on_mouse_button_down(click, MouseButton::Left);
    ui.on_mouse_button_up(click, MouseButton::Left);

    assert_eq!(expanded_log.borrow().as_slice(), ["branch"]);
    {
        let tree = tree.borrow();
        let tv = tree.as_any().downcast_ref::<TreeView>().unwrap();
        assert_eq!(tv.visible_count(), 5, "lazy-loaded children did not appear");
    }

    // Expanding a node whose handler supplies nothing clears its chevron.
    let click = Point::new(rect.min.x + 2 + 16 + 8, rect.min.y + 2 + 4 * row_h + row_h / 2);
    ui.on_mouse_button_down(click, MouseButton::Left);
    ui.on_mouse_button_up(click, MouseButton::Left);
    {
        let tree = tree.borrow();
        let tv = tree.as_any().downcast_ref::<TreeView>().unwrap();
        assert_eq!(expanded_log.borrow().as_slice(), ["branch", "leafless"]);
        assert!(!tv.node("leafless").unwrap().has_children, "empty expand kept the chevron");
        assert_eq!(tv.visible_count(), 5);
    }
}

#[test]
fn tree_row_click_selects_and_fires() {
    let mut ui = build_ui();
    let log: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let log = Rc::clone(&log);
        let tree = ui.get_view("tree").unwrap();
        tree.borrow_mut().on_event(EventType::SelectionChanged, Box::new(move |_ui, _view, data| {
            if let EventData::Selected(i) = data {
                log.borrow_mut().push(*i);
            }
            true
        }));
    }

    let tree = ui.get_view("tree").unwrap();
    let rect = tree.borrow().get_rect();
    // Row 1 ("branch"), well right of the chevron zone.
    let click = Point::new(rect.min.x + 2 + 2 * 16 + 30, rect.min.y + 2 + 22 + 11);
    ui.on_mouse_button_down(click, MouseButton::Left);
    ui.on_mouse_button_up(click, MouseButton::Left);

    let tree = tree.borrow();
    let tv = tree.as_any().downcast_ref::<TreeView>().unwrap();
    assert_eq!(tv.selected_key().as_deref(), Some("branch"));
    assert_eq!(log.borrow().as_slice(), [1]);
}

#[test]
fn icon_list_multi_select() {
    let mut ui = build_ui();
    let points = icon_item_points(&ui);
    assert!(points.len() >= 10);

    let selected = |ui: &UI| -> Vec<usize> {
        let files = ui.get_view("files").unwrap();
        let files = files.borrow();
        files.as_any().downcast_ref::<IconList>().unwrap().selected_indices()
    };

    // Plain click selects one.
    ui.on_mouse_button_down(points[0], MouseButton::Left);
    ui.on_mouse_button_up(points[0], MouseButton::Left);
    assert_eq!(selected(&ui), vec![0]);

    // Ctrl+Click adds.
    ui.set_modifiers(ModifiersState::new(true, false, false, false));
    ui.on_mouse_button_down(points[4], MouseButton::Left);
    ui.on_mouse_button_up(points[4], MouseButton::Left);
    assert_eq!(selected(&ui), vec![0, 4]);

    // Shift+Click selects the range from the ctrl-click anchor.
    ui.set_modifiers(ModifiersState::new(false, false, true, false));
    ui.on_mouse_button_down(points[7], MouseButton::Left);
    ui.on_mouse_button_up(points[7], MouseButton::Left);
    assert_eq!(selected(&ui), vec![4, 5, 6, 7]);

    // Plain click collapses back to one.
    ui.set_modifiers(ModifiersState::default());
    ui.on_mouse_button_down(points[2], MouseButton::Left);
    ui.on_mouse_button_up(points[2], MouseButton::Left);
    assert_eq!(selected(&ui), vec![2]);
}
