//! Headless tests for the accessibility tree builder: `build_tree` is a pure
//! function of a laid-out `UI`, so roles, labels, states, focus and bounds can
//! be asserted without a window or a screen reader (same synthetic-dispatch
//! pattern as tests/event_system.rs).

use include_dir::{Dir, include_dir};

use lumio::accessibility::{ROOT_NODE_ID, build_tree, item_node_id, node_id_for, perform_action};
use lumio::accesskit::{Action, ActionData, ActionRequest, Node, NodeId, Role, Toggled, TreeId, TreeUpdate};
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
    <Label id="label1" text="Hello label"/>
    <Button id="btn1" text="Click me" tooltip="Does a thing"/>
    <Button id="btn_off" text="Disabled" enabled="false"/>
    <CheckBox id="check1" text="Check me" checked="true"/>
    <Edit id="edit1" text="hello" width="200"/>
    <Edit id="pass1" text="secret" password="true" width="200"/>
    <Slider id="slider1" min="0" max="100" value="40" step="5" width="200"/>
    <Label id="ghost" text="invisible" visibility="gone"/>
</Frame>
"#;

fn build_test_ui(layout: &str) -> UI {
    set_provider(Box::new(Provider { dir: ASSETS }));
    let mut ui = UI::from_xml(layout, 800, 600, default_typeface(), 1.0).unwrap();
    ui.layout(800, 600, 1.0);
    ui
}

/// The node emitted for the view with the given string id, if any.
fn find<'a>(update: &'a TreeUpdate, id: &str) -> Option<&'a Node> {
    let node_id = node_id_for(id);
    update.nodes.iter().find(|(i, _)| *i == node_id).map(|(_, n)| n)
}

#[test]
fn window_root_wraps_the_view_tree() {
    let ui = build_test_ui(LAYOUT);
    let update = build_tree(&ui);

    let (_, window) = update.nodes.iter().find(|(i, _)| *i == ROOT_NODE_ID)
        .expect("synthetic window root missing");
    assert_eq!(window.role(), Role::Window);
    assert_eq!(window.children().len(), 1, "one top-level child (the root Frame)");

    let tree = update.tree.expect("full tree descriptor expected");
    assert_eq!(tree.root, ROOT_NODE_ID);

    // Every child reference points at an emitted node.
    for (_, node) in &update.nodes {
        for child in node.children() {
            assert!(update.nodes.iter().any(|(i, _)| i == child), "dangling child id");
        }
    }
}

#[test]
fn widget_roles_labels_and_states() {
    let ui = build_test_ui(LAYOUT);
    let update = build_tree(&ui);

    let label = find(&update, "label1").expect("label node");
    assert_eq!(label.role(), Role::Label);
    // Static text carries its content in `value` (AccessKit's
    // `label_comes_from_value` convention for Role::Label).
    assert_eq!(label.value(), Some("Hello label"));

    let button = find(&update, "btn1").expect("button node");
    assert_eq!(button.role(), Role::Button);
    assert_eq!(button.label(), Some("Click me"));
    assert_eq!(button.description(), Some("Does a thing"), "tooltip becomes the description");
    assert!(!button.is_disabled());

    let disabled = find(&update, "btn_off").expect("disabled button node");
    assert!(disabled.is_disabled());

    let checkbox = find(&update, "check1").expect("checkbox node");
    assert_eq!(checkbox.role(), Role::CheckBox);
    assert_eq!(checkbox.label(), Some("Check me"));
    assert_eq!(checkbox.toggled(), Some(Toggled::True));

    let edit = find(&update, "edit1").expect("edit node");
    assert_eq!(edit.role(), Role::TextInput);
    assert_eq!(edit.value(), Some("hello"));

    let slider = find(&update, "slider1").expect("slider node");
    assert_eq!(slider.role(), Role::Slider);
    assert_eq!(slider.numeric_value(), Some(40.0));
    assert_eq!(slider.min_numeric_value(), Some(0.0));
    assert_eq!(slider.max_numeric_value(), Some(100.0));
    assert_eq!(slider.numeric_value_step(), Some(5.0));
}

