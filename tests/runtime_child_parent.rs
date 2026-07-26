//! A view added to a container at runtime must end up linked to it, the same as
//! one built from XML. Regression test: `Container::add_view` cannot set that
//! link itself — it has no handle to the `Rc` the container lives in — so the
//! link is made while laying out instead. Without it a runtime-added view has no
//! ancestors to walk, reports the position it has *inside its parent* as though
//! that were the window's origin, and every coordinate mapped onto it (hit
//! tests, `TermGrid::cell_at`) misses by however far the parent sits from the
//! window's edge.

use std::cell::RefCell;
use std::rc::Rc;

use lumio::prelude::*;
use lumio::views::Label;

const LAYOUT: &str = r#"
<Frame id="root" width="max" height="max" direction="vertical">
    <SplitPanel id="split" width="max" height="max" direction="horizontal" split_pos="300">
        <Frame id="left" width="max" height="max" direction="vertical"/>
        <Frame id="right" width="max" height="max" direction="vertical"/>
    </SplitPanel>
</Frame>
"#;

#[test]
fn a_view_added_at_runtime_knows_where_it_sits() {
    let mut ui = UI::from_xml(LAYOUT, 800, 600, Typeface::default(), 1.0).unwrap();
    ui.layout(800, 600, 1.0);

    // Straight into the container, which is all an embedder has to go on.
    let added: Element = Rc::new(RefCell::new(Label::default()));
    added.borrow_mut().set_id("added");
    let right = ui.get_view("right").unwrap();
    right.borrow_mut().as_container_mut().unwrap().add_view(Rc::clone(&added));
    ui.force_layout();

    let parent = added.borrow().get_parent().map(|p| p.borrow().get_id());
    assert_eq!(parent.as_deref(), Some("right"), "the child was linked to its container");

    // The right-hand pane starts at the split, so anything in it does too.
    let origin = added.borrow().get_absolute_position();
    assert!(
        origin.x >= 300,
        "it reports its place in the window, not in its parent: {origin:?}"
    );
}
