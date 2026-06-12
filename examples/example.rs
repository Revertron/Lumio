#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::rc::Rc;
use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{VirtualKeyCode, WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1200;
const TITLE: &'static str = "Lumio";

// Usually you will not use the `examples` part
const ASSETS: Dir = include_dir!("$CARGO_MANIFEST_DIR/examples/assets");

struct Provider {
    dir: Dir<'static>
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

// Message data structure
struct Message {
    sender: String,
    text: String,
    time: String,
}

// MessageAdapter implements RecyclerAdapter
struct MessageAdapter {
    messages: Vec<Message>,
    item_layout: String,
}

impl MessageAdapter {
    fn new(messages: Vec<Message>) -> Self {
        let item_layout = include_str!("message.xml").to_string();
        Self { messages, item_layout }
    }
}

impl RecyclerAdapter for MessageAdapter {
    fn get_item_count(&self) -> usize {
        self.messages.len()
    }

    fn get_item_view_type(&self, _position: usize) -> i32 {
        0 // Single view type for now
    }

    fn create_view_holder(&mut self, _view_type: i32) -> ViewHolder {
        // Parse XML layout for each item
        let ui = UI::from_xml(&self.item_layout, 800, 100, Classic::typeface(), 1.0).unwrap();
        let root = ui.get_view("message_item").expect("message_item not found in XML");

        ViewHolder::new(root, _view_type)
    }

    fn bind_view_holder(&self, holder: &ViewHolder, position: usize) {
        if position >= self.messages.len() {
            return;
        }

        let message = &self.messages[position];

        // Get the root frame
        if let Some(frame) = holder.item_view.borrow().downcast_ref::<Frame>() {
            // Find and update sender label
            if let Some(sender_view) = frame.as_container().unwrap().get_view("sender") {
                if let Some(label) = sender_view.borrow_mut().downcast_mut::<lumio::views::Label>() {
                    label.set_text(&message.sender);
                }
            }

            // Find and update time label
            if let Some(time_view) = frame.as_container().unwrap().get_view("time") {
                if let Some(label) = time_view.borrow_mut().downcast_mut::<lumio::views::Label>() {
                    label.set_text(&message.time);
                }
            }

            // Find and update text label
            if let Some(text_view) = frame.as_container().unwrap().get_view("text") {
                if let Some(label) = text_view.borrow_mut().downcast_mut::<lumio::views::Label>() {
                    label.set_text(&message.text);
                }
            }
        }
    }
}

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("layout.xml");
    let mut ui = UI::from_xml(layout, WIDTH, HEIGHT, Classic::typeface(), 1.0).unwrap();

    if let Some(button) = ui.get_view("btn1") {
        button.borrow_mut().on_event(EventType::Click, Box::new(button1_click));
    }

    if let Some(button) = ui.get_view("btn2") {
        button.borrow_mut().on_event(EventType::Click, Box::new(button2_click));
    }

    if let Some(button) = ui.get_view("btn3") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            let menu_element = ui.create("PopupMenu");
            {
                let mut menu = menu_element.borrow_mut();
                let popup = menu.downcast_mut::<PopupMenu>().unwrap();
                popup.add_item("cut", "icons/cut.png", "Cut");
                popup.add_item("copy", "icons/copy.png", "Copy");
                popup.add_item("paste", "icons/paste.png", "Paste");
                popup.on_event(EventType::Click, Box::new(|_ui, view, _data| {
                    let popup = view.as_any().downcast_ref::<PopupMenu>().unwrap();
                    if let Some(index) = popup.get_hovered_index() {
                        println!("Selected item: {}", index);
                    }
                    true
                }));
            }
            let pos = ui.get_mouse_pos();
            ui.show_popup(menu_element, pos.x, pos.y, PopupDirection::BottomRight, PopupMode::Popup);
            true
        }));
    }

    if let Some(button) = ui.get_view("btn9") {
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, _view, _data| {
            let dlg: Element = Rc::new(RefCell::new(Dialog::new()));
            {
                let mut d = dlg.borrow_mut();
                let dialog = d.downcast_mut::<Dialog>().unwrap();
                dialog.set_icon("icons/warning.png");
                dialog.set_message("Are you sure you want to delete this file?");
                dialog.add_button("yes", "Yes", ButtonSide::Right, true);
                dialog.add_button("no", "No", ButtonSide::Right, false);
                dialog.add_button("help", "Help", ButtonSide::Left, false);
                // Esc now presses "No" instead of just closing the dialog;
                // Enter presses the focused (or default) button.
                dialog.set_cancel_button("no");
                dialog.on_event(EventType::Click, Box::new(|ui, view, _data| {
                    let d = view.as_any().downcast_ref::<Dialog>().unwrap();
                    println!("Pressed: {:?}", d.get_pressed_button());
                    d.close(ui);
                    true
                }));
            }
            let cx = (ui.get_width() / 2) as i32;
            let cy = (ui.get_height() / 2) as i32;
            ui.show_popup(dlg, cx, cy, PopupDirection::Center, PopupMode::Modal);
            true
        }));
    }

    if let Some(check) = ui.get_view("dark_mode") {
        check.borrow_mut().on_event(EventType::CheckedChanged, Box::new(|ui, view, _data| {
            let dark = view.as_any().downcast_ref::<CheckBox>().map(|c| c.is_checked()).unwrap_or(false);
            ui.set_palette(if dark { Palette::dark() } else { Palette::classic() });
            true
        }));
    }

    if let Some(image) = ui.get_view("my_image") {
        image.borrow_mut().on_event(EventType::Click, Box::new(|_ui, _view, _data| {
            println!("Image clicked!");
            true
        }));
    }

    // --- Event-system demos: focus, hover, double-click, key-down, context menu ---

    if let Some(edit) = ui.get_view("edit1") {
        let mut edit = edit.borrow_mut();
        // Validate-on-blur: an empty edit1 shows the error underline.
        edit.on_event(EventType::FocusLost, Box::new(|_ui, view, _data| {
            let edit = view.as_any().downcast_ref::<Edit>().unwrap();
            let empty = edit.get_text().is_empty();
            edit.set_error(empty);
            println!("edit1 lost focus (empty = {})", empty);
            true
        }));
        edit.on_event(EventType::FocusGained, Box::new(|_ui, view, _data| {
            let edit = view.as_any().downcast_ref::<Edit>().unwrap();
            edit.set_error(false);
            println!("edit1 gained focus");
            true
        }));
        // KeyDown runs before the Edit's own handling: F2 is intercepted
        // (returns true), everything else falls through and types normally.
        edit.on_event(EventType::KeyDown, Box::new(|_ui, _view, data| {
            if let EventData::Key { code, modifiers } = data {
                println!("edit1 key: {:?} (ctrl = {})", code, modifiers.ctrl());
                if *code == Some(VirtualKeyCode::F2) {
                    println!("F2 intercepted by the KeyDown listener");
                    return true;
                }
            }
            false
        }));
        // Returning true suppresses the built-in Cut/Copy/Paste menu.
        edit.on_event(EventType::ContextMenu, Box::new(|ui, _view, data| {
            let menu_element = ui.create("PopupMenu");
            {
                let mut menu = menu_element.borrow_mut();
                let popup = menu.downcast_mut::<PopupMenu>().unwrap();
                popup.add_item("custom1", "", "Custom menu");
                popup.add_item("custom2", "", "replacing the built-in one");
            }
            if let EventData::Position { x, y } = data {
                ui.show_popup(menu_element, *x, *y, PopupDirection::BottomRight, PopupMode::Popup);
            }
            true
        }));
    }

    if let Some(button) = ui.get_view("btn1") {
        let mut button = button.borrow_mut();
        button.on_event(EventType::HoverEnter, Box::new(|ui, _view, _data| {
            set_status(ui, "Hovering Button 1");
            true
        }));
        button.on_event(EventType::HoverExit, Box::new(|ui, _view, _data| {
            set_status(ui, "Ready");
            true
        }));
    }

    if let Some(label) = ui.get_view("label1") {
        label.borrow_mut().on_event(EventType::DoubleClick, Box::new(|_ui, _view, data| {
            println!("label1 double-clicked at {:?}", data);
            true
        }));
    }

    // A view without any built-in context menu gets one via the event.
    if let Some(button) = ui.get_view("btn2") {
        button.borrow_mut().on_event(EventType::ContextMenu, Box::new(|ui, _view, data| {
            let menu_element = ui.create("PopupMenu");
            {
                let mut menu = menu_element.borrow_mut();
                let popup = menu.downcast_mut::<PopupMenu>().unwrap();
                popup.add_item("action_a", "", "Button 2 action A");
                popup.add_item("action_b", "", "Button 2 action B");
            }
            if let EventData::Position { x, y } = data {
                ui.show_popup(menu_element, *x, *y, PopupDirection::BottomRight, PopupMode::Popup);
            }
            true
        }));
    }

    // Global accelerators: fire when the focused view did not consume the key.
    ui.add_shortcut("Ctrl+Shift+S", Box::new(|_ui| {
        println!("Ctrl+Shift+S shortcut fired");
        true
    }));
    ui.add_shortcut("F5", Box::new(|_ui| {
        println!("F5 shortcut fired");
        true
    }));

    ui.on_start(Box::new(on_start));

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}