#[test]
fn password_field_exposes_no_value() {
    let ui = build_test_ui(LAYOUT);
    let update = build_tree(&ui);

    let password = find(&update, "pass1").expect("password node");
    assert_eq!(password.role(), Role::PasswordInput);
    assert_eq!(password.value(), None, "password text must never reach the accessibility tree");
}

#[test]
fn hidden_views_are_skipped() {
    let ui = build_test_ui(LAYOUT);
    let update = build_tree(&ui);
    assert!(find(&update, "ghost").is_none(), "visibility=gone view must not be exposed");
}

#[test]
fn focus_follows_the_focused_view() {
    let mut ui = build_test_ui(LAYOUT);

    // Nothing focused yet: focus falls back to the window root.
    let update = build_tree(&ui);
    assert_eq!(update.focus, ROOT_NODE_ID);

    // Click into the Edit; sync_focus runs inside the dispatch.
    let edit = ui.get_view("edit1").unwrap();
    let rect = edit.borrow().get_rect();
    let center = Point::new((rect.min.x + rect.max.x) / 2, (rect.min.y + rect.max.y) / 2);
    ui.on_mouse_button_down(center, MouseButton::Left);
    ui.on_mouse_button_up(center, MouseButton::Left);

    let update = build_tree(&ui);
    assert_eq!(update.focus, node_id_for("edit1"));
}

#[test]
fn bounds_are_absolute_window_coordinates() {
    let ui = build_test_ui(LAYOUT);
    let update = build_tree(&ui);

    // The root Frame sits at the window origin, so its children's rects are
    // already window coordinates.
    let view = ui.get_view("btn1").unwrap();
    let rect = view.borrow().get_rect();
    let bounds = find(&update, "btn1").unwrap().bounds().expect("bounds set");
    assert_eq!(bounds.x0, rect.min.x as f64);
    assert_eq!(bounds.y0, rect.min.y as f64);
    assert_eq!(bounds.x1, rect.max.x as f64);
    assert_eq!(bounds.y1, rect.max.y as f64);
    assert!(bounds.x1 > bounds.x0 && bounds.y1 > bounds.y0, "degenerate bounds — layout failed");
}

const PHASE_B_LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <RadioButton id="radio1" text="Option A" group="g" checked="true"/>
    <Memo id="memo1" text="line one" width="200"/>
    <ComboBox id="combo1" width="150">
        <Item text="Red"/>
        <Item text="Green"/>
    </ComboBox>
    <ProgressBar id="prog1" value="0.5" width="200"/>
    <List id="list1" width="200" height="150"/>
    <TabView id="tabs1" width="300" height="150">
        <Frame id="tab_a"><Label id="lbl_a" text="A content"/></Frame>
        <Frame id="tab_b"><Label id="lbl_b" text="B content"/></Frame>
    </TabView>
    <Separator id="sep1"/>
    <ImageView id="img_plain" width="32" height="32"/>
    <ImageView id="img_desc" width="32" height="32" content_description="Company logo"/>
    <Button id="btn_cd" text="visual text" content_description="Spoken label"/>
    <RichText id="rich1">plain <b>bold</b> text</RichText>
    <MenuBar id="menubar1"/>
</Frame>
"#;

#[test]
fn phase_b_widget_roles_and_states() {
    let ui = build_test_ui(PHASE_B_LAYOUT);
    let update = build_tree(&ui);

    let radio = find(&update, "radio1").expect("radio node");
    assert_eq!(radio.role(), Role::RadioButton);
    assert_eq!(radio.label(), Some("Option A"));
    assert_eq!(radio.toggled(), Some(Toggled::True));

    let memo = find(&update, "memo1").expect("memo node");
    assert_eq!(memo.role(), Role::MultilineTextInput);
    assert_eq!(memo.value(), Some("line one"));
    assert!(!memo.is_read_only());

    let combo = find(&update, "combo1").expect("combobox node");
    assert_eq!(combo.role(), Role::ComboBox);
    assert_eq!(combo.is_expanded(), Some(false));

    let progress = find(&update, "prog1").expect("progress node");
    assert_eq!(progress.role(), Role::ProgressIndicator);
    assert_eq!(progress.numeric_value(), Some(0.5));

    let rich = find(&update, "rich1").expect("richtext node");
    assert_eq!(rich.role(), Role::Label);
    let text = rich.value().expect("plain text value");
    assert!(text.contains("bold"), "markup should be stripped, got {text:?}");
}

