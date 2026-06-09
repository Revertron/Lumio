use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance, VirtualKeyCode};

use super::containers::Frame;
use super::themes::Theme;
use super::traits::{Element, View};
use super::types::Point;
use super::themes::Typeface;

use super::views::{Button, Edit, Label, CheckBox, RadioButton, ComboBox, ScrollView, ProgressBar, TabView, List, RecyclerView, ImageButton, ImageView, PopupMenu, Dialog, Separator, SplitPanel, StatusBar, Memo, NotificationStack, TableView, TableColumn, TableRow, Grid, RichText};
use super::views::Dimension;
use std::time::Duration;

/// Controls how a popup interacts with the rest of the UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupMode {
    /// Dismisses when the user clicks outside the popup.
    Popup,
    /// Blocks all input to the root tree until closed.
    Modal,
    /// Lets unhandled input fall through to overlays/root below. Never auto-dismissed.
    Transparent,
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

const TOOLTIP_DELAY_MS: u128 = 700;
const TOOLTIP_ID: &str = "__tooltip__";

struct TooltipPopup {
    element: Element,
    x: i32,
    y: i32,
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
    tooltip_view_id: Option<String>,
    tooltip_hover_start: Option<Instant>,
    tooltip_showing: bool,
    tooltip_popup: Option<TooltipPopup>,
    needs_relayout: bool,
    notification_stack: Option<Element>,
    /// Ids queued for removal during event dispatch. Processed at the next
    /// `update()` tick so removal can be requested from inside handlers
    /// without invalidating the iterator the dispatcher is walking.
    pending_removals: Vec<String>,
    /// The cursor shape requested by the view under the pointer during the
    /// current `on_mouse_move` dispatch. Reset each move; resolved via
    /// [`UI::current_cursor`] and applied by the window handler.
    requested_cursor: Option<MouseCursorType>,
}