fn button1_click(ui: &mut UI, view: &dyn View, _data: &EventData) -> bool {
    let mut checked = false;
    if let Some(checkbox) = ui.get_view("checkbox1") {
        if let Some(ch) = checkbox.borrow_mut().downcast_mut::<CheckBox>() {
            checked = ch.is_checked();
        }
    }

    // Change something in another view
    if let Some(edit) = ui.get_view("edit1") {
        if let Some(e) = edit.borrow_mut().downcast_mut::<Edit>() {
            e.set_text(&format!("CheckBox checked = {}", checked));
        }
    }
    // Change something in clicked view
    if let Some(button) = view.as_any().downcast_ref::<Button>() {
        button.set_text("Clicked!");
    }
    true
}

fn button2_click(ui: &mut UI, _view: &dyn View, _data: &EventData) -> bool {
    let mut buf = Vec::new();
    for i in 1..=20 {
        buf.push(format!("New item {}", i));
    }
    // Set items for list
    set_items_for_list1(ui, buf);
    true
}

fn set_status(ui: &mut UI, text: &str) {
    if let Some(sb) = ui.get_view("statusbar") {
        if let Some(statusbar) = sb.borrow_mut().downcast_mut::<StatusBar>() {
            statusbar.set_section_text("status", text);
        }
    }
}