#[test]
fn content_description_overrides_the_label() {
    let ui = build_test_ui(PHASE_B_LAYOUT);
    let update = build_tree(&ui);
    let button = find(&update, "btn_cd").expect("button node");
    assert_eq!(button.label(), Some("Spoken label"));
}

#[test]
fn decorative_views_are_skipped() {
    let ui = build_test_ui(PHASE_B_LAYOUT);
    let update = build_tree(&ui);
    assert!(find(&update, "sep1").is_none(), "separator is decorative");
    assert!(find(&update, "img_plain").is_none(), "image without description is decorative");
    let image = find(&update, "img_desc").expect("described image node");
    assert_eq!(image.role(), Role::Image);
    assert_eq!(image.label(), Some("Company logo"));
}

#[test]
fn list_exposes_synthetic_options() {
    let ui = build_test_ui(PHASE_B_LAYOUT);
    {
        let list = ui.get_view("list1").unwrap();
        let mut list = list.borrow_mut();
        let list = list.downcast_mut::<List>().unwrap();
        list.set_items(vec!["Alpha".into(), "Beta".into(), "Gamma".into()]);
        list.select_item(1);
    }
    let update = build_tree(&ui);

    let list_node = find(&update, "list1").expect("list node");
    assert_eq!(list_node.role(), Role::ListBox);
    assert_eq!(list_node.children().len(), 3);

    for (i, expected) in ["Alpha", "Beta", "Gamma"].iter().enumerate() {
        let option_id = lumio::accessibility::item_node_id("list1", i);
        let (_, option) = update.nodes.iter().find(|(id, _)| *id == option_id)
            .unwrap_or_else(|| panic!("option {i} missing"));
        assert_eq!(option.role(), Role::ListBoxOption);
        assert_eq!(option.label(), Some(*expected));
        assert_eq!(option.is_selected(), Some(i == 1));
    }
}

#[test]
fn tabview_exposes_tabs_and_only_active_content() {
    let ui = build_test_ui(PHASE_B_LAYOUT);
    let update = build_tree(&ui);

    let tabs = find(&update, "tabs1").expect("tablist node");
    assert_eq!(tabs.role(), Role::TabList);
    // 2 synthetic tabs + the active tab's content subtree.
    assert_eq!(tabs.children().len(), 3);

    let tab0_id = lumio::accessibility::item_node_id("tabs1", 0);
    let (_, tab0) = update.nodes.iter().find(|(id, _)| *id == tab0_id).expect("tab 0");
    assert_eq!(tab0.role(), Role::Tab);
    assert_eq!(tab0.label(), Some("tab_a"), "default tab title is the child id");
    assert_eq!(tab0.is_selected(), Some(true));

    assert!(find(&update, "lbl_a").is_some(), "active tab content exposed");
    assert!(find(&update, "lbl_b").is_none(), "inactive tab content hidden");
}

#[test]
fn menubar_exposes_menu_titles() {
    let ui = build_test_ui(PHASE_B_LAYOUT);
    {
        let bar = ui.get_view("menubar1").unwrap();
        let mut bar = bar.borrow_mut();
        let bar = bar.downcast_mut::<MenuBar>().unwrap();
        bar.add_menu("File", vec![MenuItem { id: "open".into(), icon_path: String::new(), text: "Open".into(), separator: false, children: vec![] }]);
        bar.add_menu("Help", vec![]);
    }
    let update = build_tree(&ui);

    let bar_node = find(&update, "menubar1").expect("menubar node");
    assert_eq!(bar_node.role(), Role::MenuBar);
    assert_eq!(bar_node.children().len(), 2);

    let file_id = lumio::accessibility::item_node_id("menubar1", 0);
    let (_, file) = update.nodes.iter().find(|(id, _)| *id == file_id).expect("File item");
    assert_eq!(file.role(), Role::MenuItem);
    assert_eq!(file.label(), Some("File"));
}