#[allow(dead_code)]
impl UI {
    pub fn new(width: u32, height: u32, typeface: Typeface, scale: f64) -> Self {
        let mut ui = UI {
            width, height, typeface, scale, root: None, types: HashMap::new(),
            on_start: None, overlays: Vec::new(), mouse_pos: Vector2::new(0, 0),
            tooltip_view_id: None, tooltip_hover_start: None, tooltip_showing: false, tooltip_popup: None, needs_relayout: false,
            notification_stack: None,
            pending_removals: Vec::new(),
            requested_cursor: None,
        };
        ui.register::<Label>("Label");
        ui.register::<Button>("Button");
        ui.register::<CheckBox>("CheckBox");
        ui.register::<RadioButton>("RadioButton");
        ui.register::<ComboBox>("ComboBox");
        ui.register::<ScrollView>("ScrollView");
        ui.register::<ProgressBar>("ProgressBar");
        ui.register::<TabView>("TabView");
        ui.register::<Edit>("Edit");
        ui.register::<List>("List");
        ui.register::<RecyclerView>("RecyclerView");
        ui.register::<ImageButton>("ImageButton");
        ui.register::<ImageView>("ImageView");
        ui.register::<PopupMenu>("PopupMenu");
        ui.register::<Dialog>("Dialog");
        ui.register::<Separator>("Separator");
        ui.register::<SplitPanel>("SplitPanel");
        ui.register::<StatusBar>("StatusBar");
        ui.register::<Memo>("Memo");
        ui.register::<Frame>("Frame");
        ui.register::<NotificationStack>("NotificationStack");
        ui.register::<TableView>("TableView");
        ui.register::<TableColumn>("TableColumn");
        ui.register::<TableRow>("TableRow");
        ui.register::<Grid>("Grid");
        ui.register::<RichText>("RichText");
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

    /// Queue a view for removal from its parent. Safe to call from inside an
    /// event handler — the actual tree mutation happens at the next `update()`
    /// tick, after the dispatcher has released its borrow on the firing view.
    /// Returns silently if no view with this id exists at flush time.
    pub fn remove_view(&mut self, id: &str) {
        self.pending_removals.push(id.to_owned());
    }

    fn process_pending_removals(&mut self) -> bool {
        if self.pending_removals.is_empty() {
            return false;
        }
        let ids: Vec<String> = std::mem::take(&mut self.pending_removals);
        let mut any_removed = false;
        for id in ids {
            if self.do_remove_view(&id) {
                any_removed = true;
            }
        }
        if any_removed {
            self.needs_relayout = true;
        }
        any_removed
    }

    fn do_remove_view(&mut self, id: &str) -> bool {
        // Try overlays first; an overlay element could itself match by id, in
        // which case we drop the whole overlay entry. Otherwise recurse into
        // the overlay's container children.
        if let Some(pos) = self.overlays.iter().position(|e| e.element.borrow().get_id() == id) {
            self.overlays.remove(pos);
            return true;
        }
        for entry in &self.overlays {
            if let Some(container) = entry.element.borrow_mut().as_container_mut() {
                if container.remove_view(id) {
                    return true;
                }
            }
        }
        if let Some(root) = &self.root {
            if root.borrow().get_id() == id {
                self.root = None;
                return true;
            }
            if let Some(container) = root.borrow_mut().as_container_mut() {
                if container.remove_view(id) {
                    return true;
                }
            }
        }
        false
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

    /// Clear the text selection in every view (overlays + root). Selectable
    /// views (`Label`, `RichText`) drop their highlight; everything else is a
    /// no-op. Called when a view starts a new selection so only one view holds
    /// a selection at a time. Uses immutable borrows only, so it is safe to call
    /// from inside a mouse handler mid-dispatch.
    pub fn deselect_text(&self) {
        for entry in &self.overlays {
            Self::deselect_recursive(&entry.element);
        }
        if let Some(root) = &self.root {
            Self::deselect_recursive(root);
        }
    }

    fn deselect_recursive(element: &Element) {
        let view = element.borrow();
        view.deselect_text();
        if let Some(container) = view.as_container() {
            for child in container.get_views() {
                Self::deselect_recursive(&child);
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
        // Transparent overlays (e.g. notification stack) cover the whole window —
        // resize them with the window so newly-added items lay out into the right slot.
        let typeface = self.typeface.clone();
        let entries: Vec<(Element, PopupMode)> = self.overlays.iter()
            .map(|e| (Rc::clone(&e.element), e.mode))
            .collect();
        for (el, mode) in entries {
            if mode == PopupMode::Transparent {
                el.borrow_mut().layout_content(0, 0, width as i32, height as i32, &typeface, scale);
            }
        }
    }

    pub fn relayout(&mut self) {
        self.needs_relayout = true;
    }

    /// Run the relayout synchronously, immediately. Useful when subsequent
    /// code in the same callback needs to read post-layout state (e.g. a
    /// RecyclerView's updated `max_scroll` after inserting an item).
    pub fn force_layout(&mut self) {
        self.needs_relayout = false;
        self.do_relayout();
    }

    fn do_relayout(&mut self) {
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

    /// Closes all `Popup`-mode overlays. `Modal` and `Transparent` overlays are preserved.
    pub fn close_all_popups(&mut self) {
        self.overlays.retain(|entry| entry.mode != PopupMode::Popup);
    }

    /// Returns true if there are any active popups/overlays.
    pub fn has_popups(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// Lazily ensures the notification stack overlay exists, returning a clone of its Element.
    fn ensure_notification_stack(&mut self) -> Element {
        if let Some(el) = &self.notification_stack {
            return Rc::clone(el);
        }
        let stack: Element = Rc::new(RefCell::new(NotificationStack::new()));
        // Lay out at full window size so it can place items at absolute coords.
        let typeface = self.typeface.clone();
        stack.borrow_mut().layout_content(0, 0, self.width as i32, self.height as i32, &typeface, self.scale);
        self.overlays.push(PopupEntry {
            element: Rc::clone(&stack),
            x: 0,
            y: 0,
            mode: PopupMode::Transparent,
        });
        self.notification_stack = Some(Rc::clone(&stack));
        stack
    }

    /// Push a notification view onto the stack with the given id and optional auto-dismiss timeout.
    /// If `id` already exists it is replaced. Clicking a child of `element` that calls
    /// `dismiss_notification(id)` (or `dismiss_notification_for(view)`) removes it.
    pub fn show_notification(&mut self, element: Element, id: &str, timeout: Option<Duration>) {
        let stack = self.ensure_notification_stack();
        let typeface = self.typeface.clone();
        let scale = self.scale;
        let s = stack.borrow();
        if let Some(stack_ref) = s.as_any().downcast_ref::<NotificationStack>() {
            stack_ref.push_item(id, element, timeout, &typeface, scale);
        }
    }

    /// Animate an item out and remove it.
    pub fn dismiss_notification(&mut self, id: &str) {
        if let Some(stack) = &self.notification_stack {
            if let Some(s) = stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                s.dismiss(id);
            }
        }
    }

    /// Remove an item without animation.
    pub fn dismiss_notification_immediate(&mut self, id: &str) {
        if let Some(stack) = &self.notification_stack {
            if let Some(s) = stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                s.dismiss_immediate(id);
            }
        }
    }

    /// Animate every active notification out of the stack.
    pub fn dismiss_all_notifications(&mut self) {
        if let Some(stack) = &self.notification_stack {
            if let Some(s) = stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                s.dismiss_all();
            }
        }
    }

    pub fn has_notification(&self, id: &str) -> bool {
        match &self.notification_stack {
            Some(stack) => stack.borrow()
                .as_any()
                .downcast_ref::<NotificationStack>()
                .map(|s| s.has(id))
                .unwrap_or(false),
            None => false,
        }
    }

    /// Walks `view`'s parent chain looking for an id that matches a current
    /// notification, then dismisses it. Convenient inside close-button callbacks
    /// so the caller doesn't need to capture the id by closure.
    pub fn dismiss_notification_for(&mut self, view: &dyn View) -> bool {
        let id = match &self.notification_stack {
            Some(stack) => match stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                Some(s) => s.id_for_descendant(view),
                None => None,
            },
            None => None,
        };
        if let Some(id) = id {
            self.dismiss_notification(&id);
            true
        } else {
            false
        }
    }

    pub fn start(&mut self) {
        if let Some(mut start) = self.on_start.take() {
            start(self);
        }
    }

    pub fn update(&mut self) -> bool {
        let mut redraw = false;
        // Process queued removals first; they may flip needs_relayout.
        if self.process_pending_removals() {
            redraw = true;
        }
        // Perform deferred relayout if requested
        if self.needs_relayout {
            self.needs_relayout = false;
            self.do_relayout();
            redraw = true;
        }
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
        // Update tooltip
        redraw |= self.update_tooltip();
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
        // Paint tooltip on top of everything
        if let Some(tooltip) = &self.tooltip_popup {
            tooltip.element.borrow().paint(Point::from((tooltip.x, tooltip.y)), theme);
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
                    if element.borrow().wants_raw_content() {
                        // Capture the literal inner markup (incl. inline tags like
                        // <b>…</b>) and hand it to the view instead of parsing the
                        // children as nested views. `read_text` returns the inner
                        // slice verbatim and consumes the matching end tag, so this
                        // element gets no `Event::End` — attach it as a leaf here.
                        let inner = reader.read_text(e.name()).map(|c| c.into_owned()).unwrap_or_default();
                        element.borrow_mut().set_any("html", &inner);
                        match stack.last() {
                            Some(parent) => {
                                element.borrow_mut().set_parent(Some(Rc::downgrade(parent)));
                                let mut ref_mut = parent.borrow_mut();
                                if let Some(container) = ref_mut.as_container_mut() {
                                    container.add_view(element);
                                }
                            }
                            None => ui.add_view(element),
                        }
                    } else {
                        stack.push(element);
                    }
                },
                Ok(Event::Empty(ref e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec()).unwrap();
                    if tag_name == "Item" {
                        // Handle <Item text="..."/> inside ComboBox
                        if let Some(parent) = stack.last() {
                            let text = UI::get_attribute(e, "text").unwrap_or_default();
                            let mut ref_mut = parent.borrow_mut();
                            if let Some(combo) = ref_mut.as_any_mut().downcast_mut::<ComboBox>() {
                                combo.add_item(&text);
                            }
                        }
                    } else {
                        let element = UI::parse_element(&mut ui, e);
                        let parent = stack.pop().unwrap();
                        {
                            element.borrow_mut().set_parent(Some(Rc::downgrade(&parent)));
                            let mut ref_mut = parent.borrow_mut();
                            let container = ref_mut.as_container_mut().unwrap();
                            container.add_view(element);
                        }
                        stack.push(parent);
                    }
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
                                element.borrow_mut().set_parent(Some(Rc::downgrade(&parent)));
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

    fn get_attribute(e: &BytesStart, name: &str) -> Option<String> {
        for attr in e.attributes().flatten() {
            let key = String::from_utf8(attr.key.0.to_vec()).unwrap();
            if key == name {
                return Some(match attr.value {
                    Cow::Borrowed(c) => String::from_utf8(c.to_vec()).unwrap(),
                    Cow::Owned(c) => String::from_utf8(c.to_vec()).unwrap(),
                });
            }
        }
        None
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

    /// Walks the view tree to find the deepest view under (x, y) that has a tooltip.
    /// Coordinates are in absolute window space.
    fn hit_test_tooltip(element: &Element, x: i32, y: i32, offset_x: i32, offset_y: i32) -> Option<(String, String)> {
        let view = element.borrow();
        let rect = view.get_rect();
        let abs_x = rect.min.x + offset_x;
        let abs_y = rect.min.y + offset_y;
        let abs_max_x = rect.max.x + offset_x;
        let abs_max_y = rect.max.y + offset_y;

        if x < abs_x || x >= abs_max_x || y < abs_y || y >= abs_max_y {
            return None;
        }

        // Check children first (deepest match wins)
        if let Some(container) = view.as_container() {
            for child in container.get_views().iter().rev() {
                if let Some(result) = Self::hit_test_tooltip(child, x, y, abs_x, abs_y) {
                    return Some(result);
                }
            }
        }

        // Then check this view
        if let Some(tooltip) = view.get_tooltip() {
            if !tooltip.is_empty() {
                return Some((view.get_id(), tooltip));
            }
        }

        None
    }

    fn update_tooltip(&mut self) -> bool {
        // Don't show tooltips when blocking popups are open. Transparent overlays
        // (e.g. notification stack) let tooltips on the underlying UI keep working.
        if self.overlays.iter().any(|e| e.mode != PopupMode::Transparent) {
            return self.dismiss_tooltip();
        }

        let hit = self.root.as_ref().and_then(|root| {
            Self::hit_test_tooltip(root, self.mouse_pos.x, self.mouse_pos.y, 0, 0)
        });

        match hit {
            Some((view_id, tooltip_text)) => {
                if self.tooltip_view_id.as_deref() == Some(&view_id) {
                    // Same view — check if it's time to show
                    if !self.tooltip_showing {
                        if let Some(start) = &self.tooltip_hover_start {
                            if start.elapsed().as_millis() >= TOOLTIP_DELAY_MS {
                                self.show_tooltip(&tooltip_text);
                                return true;
                            }
                        }
                    }
                    false
                } else {
                    // Different view — reset timer
                    let dismissed = self.dismiss_tooltip();
                    self.tooltip_view_id = Some(view_id);
                    self.tooltip_hover_start = Some(Instant::now());
                    dismissed
                }
            }
            None => {
                // No tooltip view under cursor
                self.dismiss_tooltip()
            }
        }
    }

    fn show_tooltip(&mut self, text: &str) {
        let label: Element = Rc::new(RefCell::new(Label::default()));
        {
            let mut l = label.borrow_mut();
            l.set_any("text", text);
            l.set_width(Dimension::Min);
            l.set_height(Dimension::Min);
            l.set_any("font_size", "18");
        }

        let frame: Element = Rc::new(RefCell::new(Frame::default()));
        {
            let mut f = frame.borrow_mut();
            f.set_id(TOOLTIP_ID);
            f.set_width(Dimension::Min);
            f.set_height(Dimension::Min);
            f.set_padding(2, 4, 4, 2);
            f.set_background(Some(0xFFFFFFDD));
            f.set_border_color(Some(0xFF808080));
            label.borrow_mut().set_parent(Some(Rc::downgrade(&frame)));
            f.as_container_mut().unwrap().add_view(label);
        }

        // Layout to determine size
        let typeface = self.typeface.clone();
        let w = self.width as i32;
        let h = self.height as i32;
        frame.borrow_mut().layout_content(0, 0, w, h, &typeface, self.scale);

        let rect = frame.borrow().get_rect();
        let pw = rect.width();
        let ph = rect.height();

        // Position below and to the right of the cursor
        let mut ox = self.mouse_pos.x;
        let mut oy = self.mouse_pos.y + 20;

        // Clamp to window bounds
        ox = ox.max(0).min(w - pw);
        oy = oy.max(0).min(h - ph);

        self.tooltip_popup = Some(TooltipPopup { element: frame, x: ox, y: oy });
        self.tooltip_showing = true;
    }

    fn dismiss_tooltip(&mut self) -> bool {
        if self.tooltip_showing {
            self.tooltip_popup = None;
            self.tooltip_showing = false;
            self.tooltip_view_id = None;
            self.tooltip_hover_start = None;
            true
        } else if self.tooltip_view_id.is_some() {
            // Had a pending tooltip but hadn't shown yet
            self.tooltip_view_id = None;
            self.tooltip_hover_start = None;
            false
        } else {
            false
        }
    }

    pub fn on_mouse_move(&mut self, position: Vector2<i32>) -> bool {
        self.mouse_pos = position;
        // Re-evaluate the cursor from scratch each move: views over a link
        // re-request `Pointer` during dispatch; anything left is the default.
        self.requested_cursor = None;
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

    /// Requests a cursor shape for the current `on_mouse_move` dispatch. Called
    /// by a view (e.g. a hovered link) from its `on_mouse_move`. First write
    /// wins, so the topmost view under the pointer decides the cursor (dispatch
    /// visits views topmost-first).
    pub fn request_cursor(&mut self, cursor: MouseCursorType) {
        if self.requested_cursor.is_none() {
            self.requested_cursor = Some(cursor);
        }
    }

    /// The cursor shape resolved by the last `on_mouse_move`, falling back to
    /// the default arrow when no view requested one. Read by the window handler
    /// to drive the OS cursor.
    pub fn current_cursor(&self) -> MouseCursorType {
        self.requested_cursor.unwrap_or(MouseCursorType::Default)
    }

    pub fn on_mouse_button_down(&mut self, position: Vector2<i32>, button: MouseButton) -> bool {
        self.dismiss_tooltip();
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
        // Click missed all overlays — dismiss Popup-mode overlays only.
        // Transparent overlays (e.g. notification stack) let the click fall through.
        if self.overlays.iter().any(|e| e.mode == PopupMode::Popup) {
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