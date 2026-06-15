#![windows_subsystem = "windows"]

use include_dir::{Dir, include_dir};

use lumio::prelude::*;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1000;
const TITLE: &str = "Mimir";

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
        self.dir.get_file(path).map(|f| f.contents())
    }
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Contact {
    name: String,
    last_message: String,
    time: String,
}

#[derive(Clone)]
struct ChatMessage {
    sender: String,
    text: String,
    time: String,
}

// ---------------------------------------------------------------------------
// Contact list adapter
// ---------------------------------------------------------------------------

struct ContactAdapter {
    contacts: Vec<Contact>,
    item_layout: String,
}

impl ContactAdapter {
    fn new(contacts: Vec<Contact>) -> Self {
        let item_layout = include_str!("contact_item.xml").to_string();
        Self { contacts, item_layout }
    }
}

impl RecyclerAdapter for ContactAdapter {
    fn get_item_count(&self) -> usize {
        self.contacts.len()
    }

    fn get_item_view_type(&self, _position: usize) -> i32 {
        0
    }

    fn create_view_holder(&mut self, view_type: i32) -> ViewHolder {
        let ui = UI::from_xml(&self.item_layout, 360, 60, default_typeface(), 1.0).unwrap();
        let root = ui.get_view("contact_item").expect("contact_item not found");
        ViewHolder::new(root, view_type)
    }