fn set_items_for_list1(ui: &mut UI, buf: Vec<String>) {
    if let Some(list) = ui.get_view("list1") {
        if let Some(list) = list.borrow_mut().downcast_mut::<List>() {
            list.set_items(buf);
        }
    }
}

fn on_start(ui: &mut UI) {
    let mut buf = Vec::new();
    for i in 1..=20 {
        buf.push(format!("Start item {}", i));
    }
    // Set items for list
    set_items_for_list1(ui, buf);

    // Create 50 messages for RecyclerView
    let names = vec!["Alice", "Bob", "Charlie", "Diana", "Eve", "Frank"];
    let mut messages = Vec::new();

    for i in 0..50 {
        let sender = names[i % names.len()].to_string();
        let text = format!("This is message number {} with some sample text to display.", i + 1);
        let hour = 9 + (i / 6) % 12;
        let minute = (i * 10) % 60;
        let time = format!("{:02}:{:02}", hour, minute);

        messages.push(Message {
            sender,
            text,
            time,
        });
    }

    // Set adapter on RecyclerView
    if let Some(recycler) = ui.get_view("my_list") {
        if let Some(recycler) = recycler.borrow_mut().downcast_mut::<RecyclerView>() {
            let adapter = MessageAdapter::new(messages);
            recycler.set_adapter(Box::new(adapter));
        }
    }

    // Add sections to StatusBar
    if let Some(sb) = ui.get_view("statusbar") {
        if let Some(statusbar) = sb.borrow_mut().downcast_mut::<StatusBar>() {
            statusbar.add_section("status", "Ready");
            statusbar.add_section("info", "Lumio GUI Example");
            statusbar.add_section("pos", "Ln 1, Col 1");
        }
    }

    // Trigger layout after setting adapter so RecyclerView displays items immediately
    ui.relayout();
}