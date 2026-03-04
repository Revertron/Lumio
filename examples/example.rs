#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};
use speedy2d::dimen::Vector2;
use speedy2d::Window;
use speedy2d::window::{WindowCreationOptions, WindowPosition, WindowSize};

use lumio::prelude::*;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
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
        button.borrow_mut().on_event(EventType::Click, Box::new(|ui, view| {
            let menu_element = ui.create("PopupMenu");
            {
                let mut menu = menu_element.borrow_mut();
                let popup = menu.downcast_mut::<PopupMenu>().unwrap();
                popup.add_item("cut", "icons/cut.png", "Cut");
                popup.add_item("copy", "icons/copy.png", "Copy");
                popup.add_item("paste", "icons/paste.png", "Paste");
                popup.on_event(EventType::Click, Box::new(|ui, view| {
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

    if let Some(image) = ui.get_view("my_image") {
        image.borrow_mut().on_event(EventType::Click, Box::new(|_ui, _view| {
            println!("Image clicked!");
            true
        }));
    }

    ui.on_start(Box::new(on_start));

    let window_size = WindowSize::PhysicalPixels(Vector2::new(WIDTH, HEIGHT));
    let options = WindowCreationOptions::new_windowed(window_size, Some(WindowPosition::Center));
    let window: Window<WinEvent> = Window::new_with_user_events(TITLE, options).unwrap();
    let sender = window.create_user_event_sender();
    let win = Win::new(ui, sender);
    window.run_loop(win);
}

fn button1_click(ui: &mut UI, view: &dyn View) -> bool {
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

fn button2_click(ui: &mut UI, _view: &dyn View) -> bool {
    let mut buf = Vec::new();
    for i in 1..=20 {
        buf.push(format!("New item {}", i));
    }
    // Set items for list
    set_items_for_list1(ui, buf);
    true
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

    // Trigger layout after setting adapter so RecyclerView displays items immediately
    ui.relayout();
}