#[test]
fn open_popup_menu_reports_items_and_hover_focus() {
    let mut ui = build_test_ui(PHASE_B_LAYOUT);

    let mut menu = PopupMenu::new();
    menu.set_id("ctx_menu");
    menu.add_item("open", "", "Open");
    menu.add_separator();
    menu.add_item("save", "", "Save");
    let element: std::rc::Rc<std::cell::RefCell<dyn View>> = std::rc::Rc::new(std::cell::RefCell::new(menu));
    ui.show_popup(element, 100, 100, PopupDirection::BottomRight, PopupMode::Popup);

    let update = build_tree(&ui);
    let menu_node = find(&update, "ctx_menu").expect("menu node in overlay");
    assert_eq!(menu_node.role(), Role::Menu);
    // Separator is skipped: 2 items.
    assert_eq!(menu_node.children().len(), 2);
    let open_id = lumio::accessibility::item_node_id("ctx_menu", 0);
    let (_, open) = update.nodes.iter().find(|(id, _)| *id == open_id).expect("Open item");
    assert_eq!(open.label(), Some("Open"));

    // Hovering an item makes it the reported accessibility focus.
    let bounds = open.bounds().expect("item bounds");
    let center = Point::new(
        ((bounds.x0 + bounds.x1) / 2.0) as i32,
        ((bounds.y0 + bounds.y1) / 2.0) as i32,
    );
    ui.on_mouse_move(center);
    let update = build_tree(&ui);
    assert_eq!(update.focus, open_id, "hovered menu item is the AT focus");
}

fn request(action: Action, target: NodeId, data: Option<ActionData>) -> ActionRequest {
    ActionRequest { action, target_tree: TreeId::ROOT, target_node: target, data }
}

#[test]
fn action_click_fires_button_listener() {
    let mut ui = build_test_ui(LAYOUT);
    let clicked = std::rc::Rc::new(std::cell::Cell::new(false));
    {
        let clicked = std::rc::Rc::clone(&clicked);
        let button = ui.get_view("btn1").unwrap();
        button.borrow_mut().on_event(EventType::Click, Box::new(move |_ui, _view, _data| {
            clicked.set(true);
            true
        }));
    }
    let handled = perform_action(&mut ui, &request(Action::Click, node_id_for("btn1"), None));
    assert!(handled);
    assert!(clicked.get(), "AT click must reach the Click listener");
}

#[test]
fn action_click_ignores_disabled_views() {
    let mut ui = build_test_ui(LAYOUT);
    let clicked = std::rc::Rc::new(std::cell::Cell::new(false));
    {
        let clicked = std::rc::Rc::clone(&clicked);
        let button = ui.get_view("btn_off").unwrap();
        button.borrow_mut().on_event(EventType::Click, Box::new(move |_ui, _view, _data| {
            clicked.set(true);
            true
        }));
    }
    assert!(!perform_action(&mut ui, &request(Action::Click, node_id_for("btn_off"), None)));
    assert!(!clicked.get());
}

#[test]
fn action_focus_moves_keyboard_focus() {
    let mut ui = build_test_ui(LAYOUT);
    assert!(perform_action(&mut ui, &request(Action::Focus, node_id_for("edit1"), None)));

    let edit = ui.get_view("edit1").unwrap();
    assert!(edit.borrow().is_focused());
    let update = build_tree(&ui);
    assert_eq!(update.focus, node_id_for("edit1"));

    // Non-focusable targets are refused.
    assert!(!perform_action(&mut ui, &request(Action::Focus, node_id_for("label1"), None)));
}

#[test]
fn action_set_value_drives_the_slider() {
    let mut ui = build_test_ui(LAYOUT);
    let last_value = std::rc::Rc::new(std::cell::Cell::new(-1.0f32));
    {
        let last_value = std::rc::Rc::clone(&last_value);
        let slider = ui.get_view("slider1").unwrap();
        slider.borrow_mut().on_event(EventType::ValueChanged, Box::new(move |_ui, _view, data| {
            if let EventData::Value(v) = data {
                last_value.set(*v);
            }
            true
        }));
    }

    let req = request(Action::SetValue, node_id_for("slider1"), Some(ActionData::NumericValue(70.0)));
    assert!(perform_action(&mut ui, &req));
    assert_eq!(last_value.get(), 70.0, "ValueChanged must fire like the keyboard path");

    // The slider's own clamping/snapping applies (step = 5).
    let req = request(Action::SetValue, node_id_for("slider1"), Some(ActionData::NumericValue(63.0)));
    assert!(perform_action(&mut ui, &req));
    assert_eq!(last_value.get(), 65.0);
}

