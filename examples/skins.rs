#![windows_subsystem = "windows"]
//! Skins demo: register a custom skin and switch skins at runtime.
//!
//! Skins bundle a palette with the drawable *forms*. Two skins ship built in —
//! `"light"` and `"dark"` — and apps can register their own with
//! [`register_skin`], overriding only the roles they want (here `button.back`).
//! The button cycles `light -> dark -> flat` via [`UI::set_skin`], applied
//! before the next paint.
//!
//!   cargo run --example skins
//!   cargo run --example skins --no-default-features --features backend-software

use std::cell::Cell;
use std::rc::Rc;

use lumio::prelude::*;

const WIDTH: u32 = 460;
const HEIGHT: u32 = 340;

/// The rotation the "cycle" button walks through.
const SKINS: [&str; 3] = ["light", "dark", "flat"];

// The "flat" skin, written as an XML manifest (`Skin::from_xml`): a dark base
// whose `button.back` is a flat fill in the palette's progress-fill accent
// instead of the classic 3D gray. Every other role falls back to the base form.
const FLAT_SKIN: &str = r#"
<skin name="flat" base="dark">
    <drawable role="button.back">
        <selector><item><layer-list><item>
            <shape type="rect"><solid color="@progress_fill"/></shape>
        </item></layer-list></item></selector>
    </drawable>
</skin>
"#;

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical" padding="16">
    <Label text="Skins demo" font_size="22"/>
    <Label text="The button below cycles the window's skin."/>
    <Button id="cycle" text="Skin: light  (click to cycle)" width="260"/>
    <Edit text="Editable field" width="260"/>
    <CheckBox text="A checkbox" checked="true"/>
    <RadioButton text="A radio button" checked="true"/>
    <ProgressBar progress="60" width="260"/>
</Frame>
"#;

fn main() {
    // Register a custom "flat" skin (dark palette, one overridden form) so it is
    // selectable by name alongside the built-in "light"/"dark".
    register_skin(Skin::from_xml(FLAT_SKIN).expect("valid skin manifest"));

    let ui = UI::from_xml(LAYOUT, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    if let Some(button) = ui.get_view("cycle") {
        // The rotation index lives in the closure (Cell = interior mutability so
        // the handler stays `Fn`).
        let index = Rc::new(Cell::new(0usize));
        button.borrow_mut().on_event(
            EventType::Click,
            Box::new(move |ui, view, _data| {
                let next = (index.get() + 1) % SKINS.len();
                index.set(next);
                let name = SKINS[next];
                ui.set_skin(name);
                if let Some(btn) = view.as_any().downcast_ref::<Button>() {
                    btn.set_text(&format!("Skin: {name}  (click to cycle)"));
                }
                true
            }),
        );
    }

    lumio::run(ui, WindowConfig::new("Skins Demo", WIDTH, HEIGHT).center().skin("light"));
}
