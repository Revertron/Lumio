#![windows_subsystem = "windows"]

//! `ProgressCircle` demo: determinate rings driven by a slider (watch them ease
//! toward the value), indeterminate comet spinners at several sizes, and a
//! button that flips one ring between the two modes.

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 620;
const HEIGHT: u32 = 1600;
const TITLE: &str = "ProgressCircle Example";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|file| file.contents())
    }
}

/// Ids of the rings the slider drives.
const RINGS: [&str; 4] = ["ring_small", "ring_mid", "ring_big", "ring_custom"];

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));

    let layout = include_str!("progress_circle_example.xml");
    let ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    // Slider drives every determinate ring; `set_value` only sets the target,
    // the rings animate their way there.
    if let Some(slider) = ui.get_view("drive") {
        slider.borrow_mut().on_event(EventType::ValueChanged, Box::new(|ui, _view, data| {
            if let EventData::Value(v) = data {
                for id in RINGS {
                    if let Some(ring) = ui.get_view(id)
                        && let Some(circle) = ring.borrow().downcast_ref::<ProgressCircle>()
                    {
                        circle.set_value(*v / 100.0);
                    }
                }
            }
            true
        }));
    }

    // Flip the small ring between "busy" (indeterminate) and a fixed value.
    if let Some(button) = ui.get_view("busy") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            if let Some(ring) = ui.get_view("busy_ring")
                && let Some(circle) = ring.borrow().downcast_ref::<ProgressCircle>()
            {
                let busy = circle.is_indeterminate();
                circle.set_indeterminate(!busy);
                if busy {
                    circle.set_value_now(0.65);
                }
            }
            true
        }));
    }

    // Dark-mode toggle to check both palettes.
    if let Some(check) = ui.get_view("dark_mode") {
        check.borrow_mut().on_event(EventType::CheckedChanged, Box::new(|ui, view, _data| {
            let on = view.get_state().map(|s| s.checked).unwrap_or(false);
            ui.set_palette(if on { Palette::dark() } else { Palette::classic() });
            true
        }));
    }

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
