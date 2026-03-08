use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use super::containers::Frame;
use super::themes::Theme;
use super::traits::{Element, View};
use super::types::Point;
use super::themes::Typeface;

use super::views::{Button, Edit, Label, CheckBox, RadioButton, List, RecyclerView, ImageButton, ImageView, PopupMenu, Dialog};

/// Controls how a popup interacts with the rest of the UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupMode {
    /// Dismisses when the user clicks outside the popup.
    Popup,
    /// Blocks all input to the root tree until closed.
    Modal,
}

/// Controls which direction the popup expands from the anchor point (x, y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupDirection {
    /// (x, y) is the top-left corner of the popup.
    BottomRight,
    /// (x, y) is the top-right corner of the popup.
    BottomLeft,
    /// (x, y) is the bottom-left corner of the popup.
    TopRight,
    /// (x, y) is the bottom-right corner of the popup.
    TopLeft,
    /// (x, y) is the center of the popup.
    Center,
}

struct PopupEntry {
    element: Element,
    x: i32,
    y: i32,
    mode: PopupMode,
}

pub struct UI {
    width: u32,
    height: u32,
    scale: f64,
    typeface: Typeface,
    root: Option<Element>,
    types: HashMap<String, fn() -> Element>,
    on_start: Option<Box<dyn FnMut(&mut UI)>>,
    overlays: Vec<PopupEntry>,
    mouse_pos: Vector2<i32>,
}

#[allow(dead_code)]
impl UI {
    pub fn new(width: u32, height: u32, typeface: Typeface, scale: f64) -> Self {
        let mut ui = UI { width, height, typeface, scale, root: None, types: HashMap::new(), on_start: None, overlays: Vec::new(), mouse_pos: Vector2::new(0, 0) };
        ui.register::<Label>("Label");
        ui.register::<Button>("Button");
        ui.register::<CheckBox>("CheckBox");
        ui.register::<RadioButton>("RadioButton");
        ui.register::<Edit>("Edit");
        ui.register::<List>("List");
        ui.register::<RecyclerView>("RecyclerView");
        ui.register::<ImageButton>("ImageButton");
        ui.register::<ImageView>("ImageView");
        ui.register::<PopupMenu>("PopupMenu");
        ui.register::<Dialog>("Dialog");
        ui.register::<Frame>("Frame");
        ui
    }

    pub fn add_view(&mut self, view: Element) {
        match &self.root {
            None => {
                self.root = Some(view);
            }
            Some(root) => {
                let mut root = root.try_borrow_mut().unwrap();
                root.as_container_mut().unwrap().add_view(view);
            }
        }
    }

    pub fn get_view(&self, id: &str) -> Option<Element> {
        // Search overlays first (topmost)
        for entry in self.overlays.iter().rev() {
            let element = &entry.element;
            if element.borrow().get_id() == id {
                return Some(Rc::clone(element));
            }
            if let Some(container) = element.borrow().as_container() {
                if let Some(found) = container.get_view(id) {
                    return Some(found);
                }
            }
        }
        // Then search root
        match &self.root {
            None => None,
            Some(root) => {
                if root.borrow().get_id() == id {
                    return Some(Rc::clone(root));
                }
                if let Some(root) = root.borrow().as_container() {
                    return root.get_view(id);
                }
                None
            }
        }
    }

    pub fn find_with(&self, predicate: &dyn Fn(&dyn View) -> bool) -> Vec<Element> {
        let mut result = Vec::new();
        // Search overlays
        for entry in &self.overlays {
            Self::collect_matching(&entry.element, predicate, &mut result);
        }
        // Search root
        if let Some(root) = &self.root {
            Self::collect_matching(root, predicate, &mut result);
        }
        result
    }

    fn collect_matching(element: &Element, predicate: &dyn Fn(&dyn View) -> bool, result: &mut Vec<Element>) {
        let view = element.borrow();
        if predicate(&*view) {
            result.push(Rc::clone(element));
        }
        if let Some(container) = view.as_container() {
            for child in container.get_views() {
                Self::collect_matching(&child, predicate, result);
            }
        }
    }

