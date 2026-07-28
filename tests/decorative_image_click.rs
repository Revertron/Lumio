//! An image inside a click target must not eat the click. A tile of icon plus
//! caption is one button as far as the user is concerned, and the listener sits
//! on the Frame around them — so an `ImageView` with nothing listening to it has
//! to let the press and the release fall through to its parent.
//!
//! Regression test: `ImageView` used to capture every press and consume the
//! matching release whether or not anyone was listening, which left the parent's
//! `Click` firing only on the padding between the picture and the text.

use std::cell::Cell;
use std::rc::Rc;

use lumio::input::MouseButton;
use lumio::prelude::*;

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical">
    <Frame id="tile" width="min" height="min" direction="vertical" padding="10">
        <ImageView id="icon" width="56" height="56"/>
        <Label id="caption" width="min" text="baboon"/>
    </Frame>
</Frame>
"#;

fn centre_of(ui: &UI, id: &str) -> Point<i32> {
    let element = ui.get_view(id).unwrap();
    let view = element.borrow();
    let origin = view.get_absolute_position();
    let rect = view.get_rect();
    assert!(rect.width() > 0 && rect.height() > 0, "{id} was measured");
    Point::new(
        origin.x + rect.width() / 2,
        origin.y + rect.height() / 2,
    )
}

#[test]
fn a_click_on_the_picture_reaches_the_tile_around_it() {
    let mut ui = UI::from_xml(LAYOUT, 400, 300, Typeface::default(), 1.0).unwrap();
    let clicks = Rc::new(Cell::new(0u32));

    let tile = ui.get_view("tile").unwrap();
    let counter = Rc::clone(&clicks);
    tile.borrow_mut().on_event(
        EventType::Click,
        Box::new(move |_ui, _view, _data| {
            counter.set(counter.get() + 1);
            true
        }),
    );
    ui.layout(400, 300, 1.0);

    // Over the picture: the part a user aims at, and the part that used to be
    // dead.
    let on_icon = centre_of(&ui, "icon");
    ui.on_mouse_button_down(on_icon, MouseButton::Left);
    ui.on_mouse_button_up(on_icon, MouseButton::Left);
    assert_eq!(clicks.get(), 1, "the picture swallowed the click");

    // Over the caption, which never captured a plain click to begin with.
    let on_caption = centre_of(&ui, "caption");
    ui.on_mouse_button_down(on_caption, MouseButton::Left);
    ui.on_mouse_button_up(on_caption, MouseButton::Left);
    assert_eq!(clicks.get(), 2, "the caption swallowed the click");
}

#[test]
fn an_image_that_is_listened_to_still_takes_its_own_click() {
    let mut ui = UI::from_xml(LAYOUT, 400, 300, Typeface::default(), 1.0).unwrap();
    let clicks = Rc::new(Cell::new(0u32));

    let icon = ui.get_view("icon").unwrap();
    let counter = Rc::clone(&clicks);
    icon.borrow_mut().on_event(
        EventType::Click,
        Box::new(move |_ui, _view, _data| {
            counter.set(counter.get() + 1);
            true
        }),
    );
    ui.layout(400, 300, 1.0);

    let on_icon = centre_of(&ui, "icon");
    ui.on_mouse_button_down(on_icon, MouseButton::Left);
    ui.on_mouse_button_up(on_icon, MouseButton::Left);
    assert_eq!(clicks.get(), 1, "an image with a listener is still clickable");
}
