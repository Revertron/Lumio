//! Malformed-XML robustness: `UI::from_xml` must return `None` on bad input
//! instead of panicking — untrusted layout XML must not be able to crash the
//! host — while valid layouts (including a self-closing root) still parse.

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

fn parse(xml: &str) -> Option<UI> {
    set_provider(Box::new(Provider { dir: ASSETS }));
    UI::from_xml(xml, 800, 600, default_typeface(), 1.0)
}

#[test]
fn malformed_attribute_returns_none() {
    // Attribute without `="value"` used to panic in the attribute unwrap
    assert!(parse(r#"<Frame width></Frame>"#).is_none());
    assert!(parse(r#"<Frame><Button text/></Frame>"#).is_none());
    assert!(parse(r#"<Frame width=max></Frame>"#).is_none());
}

#[test]
fn unknown_view_type_returns_none() {
    assert!(parse(r#"<Bogus/>"#).is_none());
    assert!(parse(r#"<Frame><Bogus/></Frame>"#).is_none());
}

#[test]
fn unbalanced_tags_return_none() {
    assert!(parse(r#"<Frame>"#).is_none());
    assert!(parse(r#"</Frame>"#).is_none());
    assert!(parse(r#"<Frame></Button>"#).is_none());
    assert!(parse(r#"<Frame><Button text="x"></Frame>"#).is_none());
}

#[test]
fn child_inside_non_container_does_not_panic() {
    // Button is not a container; the stray child is dropped, not a panic
    assert!(parse(r#"<Button text="x"><Label text="y"/></Button>"#).is_some());
}

#[test]
fn valid_layout_still_parses() {
    let ui = parse(
        r#"<Frame id="root" width="max" height="max">
               <Button id="btn" text="Click"/>
           </Frame>"#,
    )
    .unwrap();
    assert!(ui.get_view("btn").is_some());
}

#[test]
fn self_closing_root_parses() {
    // A self-closing root element used to panic on an empty parent stack
    let ui = parse(r#"<Frame id="root" width="max" height="max"/>"#).unwrap();
    assert!(ui.get_view("root").is_some());
}