#[test]
fn action_increment_and_decrement_step_the_slider() {
    let mut ui = build_test_ui(LAYOUT);
    // slider1: min 0, max 100, value 40, step 5.
    assert!(perform_action(&mut ui, &request(Action::Increment, node_id_for("slider1"), None)));
    assert!(perform_action(&mut ui, &request(Action::Decrement, node_id_for("slider1"), None)));
    assert!(perform_action(&mut ui, &request(Action::Decrement, node_id_for("slider1"), None)));

    let slider = ui.get_view("slider1").unwrap();
    let slider = slider.borrow();
    let slider = slider.as_any().downcast_ref::<Slider>().unwrap();
    assert_eq!(slider.get_value(), 35.0);
}

#[test]
fn action_click_selects_a_list_option() {
    let mut ui = build_test_ui(PHASE_B_LAYOUT);
    {
        let list = ui.get_view("list1").unwrap();
        let mut list = list.borrow_mut();
        let list = list.downcast_mut::<List>().unwrap();
        list.set_items(vec!["Alpha".into(), "Beta".into(), "Gamma".into()]);
    }
    assert!(perform_action(&mut ui, &request(Action::Click, item_node_id("list1", 2), None)));

    let list = ui.get_view("list1").unwrap();
    let list = list.borrow();
    let list = list.as_any().downcast_ref::<List>().unwrap();
    assert_eq!(list.get_selected(), Some(2), "AT click on an option selects it");
}

#[test]
fn action_click_switches_tabs() {
    let mut ui = build_test_ui(PHASE_B_LAYOUT);
    assert!(perform_action(&mut ui, &request(Action::Click, item_node_id("tabs1", 1), None)));

    let tabs = ui.get_view("tabs1").unwrap();
    let active = tabs.borrow().as_any().downcast_ref::<TabView>().unwrap().get_active_tab();
    assert_eq!(active, 1);

    let update = build_tree(&ui);
    assert!(find(&update, "lbl_b").is_some(), "newly active tab content exposed");
    assert!(find(&update, "lbl_a").is_none(), "previous tab content hidden");
}

#[test]
fn action_on_unknown_target_is_refused() {
    let mut ui = build_test_ui(LAYOUT);
    assert!(!perform_action(&mut ui, &request(Action::Click, node_id_for("no_such_view"), None)));
}

const PHASE_D_LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <Label id="name_label" text="Name:"/>
    <Edit id="named_edit" text="hi" width="200" labelled_by="name_label"/>
    <Edit id="text_edit" text="hello world" width="200"/>
    <Memo id="memo" text="one&#10;two" width="200"/>
    <TableView id="table" width="300" height="200"/>
    <RecyclerView id="recycler" width="200" height="100"/>
    <StatusBar id="status" width="max"/>
</Frame>
"#;

#[test]
fn labelled_by_references_the_labeling_node() {
    let ui = build_test_ui(PHASE_D_LAYOUT);
    let update = build_tree(&ui);
    let edit = find(&update, "named_edit").expect("edit node");
    assert_eq!(edit.labelled_by(), &[node_id_for("name_label")]);
}

#[test]
fn status_bar_sections_are_live_labels() {
    let ui = build_test_ui(PHASE_D_LAYOUT);
    {
        let bar = ui.get_view("status").unwrap();
        let mut bar = bar.borrow_mut();
        let bar = bar.downcast_mut::<StatusBar>().unwrap();
        bar.add_section("s1", "Ready");
        bar.add_section("s2", "42%");
    }
    let update = build_tree(&ui);

    let status = find(&update, "status").expect("status node");
    assert_eq!(status.role(), Role::Status);
    assert_eq!(status.children().len(), 2);

    let (_, section) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("status", 0)).expect("section 0");
    assert_eq!(section.value(), Some("Ready"));
    assert_eq!(section.live(), Some(lumio::accesskit::Live::Polite));
}

