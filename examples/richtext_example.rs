#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 720;
const HEIGHT: u32 = 600;
const TITLE: &str = "Lumio — RichText demo";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider { dir: Dir<'static> }
impl Provider { fn new(dir: Dir<'static>) -> Self { Self { dir } } }
impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|f| f.contents())
    }
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("richtext_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    // Demonstrate the programmatic builder API too: append a 4th block at runtime
    // (equivalent to what the HTML parser produces).
    if let Some(root) = ui.get_view("root") {
        let mut rt = RichText::default();
        rt.push("Built in code: ", SpanStyle::default());
        rt.push("bold", SpanStyle::default().bold());
        rt.push(" + ", SpanStyle::default());
        rt.push("green", SpanStyle::default().color(0xFF22AA55));
        rt.push(" + ", SpanStyle::default());
        rt.push_link("a link", "https://example.org/builder");
        rt.push(".", SpanStyle::default());
        rt.set_width(Dimension::Max);
        rt.set_height(Dimension::Min);
        rt.set_id("rt4");
        let element: Element = std::rc::Rc::new(std::cell::RefCell::new(rt));
        element.borrow().set_parent(Some(std::rc::Rc::downgrade(&root)));
        if let Some(frame) = root.borrow_mut().downcast_mut::<Frame>() {
            frame.add_view(element);
        }
    }

    // Wire link clicks: each RichText reports the clicked href via clicked_href().
    for id in ["rt1", "rt2", "rt3", "rt4"] {
        if let Some(el) = ui.get_view(id) {
            el.borrow_mut().on_event(EventType::Click, Box::new(|ui, view, _data| {
                let href = view.as_any().downcast_ref::<RichText>().and_then(|rt| rt.clicked_href());
                if let Some(status) = ui.get_view("status")
                    && let Some(label) = status.borrow_mut().downcast_mut::<Label>()
                {
                    match href {
                        Some(h) => label.set_text(&format!("Clicked link: {}", h)),
                        None => label.set_text("Clicked (no href)"),
                    }
                }
                true
            }));
        }
    }

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}