    pub fn register<T: Default + View + 'static>(&mut self, name: &str) {
        self.types.insert(name.to_owned(), || Rc::new(RefCell::from(T::default())));
    }

    pub fn create(&self, name: &str) -> Element {
        self.types.get(name).expect("No type!")()
    }

    pub fn on_start(&mut self, func: Box<dyn FnMut(&mut UI)>) {
        self.on_start = Some(func);
    }

    pub fn layout(&mut self, width: u32, height: u32, scale: f64) {
        self.width = width;
        self.height = height;
        self.scale = scale;
        let root = self.root.clone();
        if let Some(root) = root {
            root.borrow_mut().layout_content(0, 0, width as i32, height as i32, &self.typeface.clone(), scale);
        }
    }

    pub fn relayout(&mut self) {
        let root = self.root.clone();
        if let Some(root) = root {
            root.borrow_mut().layout_content(0, 0, self.width as i32, self.height as i32, &self.typeface.clone(), self.scale);
        }
    }

    /// Shows a popup at the given anchor point, expanding in the given direction.
    pub fn show_popup(&mut self, popup: Element, x: i32, y: i32, direction: PopupDirection, mode: PopupMode) {
        // Layout the popup to determine its size
        let typeface = self.typeface.clone();
        let w = self.width as i32;
        let h = self.height as i32;
        popup.borrow_mut().layout_content(0, 0, w, h, &typeface, self.scale);

        let rect = popup.borrow().get_rect();
        let pw = rect.width();
        let ph = rect.height();

        // Compute origin based on direction
        let (mut ox, mut oy) = match direction {
            PopupDirection::BottomRight => (x, y),
            PopupDirection::BottomLeft => (x - pw, y),
            PopupDirection::TopRight => (x, y - ph),
            PopupDirection::TopLeft => (x - pw, y - ph),
            PopupDirection::Center => (x - pw / 2, y - ph / 2),
        };

        // Clamp to window bounds
        ox = ox.max(0).min(w - pw);
        oy = oy.max(0).min(h - ph);

        self.overlays.push(PopupEntry {
            element: popup,
            x: ox,
            y: oy,
            mode,
        });
    }

    /// Closes a popup by its view ID.
    pub fn close_popup(&mut self, id: &str) {
        self.overlays.retain(|entry| entry.element.borrow().get_id() != id);
    }

    /// Closes all non-modal popups.
    pub fn close_all_popups(&mut self) {
        self.overlays.retain(|entry| entry.mode == PopupMode::Modal);
    }

    /// Returns true if there are any active popups/overlays.
    pub fn has_popups(&self) -> bool {
        !self.overlays.is_empty()
    }

    pub fn start(&mut self) {
        if let Some(mut start) = self.on_start.take() {
            start(self);
        }
    }

    pub fn update(&mut self) -> bool {
        let mut redraw = false;
        // Update overlays first
        let overlays: Vec<Element> = self.overlays.iter().map(|e| Rc::clone(&e.element)).collect();
        for element in overlays {
            redraw |= element.borrow_mut().update(self);
        }
        // Then update root
        let root = self.root.clone();
        if let Some(root) = root {
            redraw |= root.borrow_mut().update(self);
        }
        redraw
    }

    pub fn paint(&self, theme: &mut dyn Theme) {
        theme.clear_screen();
        if let Some(root) = &self.root {
            root.borrow().paint(Point::from((0, 0)), theme);
        }
        // Paint overlays on top, in order (last = topmost)
        for entry in &self.overlays {
            entry.element.borrow().paint(Point::from((entry.x, entry.y)), theme);
        }
    }

    pub fn from_xml(xml: &str, width: u32, height: u32, typeface: Typeface, scale: f64) -> Option<Self> {
        let mut ui = UI::new(width, height, typeface, scale);
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut txt = Vec::new();
        let mut stack: Vec<Element> = Vec::new();

        // TODO extract parsing views into self contained method
        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let element = UI::parse_element(&mut ui, e);
                    stack.push(element);
                },
                Ok(Event::Empty(ref e)) => {
                    let element = UI::parse_element(&mut ui, e);
                    let parent = stack.pop().unwrap();
                    {
                        element.borrow_mut().set_parent(Some(Rc::downgrade(&parent)));
                        let mut ref_mut = parent.borrow_mut();
                        let container = ref_mut.as_container_mut().unwrap();
                        container.add_view(element);
                    }
                    stack.push(parent);
                },
                Ok(Event::End(_)) => {
                    // TODO check that it is the same tag
                    let element = stack.pop().unwrap();
                    match stack.pop() {
                        None => {
                            ui.add_view(element);
                        }
                        Some(parent) => {
                            {
                                let mut ref_mut = parent.borrow_mut();
                                let container = ref_mut.as_container_mut().unwrap();
                                container.add_view(element);
                            }
                            stack.push(parent);
                        }
                    }
                },
                // unescape and decode the text event using the reader encoding
                Ok(Event::Text(e)) => txt.push(e.into_inner().into_owned()),
                Ok(Event::Eof) => break, // exits the loop when reaching end of file
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
        }
        Some(ui)
    }

    fn parse_element(ui: &mut UI, e: &BytesStart) -> Element {
        let attributes = e
            .attributes()
            .map(|a| a.unwrap())
            .collect::<Vec<_>>();
        //println!("attributes values: {:?}", attributes);
        let view_type = String::from_utf8(e.name().0.to_vec()).unwrap();
        let view = ui.create(&view_type);
        //println!("Loaded {}", &view_type);
        for attribute in attributes {
            let name = String::from_utf8(attribute.key.0.to_vec()).unwrap();
            let value = match attribute.value {
                Cow::Borrowed(c) => {
                    String::from_utf8(c.to_vec()).unwrap()
                }
                Cow::Owned(c) => {
                    String::from_utf8(c.to_vec()).unwrap()
                }
            };
            view.borrow_mut().set_any(&name, &value);
            //println!("Attribute: {} = {}", &name, &value);
        }
        view
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Returns the current mouse position in absolute window coordinates.
    pub fn get_mouse_pos(&self) -> Vector2<i32> {
        self.mouse_pos
    }

    /// Returns true if any overlay is modal.
    fn has_modal_overlay(&self) -> bool {
        self.overlays.iter().any(|e| e.mode == PopupMode::Modal)
    }

    pub fn on_mouse_move(&mut self, position: Vector2<i32>) -> bool {
        self.mouse_pos = position;
        // Dispatch to overlays first (reverse order = topmost first)
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Vector2::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_move(self, local) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_move(self, position),
        }
    }

    pub fn on_mouse_button_down(&mut self, position: Vector2<i32>, button: MouseButton) -> bool {
        // Dispatch to overlays first
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Vector2::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_button_down(self, local, button) {
                return true;
            }
        }
        // Click missed all overlays — dismiss Popup-mode overlays
        if !self.overlays.is_empty() {
            self.close_all_popups();
            return true;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_button_down(self, position, button),
        }
    }

    pub fn on_mouse_button_up(&mut self, position: Vector2<i32>, button: MouseButton) -> bool {
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Vector2::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_button_up(self, local, button) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_button_up(self, position, button),
        }
    }

    pub fn on_mouse_wheel_scroll(&mut self, position: Vector2<i32>, distance: MouseScrollDistance) -> bool {
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Vector2::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_wheel_scroll(self, local, distance) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_wheel_scroll(self, position, distance),
        }
    }

    pub fn on_key_down(&mut self, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, modifiers: ModifiersState) -> bool {
        // Dispatch to overlays first (reverse order)
        let elements: Vec<Element> = self.overlays.iter().rev()
            .map(|e| Rc::clone(&e.element))
            .collect();
        for element in &elements {
            if element.borrow().on_key_down(self, virtual_key_code, scancode, modifiers.clone()) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_key_down(self, virtual_key_code, scancode, modifiers),
        }
    }

    pub fn on_key_up(&mut self, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, modifiers: ModifiersState) -> bool {
        let elements: Vec<Element> = self.overlays.iter().rev()
            .map(|e| Rc::clone(&e.element))
            .collect();
        for element in &elements {
            if element.borrow().on_key_up(self, virtual_key_code, scancode, modifiers.clone()) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_key_up(self, virtual_key_code, scancode, modifiers),
        }
    }

    pub fn on_key_char(&mut self, unicode_codepoint: char, modifiers: ModifiersState) -> bool {
        let elements: Vec<Element> = self.overlays.iter().rev()
            .map(|e| Rc::clone(&e.element))
            .collect();
        for element in &elements {
            if element.borrow().on_key_char(self, unicode_codepoint, modifiers.clone()) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_key_char(self, unicode_codepoint, modifiers),
        }
    }
}