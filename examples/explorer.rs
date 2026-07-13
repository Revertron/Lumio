#![windows_subsystem = "windows"]

//! File-manager demo: a lazily-loaded directory `TreeView` on the left, an
//! Explorer-"List"-mode `IconList` with the directory contents on the right.
//! Expand tree branches (chevron / Right key), select a directory to list it,
//! multi-select files with Ctrl/Shift+Click, double-click a folder to
//! navigate into it.

use std::fs;
use std::path::Path;

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 1800;
const HEIGHT: u32 = 900;
const TITLE: &str = "Explorer Example";

const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

const FOLDER_ICON: &str = "icons/folder.svg";
const FILE_ICON: &str = "icons/file-outline.svg";
const FOLDER_TINT: u32 = 0xFFF0C060; // amber folders, like Explorer
const FILE_TINT: u32 = 0xFFB0B0B0;

struct Provider {
    dir: Dir<'static>,
}

impl AssetsProvider for Provider {
    fn get_file(&self, path: &str) -> Option<&[u8]> {
        self.dir.get_file(path).map(|file| file.contents())
    }
}

fn folder_node(text: &str, key: &str) -> TreeNode {
    TreeNode {
        text: text.to_owned(),
        key: key.to_owned(),
        icon: Some(FOLDER_ICON.to_owned()),
        tint: Some(FOLDER_TINT),
        // Optimistic: show a chevron; it clears itself on an empty expand.
        has_children: true,
        ..TreeNode::default()
    }
}

#[cfg(windows)]
fn list_roots() -> Vec<TreeNode> {
    ('A'..='Z')
        .filter_map(|c| {
            let path = format!("{c}:\\");
            Path::new(&path).exists().then(|| folder_node(&path, &path))
        })
        .collect()
}

#[cfg(not(windows))]
fn list_roots() -> Vec<TreeNode> {
    vec![folder_node("/", "/")]
}

/// Subdirectories of `dir`, sorted case-insensitively. Unreadable entries
/// are skipped silently (permission-denied dirs, vanished removable drives).
fn list_dirs(dir: &str) -> Vec<TreeNode> {
    let mut dirs: Vec<(String, String)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().to_string_lossy().into_owned()))
        .collect();
    dirs.sort_by_key(|(name, _)| name.to_lowercase());
    dirs.iter().map(|(name, path)| folder_node(name, path)).collect()
}

/// Fill the right pane with the contents of `dir`: folders first, then
/// files, each group sorted case-insensitively.
fn populate_files(ui: &mut UI, dir: &str) {
    let mut dirs: Vec<(String, String)> = Vec::new();
    let mut files: Vec<(String, String)> = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path().to_string_lossy().into_owned();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            dirs.push((name, path));
        } else {
            files.push((name, path));
        }
    }
    dirs.sort_by_key(|(name, _)| name.to_lowercase());
    files.sort_by_key(|(name, _)| name.to_lowercase());

    let items: Vec<IconListItem> = dirs.iter()
        .map(|(name, path)| IconListItem::new(name, FOLDER_ICON, FOLDER_TINT, path))
        .chain(files.iter().map(|(name, path)| IconListItem::new(name, FILE_ICON, FILE_TINT, path)))
        .collect();

    if let Some(list) = ui.get_view("files") {
        if let Some(il) = list.borrow().as_any().downcast_ref::<IconList>() {
            il.set_items(items);
        }
    }
    if let Some(label) = ui.get_view("path_label") {
        if let Some(l) = label.borrow_mut().downcast_mut::<Label>() {
            l.set_text(dir);
        }
    }
}

fn main() {
    set_provider(Box::new(Provider { dir: ASSETS }));

    let layout = include_str!("explorer.xml");
    let mut ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    if let Some(tree) = ui.get_view("tree") {
        // Lazy loading: the first time a branch expands, read its
        // subdirectories from disk. The handler mutates the firing view
        // through its `&self` API (never borrow_mut the firing view).
        tree.borrow_mut().on_event(EventType::Expanded, Box::new(|_ui, view, _data| {
            let tv = view.as_any().downcast_ref::<TreeView>().unwrap();
            if let Some(key) = tv.expanded_key() {
                let unloaded = tv.node(&key).map(|n| n.children.is_empty()).unwrap_or(false);
                if unloaded {
                    tv.set_children(&key, list_dirs(&key));
                }
            }
            true
        }));

        // Selecting a directory lists its contents on the right.
        tree.borrow_mut().on_event(EventType::SelectionChanged, Box::new(|ui, view, _data| {
            let tv = view.as_any().downcast_ref::<TreeView>().unwrap();
            if let Some(key) = tv.selected_key() {
                populate_files(ui, &key);
            }
            true
        }));
    }

    // Double-click a folder in the file pane → navigate into it.
    if let Some(files) = ui.get_view("files") {
        files.borrow_mut().on_event(EventType::DoubleClick, Box::new(|ui, view, data| {
            let il = view.as_any().downcast_ref::<IconList>().unwrap();
            let EventData::Position { x, y } = data else { return false; };
            let Some(idx) = il.item_at(*x, *y) else { return false; };
            let Some(item) = il.item(idx) else { return false; };
            if !Path::new(&item.key).is_dir() {
                return false;
            }
            if let Some(tree) = ui.get_view("tree") {
                if let Some(tv) = tree.borrow().as_any().downcast_ref::<TreeView>() {
                    if tv.node(&item.key).map(|n| n.children.is_empty()).unwrap_or(false) {
                        tv.set_children(&item.key, list_dirs(&item.key));
                    }
                    // Programmatic selection fires no event, so list explicitly.
                    tv.select_key(&item.key);
                }
            }
            populate_files(ui, &item.key);
            true
        }));
    }

    // Initial tree: all drive roots (or "/" on unix), first one opened.
    ui.on_start(Box::new(|ui| {
        let first = {
            let Some(tree) = ui.get_view("tree") else { return; };
            let tree = tree.borrow();
            let Some(tv) = tree.as_any().downcast_ref::<TreeView>() else { return; };
            let roots = list_roots();
            let first = roots.first().map(|n| n.key.clone());
            tv.set_roots(roots);
            if let Some(key) = &first {
                tv.set_children(key, list_dirs(key));
                tv.set_expanded(key, true);
                tv.select_key(key);
            }
            first
        };
        if let Some(key) = first {
            populate_files(ui, &key);
        }
        ui.relayout();
    }));

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}
