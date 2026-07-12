//! Manual screen-reader test bed for the AccessKit integration. Run it with
//! Narrator (Win+Ctrl+Enter) or NVDA active and click through the widgets:
//! each should be announced with its role, label and state. Inspect the UIA
//! tree with Accessibility Insights for Windows to verify structure and
//! bounds (also on a monitor scaled above 100%).

#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 700;
const HEIGHT: u32 = 560;
const TITLE: &str = "Accessibility Example";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|file| file.contents())
    }
}

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="horizontal" padding="16" font="Noto Sans" font_style="Regular">
    <Frame id="col_left" width="300" height="max" direction="vertical">
        <Label id="title" text="Screen reader test bed"/>
        <Button id="btn" text="Ordinary button" tooltip="Fires a click" margin_top="8"/>
        <Button id="btn_off" text="Disabled button" enabled="false" margin_top="8"/>
        <CheckBox id="check" text="Notify me" checked="true" margin_top="8"/>
        <Label id="edit_label" text="Your name:" margin_top="8"/>
        <Edit id="edit" text="Jane" width="240" labelled_by="edit_label"/>
        <Label id="pass_label" text="Password (must stay silent):" margin_top="8"/>
        <Edit id="pass" text="hunter2" password="true" width="240"/>
        <Label id="slider_label" text="Volume:" margin_top="8"/>
        <Slider id="slider" min="0" max="100" value="30" step="10" width="240" label_style="ends"/>
        <RadioButton id="radio_a" text="Small" group="size" checked="true" margin_top="8"/>
        <RadioButton id="radio_b" text="Large" group="size"/>
        <Label id="status" text="Nothing clicked yet" margin_top="12"/>
    </Frame>
    <Frame id="col_right" width="max" height="max" direction="vertical" margin_left="16">
        <ComboBox id="combo" width="160" items="Red|Green|Blue" selected="1"/>
        <List id="list" width="200" height="80" margin_top="8"/>
        <TabView id="tabs" width="320" height="90" margin_top="8">
            <Frame id="general"><Label text="General settings"/></Frame>
            <Frame id="advanced"><Label text="Advanced settings"/></Frame>
        </TabView>
        <ProgressBar id="progress" value="0.4" width="240" margin_top="8"/>
        <TableView id="table" width="320" height="120" margin_top="8"/>
        <Memo id="memo" text="First line&#10;Second line" width="320" height="80" margin_top="8"/>
    </Frame>
</Frame>
"#;

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));

    let mut ui = UI::from_xml(LAYOUT, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    // List items need the laid-out font, so fill them after the first layout.
    ui.layout(WIDTH, HEIGHT, 1.0);
    if let Some(list) = ui.get_view("list")
        && let Some(l) = list.borrow_mut().downcast_mut::<List>()
    {
        l.set_items(vec!["Alpha".into(), "Beta".into(), "Gamma".into()]);
        l.select_item(0);
    }
    if let Some(table) = ui.get_view("table")
        && let Some(t) = table.borrow().as_any().downcast_ref::<TableView>()
    {
        t.set_data(
            vec!["Name".into(), "Age".into()],
            vec![
                vec!["Ann".into(), "30".into()],
                vec!["Bob".into(), "25".into()],
            ],
        );
        t.select_row(0);
    }

    // Live state changes: a screen reader should pick these up on the next
    // focus/inspection since every redraw re-pushes the accessibility tree.
    if let Some(btn) = ui.get_view("btn") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            if let Some(lbl) = ui.get_view("status")
                && let Some(l) = lbl.borrow_mut().downcast_mut::<Label>()
            {
                l.set_text("Button clicked");
            }
            true
        }));
    }
    if let Some(check) = ui.get_view("check") {
        check.borrow_mut().on_event(EventType::CheckedChanged, Box::new(|ui, _view, data| {
            if let EventData::Checked(on) = data
                && let Some(lbl) = ui.get_view("status")
                && let Some(l) = lbl.borrow_mut().downcast_mut::<Label>()
            {
                l.set_text(if *on { "Checked" } else { "Unchecked" });
            }
            true
        }));
    }

    // Logical size: the window matches the dip-designed layout on any monitor scale.
    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).logical_size().center());
}