#[test]
fn table_exposes_headers_rows_and_cells() {
    let mut ui = build_test_ui(PHASE_D_LAYOUT);
    {
        let table = ui.get_view("table").unwrap();
        let table = table.borrow();
        let table = table.as_any().downcast_ref::<TableView>().unwrap();
        table.set_data(
            vec!["Name".into(), "Age".into()],
            vec![
                vec!["Ann".into(), "30".into()],
                vec!["Bob".into(), "25".into()],
            ],
        );
        table.select_row(1);
    }
    ui.layout(800, 600, 1.0);
    let update = build_tree(&ui);

    // The table's direct children are the header row + 2 data rows — no
    // loose cells (they are re-parented under their rows).
    let table_node = find(&update, "table").expect("table node");
    assert_eq!(table_node.role(), Role::Table);
    assert_eq!(table_node.children().len(), 3);

    let (_, header_row) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("table", 0)).expect("header row");
    assert_eq!(header_row.role(), Role::Row);
    assert_eq!(header_row.children().len(), 2);

    let (_, name_header) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("table", 1)).expect("Name header");
    assert_eq!(name_header.role(), Role::ColumnHeader);
    assert_eq!(name_header.label(), Some("Name"));

    // Data row ids are keyed by raw index after the header ids (base = 1 + columns).
    let (_, row0) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("table", 3)).expect("row 0");
    assert_eq!(row0.role(), Role::Row);
    assert_eq!(row0.children().len(), 2);
    assert_eq!(row0.is_selected(), Some(false));
    let first_cell = row0.children()[0];
    let (_, cell) = update.nodes.iter().find(|(id, _)| *id == first_cell).expect("cell node");
    assert_eq!(cell.value(), Some("Ann"), "text-mode cells are Labels carrying their text");

    let (_, row1) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("table", 4)).expect("row 1");
    assert_eq!(row1.is_selected(), Some(true));

    // AT click on a sortable header sorts the table and reports the direction.
    assert!(perform_action(&mut ui, &request(Action::Click, item_node_id("table", 1), None)));
    let update = build_tree(&ui);
    let (_, name_header) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("table", 1)).expect("Name header");
    assert_eq!(name_header.sort_direction(), Some(lumio::accesskit::SortDirection::Ascending));
}

#[test]
fn edit_exposes_text_run_with_geometry_and_selection() {
    let ui = build_test_ui(PHASE_D_LAYOUT);
    let run_id = item_node_id("text_edit", 0);

    let update = build_tree(&ui);
    let (_, run) = update.nodes.iter().find(|(id, _)| *id == run_id).expect("text run");
    assert_eq!(run.role(), Role::TextRun);
    assert_eq!(run.value(), Some("hello world"));
    assert_eq!(run.character_lengths().len(), 11);
    assert_eq!(run.word_starts(), &[0, 6]);
    assert_eq!(run.character_positions().map(|p| p.len()), Some(11), "per-char geometry present");

    // Select-all is reflected as an anchor..focus selection over the run.
    {
        let edit = ui.get_view("text_edit").unwrap();
        let edit = edit.borrow();
        edit.as_any().downcast_ref::<lumio::views::Edit>().unwrap().select_all();
    }
    let update = build_tree(&ui);
    let edit_node = find(&update, "text_edit").expect("edit node");
    let selection = edit_node.text_selection().expect("selection reported");
    assert_eq!(selection.anchor.node, run_id);
    assert_eq!(selection.anchor.character_index, 0);
    assert_eq!(selection.focus.node, run_id);
    assert_eq!(selection.focus.character_index, 11);
}

#[test]
fn memo_exposes_one_run_per_line() {
    let ui = build_test_ui(PHASE_D_LAYOUT);
    let update = build_tree(&ui);

    let (_, line0) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("memo", 0)).expect("line 0 run");
    assert_eq!(line0.role(), Role::TextRun);
    assert_eq!(line0.value(), Some("one\n"), "hard break belongs to its line, as one character");
    assert_eq!(line0.character_lengths().len(), 4);

    let (_, line1) = update.nodes.iter()
        .find(|(id, _)| *id == item_node_id("memo", 1)).expect("line 1 run");
    assert_eq!(line1.value(), Some("two"));

    let memo_node = find(&update, "memo").expect("memo node");
    let selection = memo_node.text_selection().expect("caret reported");
    assert_eq!(selection.focus.node, item_node_id("memo", 0), "caret starts on line 0");
    assert_eq!(selection.focus.character_index, 0);
}

