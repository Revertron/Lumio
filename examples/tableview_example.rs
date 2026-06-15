#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 700;
const TITLE: &str = "TableView Example";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>,
}

impl Provider {
    pub fn new(dir: Dir<'static>) -> Self {
        Self { dir }
    }
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        if let Some(file) = self.dir.get_file(path) {
            return Some(file.contents());
        }
        None
    }
}

fn sample_rows() -> Vec<Vec<String>> {
    let pool = [
        ("example.btc",     "active",   "alice",  "2026-05-01", "primary domain"),
        ("dev.btc",         "pending",  "bob",    "2026-05-03", "renewal queued"),
        ("staging.btc",     "active",   "carol",  "2026-04-28", ""),
        ("backup.btc",      "inactive", "dan",    "2026-03-12", "archived"),
        ("vault.btc",       "active",   "eve",    "2026-04-30", "key rotated"),
        ("alpha.btc",       "active",   "frank",  "2026-05-04", ""),
        ("bravo.btc",       "active",   "grace",  "2026-05-02", ""),
        ("charlie.btc",     "pending",  "henry",  "2026-05-05", "awaiting confirmation"),
        ("delta.btc",       "active",   "irene",  "2026-04-29", ""),
        ("echo.btc",        "active",   "jack",   "2026-05-01", "shared"),
        ("foxtrot.btc",     "inactive", "kate",   "2026-02-22", ""),
        ("golf.btc",        "active",   "leo",    "2026-05-06", ""),
        ("hotel.btc",       "pending",  "mary",   "2026-05-04", ""),
        ("india.btc",       "active",   "nick",   "2026-05-03", ""),
        ("juliet.btc",      "active",   "olivia", "2026-04-27", ""),
        ("kilo.btc",        "inactive", "peter",  "2026-01-15", "expired"),
        ("lima.btc",        "active",   "quinn",  "2026-05-05", ""),
        ("mike.btc",        "active",   "ruth",   "2026-05-02", ""),
        ("november.btc",    "pending",  "sam",    "2026-05-06", "auto-renew off"),
        ("oscar.btc",       "active",   "tina",   "2026-04-30", ""),
        ("papa.btc",        "active",   "ulrich", "2026-05-01", ""),
        ("quebec.btc",      "active",   "victor", "2026-05-04", ""),
        ("romeo.btc",       "active",   "wendy",  "2026-04-28", ""),
        ("sierra.btc",      "inactive", "xavier", "2026-03-01", ""),
        ("tango.btc",       "active",   "yara",   "2026-05-03", ""),
        ("uniform.btc",     "active",   "zane",   "2026-05-05", ""),
    ];
    pool.iter()
        .map(|(n, s, o, u, note)| vec![n.to_string(), s.to_string(), o.to_string(), u.to_string(), note.to_string()])
        .collect()
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("tableview_example.xml");
    let mut ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    // Populate the text-mode table programmatically.
    if let Some(domains_view) = ui.get_view("domains") {
        if let Some(table) = domains_view.borrow().downcast_ref::<TableView>() {
            table.set_data(
                vec!["Name".into(), "Status".into(), "Owner".into(), "Updated".into(), "Notes".into()],
                sample_rows(),
            );
        }
    }

    // Wire selection callbacks on both tables — print to stdout.
    if let Some(view) = ui.get_view("domains") {
        view.borrow_mut().on_event(EventType::Click, Box::new(|ui, _v, _data| {
            if let Some(g) = ui.get_view("domains") {
                if let Some(table) = g.borrow().downcast_ref::<TableView>() {
                    println!("[domains] selected raw row = {:?}", table.selected_row());
                }
            }
            true
        }));
    }
    if let Some(view) = ui.get_view("contacts") {
        view.borrow_mut().on_event(EventType::Click, Box::new(|ui, _v, _data| {
            if let Some(g) = ui.get_view("contacts") {
                if let Some(table) = g.borrow().downcast_ref::<TableView>() {
                    println!("[contacts] selected raw row = {:?}", table.selected_row());
                }
            }
            true
        }));
    }

    // Force a relayout so the freshly-set data is laid out before first paint.
    let _ = &mut ui; // silence unused-mut if any path becomes optional in the future

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