    fn bind_view_holder(&self, holder: &ViewHolder, position: usize) {
        if position >= self.contacts.len() {
            return;
        }
        let contact = &self.contacts[position];
        if let Some(frame) = holder.item_view.borrow().downcast_ref::<Frame>() {
            if let Some(container) = frame.as_container() {
                if let Some(v) = container.get_view("contact_name") {
                    if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
                        label.set_text(&contact.name);
                        label.set_single_line(true);
                    }
                }
                if let Some(v) = container.get_view("contact_time") {
                    if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
                        label.set_text(&contact.time);
                        label.set_single_line(true);
                    }
                }
                if let Some(v) = container.get_view("contact_last_msg") {
                    if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
                        label.set_text(&contact.last_message);
                        label.set_single_line(true);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Chat messages adapter
// ---------------------------------------------------------------------------

struct ChatAdapter {
    messages: Vec<ChatMessage>,
    item_layout: String,
}

impl ChatAdapter {
    fn new(messages: Vec<ChatMessage>) -> Self {
        let item_layout = include_str!("chat_message.xml").to_string();
        Self { messages, item_layout }
    }
}

impl RecyclerAdapter for ChatAdapter {
    fn get_item_count(&self) -> usize {
        self.messages.len()
    }

    fn get_item_view_type(&self, _position: usize) -> i32 {
        0
    }

    fn create_view_holder(&mut self, view_type: i32) -> ViewHolder {
        let ui = UI::from_xml(&self.item_layout, 800, 60, default_typeface(), 1.0).unwrap();
        let root = ui.get_view("chat_msg_item").expect("chat_msg_item not found");
        ViewHolder::new(root, view_type)
    }

    fn bind_view_holder(&self, holder: &ViewHolder, position: usize) {
        if position >= self.messages.len() {
            return;
        }
        let msg = &self.messages[position];
        if let Some(frame) = holder.item_view.borrow().downcast_ref::<Frame>() {
            if let Some(container) = frame.as_container() {
                if let Some(v) = container.get_view("msg_sender") {
                    if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
                        label.set_text(&msg.sender);
                        label.set_single_line(true);
                    }
                }
                if let Some(v) = container.get_view("msg_time") {
                    if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
                        label.set_text(&msg.time);
                        label.set_single_line(true);
                    }
                }
                if let Some(v) = container.get_view("msg_text") {
                    if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
                        label.set_text(&msg.text);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sample data
// ---------------------------------------------------------------------------

fn sample_contacts() -> Vec<Contact> {
    vec![
        Contact { name: "Alice".into(), last_message: "See you tomorrow!".into(), time: "10:42".into() },
        Contact { name: "Bob".into(), last_message: "Got it, thanks".into(), time: "09:15".into() },
        Contact { name: "Charlie".into(), last_message: "The build is green now".into(), time: "Yesterday".into() },
        Contact { name: "Diana".into(), last_message: "Can you review my PR?".into(), time: "Yesterday".into() },
        Contact { name: "Eve".into(), last_message: "Meeting at 3pm".into(), time: "Monday".into() },
        Contact { name: "Frank".into(), last_message: "Happy birthday!".into(), time: "Monday".into() },
        Contact { name: "Grace".into(), last_message: "Check the docs".into(), time: "Sunday".into() },
        Contact { name: "Hank".into(), last_message: "Lunch?".into(), time: "Sunday".into() },
        Contact { name: "Ivy".into(), last_message: "On my way".into(), time: "Last week".into() },
        Contact { name: "Jack".into(), last_message: "Sounds good".into(), time: "Last week".into() },
        Contact { name: "Karen".into(), last_message: "Let me check".into(), time: "Last week".into() },
        Contact { name: "Leo".into(), last_message: "Done!".into(), time: "Last week".into() },
    ]
}

fn sample_chat(contact: &str) -> Vec<ChatMessage> {
    let me = "You";
    vec![
        ChatMessage { sender: contact.into(), text: "Hey, how are you?".into(), time: "09:00".into() },
        ChatMessage { sender: me.into(), text: "I'm good, thanks! How about you?".into(), time: "09:01".into() },
        ChatMessage { sender: contact.into(), text: "Doing great. Did you see the latest update?".into(), time: "09:02".into() },
        ChatMessage { sender: me.into(), text: "Not yet, what changed?".into(), time: "09:03".into() },
        ChatMessage { sender: contact.into(), text: "They added RecyclerView support, it's really smooth now.".into(), time: "09:04".into() },
        ChatMessage { sender: me.into(), text: "That's awesome! I'll take a look.".into(), time: "09:05".into() },
        ChatMessage { sender: contact.into(), text: "Yeah, scroll performance is much better.".into(), time: "09:06".into() },
        ChatMessage { sender: me.into(), text: "Perfect, I was waiting for that.".into(), time: "09:07".into() },
        ChatMessage { sender: contact.into(), text: "Also the XML layout system is really convenient.".into(), time: "09:10".into() },
        ChatMessage { sender: me.into(), text: "Agreed, defining UI in XML keeps things clean.".into(), time: "09:11".into() },
        ChatMessage { sender: contact.into(), text: "Exactly. Want to pair on the messenger example?".into(), time: "09:15".into() },
        ChatMessage { sender: me.into(), text: "Sure, let's do it after lunch.".into(), time: "09:16".into() },
        ChatMessage { sender: contact.into(), text: "See you tomorrow!".into(), time: "10:42".into() },
    ]
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

fn main() {
    let assets = Provider::new(ASSETS);
    set_provider(Box::new(assets));

    let layout = include_str!("messenger_layout.xml");
    let mut ui = UI::from_xml(layout, WIDTH, HEIGHT, default_typeface(), 1.0).unwrap();

    // Send button click
    if let Some(btn) = ui.get_view("btn_send") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(on_send_click));
    }

    // Attach button click (placeholder)
    if let Some(btn) = ui.get_view("btn_attach") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|_ui, _view, _data| {
            println!("Attach clicked");
            true
        }));
    }

    ui.on_start(Box::new(on_start));

    lumio::run(ui, WindowConfig::new(TITLE, WIDTH, HEIGHT).center());
}

fn on_start(ui: &mut UI) {
    let contacts = sample_contacts();

    // Set up contact list
    if let Some(rv) = ui.get_view("contacts_list") {
        if let Some(recycler) = rv.borrow_mut().downcast_mut::<RecyclerView>() {
            let adapter = ContactAdapter::new(contacts.clone());
            recycler.set_adapter(Box::new(adapter));
        }
    }

    // Load the first contact's chat by default
    if let Some(first) = contacts.first() {
        load_chat(ui, &first.name);
    }

    ui.relayout();

    // Debug: print rects of key views
    /*for id in &["root", "contacts_panel", "chat_panel", "contacts_list", "messages_list", "contacts_header", "chat_header"] {
        if let Some(v) = ui.get_view(id) {
            let rect = v.borrow().get_rect();
            println!("  {} rect: {:?}", id, rect);
        }
    }*/
}

fn load_chat(ui: &mut UI, contact_name: &str) {
    // Update header
    if let Some(v) = ui.get_view("chat_title") {
        if let Some(label) = v.borrow_mut().downcast_mut::<Label>() {
            label.set_text(contact_name);
        }
    }

    // Set up message list
    let messages = sample_chat(contact_name);
    if let Some(rv) = ui.get_view("messages_list") {
        if let Some(recycler) = rv.borrow_mut().downcast_mut::<RecyclerView>() {
            let adapter = ChatAdapter::new(messages);
            recycler.set_adapter(Box::new(adapter));
        }
    }
}

fn on_send_click(ui: &mut UI, _view: &dyn View, _data: &EventData) -> bool {
    // Read the message from the input field
    let text = {
        if let Some(edit) = ui.get_view("message_input") {
            if let Some(e) = edit.borrow().downcast_ref::<Memo>() {
                let t = e.get_text().to_string();
                t
            } else {
                return false;
            }
        } else {
            return false;
        }
    };

    if text.trim().is_empty() {
        return false;
    }

    // Clear the input and shrink back to one line
    if let Some(edit) = ui.get_view("message_input") {
        if let Some(e) = edit.borrow().downcast_ref::<Memo>() {
            e.reset();
        }
    }
    ui.relayout();

    println!("Send: {}", text);
    true
}