/// Regression: Memo cached its line height at the first scale it saw, so a
/// re-layout at another scale (HiDPI monitor) kept half-size selection
/// rects, caret and line hit-testing. The line height must follow the scale.
#[test]
fn memo_line_height_follows_scale_changes() {
    let mut ui = build_test_ui(PHASE_D_LAYOUT);
    let run_id = item_node_id("memo", 0);

    let line_height_at = |update: &TreeUpdate| {
        let (_, run) = update.nodes.iter().find(|(id, _)| *id == run_id).expect("line 0 run");
        let bounds = run.bounds().expect("run bounds");
        bounds.y1 - bounds.y0
    };

    let h1 = line_height_at(&build_tree(&ui));
    ui.layout(800, 600, 2.0);
    let h2 = line_height_at(&build_tree(&ui));

    let ratio = h2 / h1;
    assert!((1.8..=2.2).contains(&ratio),
        "line height must follow the scale: {h1} at 1.0x vs {h2} at 2.0x (ratio {ratio})");
}

#[test]
fn notifications_land_in_a_live_region() {
    let mut ui = build_test_ui(PHASE_D_LAYOUT);
    let mut label = Label::default();
    label.set_text("Download finished");
    let element: std::rc::Rc<std::cell::RefCell<dyn View>> = std::rc::Rc::new(std::cell::RefCell::new(label));
    // show_notification tags the element with the notification id.
    ui.show_notification(element, "note1", None);

    let update = build_tree(&ui);
    let note = find(&update, "note1").expect("notification content exposed");
    assert_eq!(note.value(), Some("Download finished"));
    // Its container (the stack overlay) is a polite live region.
    let (_, stack) = update.nodes.iter()
        .find(|(_, n)| n.children().contains(&node_id_for("note1")))
        .expect("stack node parents the notification");
    assert_eq!(stack.live(), Some(lumio::accesskit::Live::Polite));
}

struct TestAdapter {
    items: Vec<String>,
}

impl RecyclerAdapter for TestAdapter {
    fn get_item_count(&self) -> usize {
        self.items.len()
    }

    fn create_view_holder(&mut self, view_type: i32) -> ViewHolder {
        let element: std::rc::Rc<std::cell::RefCell<dyn View>> =
            std::rc::Rc::new(std::cell::RefCell::new(Label::default()));
        ViewHolder::new(element, view_type)
    }

    fn bind_view_holder(&self, holder: &ViewHolder, position: usize) {
        let mut view = holder.item_view.borrow_mut();
        if let Some(label) = view.downcast_mut::<Label>() {
            label.set_text(&self.items[position]);
        }
    }
}

#[test]
fn recycler_exposes_realized_rows() {
    let mut ui = build_test_ui(PHASE_D_LAYOUT);
    {
        let recycler = ui.get_view("recycler").unwrap();
        let recycler = recycler.borrow();
        let recycler = recycler.as_any().downcast_ref::<RecyclerView>().unwrap();
        recycler.set_adapter(Box::new(TestAdapter {
            items: (0..20).map(|i| format!("Item {i}")).collect(),
        }));
    }
    ui.layout(800, 600, 1.0);
    let update = build_tree(&ui);

    let recycler_node = find(&update, "recycler").expect("recycler node");
    assert_eq!(recycler_node.role(), Role::List);
    assert!(!recycler_node.children().is_empty(), "realized rows are exposed");
    assert_eq!(recycler_node.size_of_set(), Some(20), "total item count reported");

    // The first realized row's label is readable.
    let first = recycler_node.children()[0];
    let (_, row) = update.nodes.iter().find(|(id, _)| *id == first).expect("row node");
    assert_eq!(row.value(), Some("Item 0"));
}

#[test]
fn duplicate_ids_emit_one_node() {
    let layout = r#"
<Frame id="root" width="max" height="max" direction="vertical" font="Noto Sans" font_style="Regular">
    <Label id="dup" text="first"/>
    <Label id="dup" text="second"/>
</Frame>
"#;
    let ui = build_test_ui(layout);
    let update = build_tree(&ui);

    let dup_id = node_id_for("dup");
    let count = update.nodes.iter().filter(|(i, _)| *i == dup_id).count();
    assert_eq!(count, 1, "a TreeUpdate must not contain the same node id twice");
}
