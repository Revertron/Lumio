use std::cell::{Cell, RefCell};
use std::cmp::max;
use std::rc::Rc;

use crate::text::{TextBlock, TextOptions};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Renderer, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, point, rect};
use crate::ui::{PopupDirection, PopupMode, UI};
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, FieldsTexted, Gravity, Visibility};
use crate::views::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH};
use crate::styles::selector::FontSelector;

const ARROW_AREA_WIDTH: i32 = 16;
const ITEM_HEIGHT: i32 = 28;
const ITEM_PADDING_LEFT: i32 = 6;
const ITEM_PADDING_RIGHT: i32 = 6;
const MIN_THUMB_SIZE: i32 = 16;
/// The sunken bevel `edit.body` draws around the closed box — two dip, an outer
/// and an inner line (drawables/edit_field_classic_body.xml).
const FIELD_BORDER: i32 = 2;
/// The flat outline `popup.body` draws around the dropdown — one dip
/// (drawables/popup_classic_body.xml). It is also the dropdown's padding: the
/// list fills the popup right up to the border on all four sides, as it does on
/// Windows, so a highlighted row has no white strip beside it.
const POPUP_BORDER: i32 = 1;

// ─── ComboBox ────────────────────────────────────────────────────────────────

pub struct ComboBox {
    state: RefCell<FieldsTexted>,
    items: RefCell<Vec<String>>,
    selected: RefCell<Option<usize>>,
    deferred_selected: RefCell<Option<usize>>,
    pending_selection: Rc<RefCell<Option<usize>>>,
    dropdown_id: RefCell<Option<String>>,
}

impl HasMainFields for ComboBox {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for ComboBox {}

#[allow(dead_code)]
impl ComboBox {
    pub fn new(rect: Rect<i32>, text_size: f32) -> ComboBox {
        let mut main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        main.padding = Borders::with_padding(4);
        ComboBox {
            state: RefCell::new(FieldsTexted {
                main,
                text: String::new(),
                text_size,
                line_height: 0f32,
                single_line: true,
                cached_text: None,
                font: FontSelector::new(),
            }),
            items: RefCell::new(Vec::new()),
            selected: RefCell::new(None),
            deferred_selected: RefCell::new(None),
            pending_selection: Rc::new(RefCell::new(None)),
            dropdown_id: RefCell::new(None),
        }
    }

    pub fn add_item(&self, text: &str) {
        self.items.borrow_mut().push(text.to_owned());
    }

    /// Removes all items and clears the selection and the displayed text.
    pub fn clear_items(&self) {
        self.items.borrow_mut().clear();
        *self.selected.borrow_mut() = None;
        self.set_display_text("");
    }

    /// Test hook: stores a pending selection exactly as a dropdown click
    /// would, so tests can drive the update-tick dispatch path.
    #[cfg(test)]
    pub fn simulate_pending_selection(&self, index: usize) {
        *self.pending_selection.borrow_mut() = Some(index);
    }

    pub fn on_change(&mut self, func: EventCallback) {
        self.base_on_event(EventType::SelectionChanged, func);
    }

    pub fn get_selected_index(&self) -> Option<usize> {
        *self.selected.borrow()
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let selected = *self.selected.borrow();
        selected.map(|i| self.items.borrow()[i].clone())
    }

    /// Whether the dropdown popup is currently open.
    pub fn is_open(&self) -> bool {
        self.dropdown_id.borrow().is_some()
    }

    pub fn set_selected(&self, index: usize) {
        let items = self.items.borrow();
        if index < items.len() {
            *self.selected.borrow_mut() = Some(index);
            let text = items[index].clone();
            drop(items);
            self.set_display_text(&text);
        }
    }

    pub fn item_count(&self) -> usize {
        self.items.borrow().len()
    }

    fn set_display_text(&self, text: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.text.clear();
            state.text.push_str(text);
            state.cached_text = None;
        }
        let scale = self.state.borrow().main.scale;
        self.layout_text(self.get_rect_width(), scale);
    }

    fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        self.state.borrow().main.font_manager.get_typeface(parent_typeface)
    }

    fn set_font(&self, font_name: &str) {
        self.state.borrow_mut().main.font_manager.set_font(font_name);
    }

    fn set_font_style(&self, style: &str) {
        self.state.borrow_mut().main.font_manager.set_font_style(style);
    }

    fn set_font_size(&self, size: f32) {
        let mut state = self.state.borrow_mut();
        state.main.font_manager.set_font_size(size);
        state.cached_text = None;
    }

    fn layout_text(&self, max_width: i32, scale: f64) {
        if max_width <= 0 {
            self.state.borrow_mut().cached_text = None;
            return;
        }
        let typeface = self.state.borrow().main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let scale_i = scale.round() as i32;
                let arrow_w = ARROW_AREA_WIDTH * scale_i;
                let width = max_width - arrow_w;
                if width <= 0 {
                    return;
                }
                let options = TextOptions::new();
                let base_size = typeface.font_size.unwrap_or(self.state.borrow().text_size);
                let size = base_size * scale_i as f32;
                let text = font.layout_text(&self.state.borrow().text, size, options);
                self.state.borrow_mut().cached_text = Some(text);
            }
        }
    }

    fn open_dropdown(&self, ui: &mut UI) {
        let items: Vec<String> = self.items.borrow().clone();
        if items.is_empty() {
            return;
        }

        let typeface = self.state.borrow().main.font_manager.get();
        let scale = self.state.borrow().main.scale;
        let width = self.get_rect_width();

        let mut dropdown = ComboDropdown::new(items, typeface, scale, width, Rc::clone(&self.pending_selection), *self.selected.borrow());

        // Placement, the way Windows does it: below the box when the whole list
        // fits there, else flipped above when it fits there, else on whichever
        // side has more room, shortened to it (the list scrolls).
        let pos = self.get_absolute_position();
        let height = self.get_rect_height();
        let wanted = dropdown.natural_height();
        let below = ui.get_height() as i32 - (pos.y + height);
        let above = pos.y;
        let (direction, anchor_y, max_height) = if wanted <= below {
            (PopupDirection::BottomRight, pos.y + height, wanted)
        } else if wanted <= above {
            (PopupDirection::TopRight, pos.y, wanted)
        } else if below >= above {
            (PopupDirection::BottomRight, pos.y + height, below)
        } else {
            (PopupDirection::TopRight, pos.y, above)
        };
        dropdown.set_max_height(max_height);

        let element: Element = Rc::new(RefCell::new(dropdown));
        let id = element.borrow().get_id();
        *self.dropdown_id.borrow_mut() = Some(id);

        ui.show_popup(element, pos.x, anchor_y, direction, PopupMode::Popup);
    }
}

impl View for ComboBox {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "items" => {
                for item in value.split('|') {
                    let trimmed = item.trim();
                    if !trimmed.is_empty() {
                        self.add_item(trimmed);
                    }
                }
            }
            "selected" => {
                if let Ok(index) = value.parse::<usize>() {
                    if index < self.items.borrow().len() {
                        self.set_selected(index);
                    } else {
                        // Items not yet added (e.g. nested <Item> tags) — defer
                        *self.deferred_selected.borrow_mut() = Some(index);
                    }
                }
            }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
            }
            &_ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        // Apply deferred selection now that items have been added
        if let Some(index) = self.deferred_selected.borrow_mut().take() {
            self.set_selected(index);
        }
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface));
        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let max_width = width.max(BUTTON_MIN_WIDTH) - horizontal;
        let max_height = height.max(BUTTON_MIN_HEIGHT) - vertical;
        let (new_width, _new_height) = self.calculate_size(max_width, max_height, scale);
        self.layout_text(new_width, scale);
        let (width, height) = self.calculate_bounded_size(width, height, scale);
        let rect = rect((x, y), (x + width, y + height));
        self.set_rect(rect);
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let state = self.state.borrow();
        match &state.cached_text {
            Some(text) => text.width() <= width as f32 && text.height() <= height as f32,
            None => width <= BUTTON_MIN_WIDTH && height <= BUTTON_MIN_HEIGHT
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Renderer) {
        let state = self.state.borrow();
        let scale = state.main.scale;
        let mut rect = state.main.rect;
        rect.move_by(origin);

        let arrow_w = (ARROW_AREA_WIDTH as f64 * scale).round() as i32;
        let border = (FIELD_BORDER as f64 * scale).round() as i32;
        let button_rect = crate::types::rect(
            (rect.max.x - arrow_w - border, rect.min.y + border),
            (rect.max.x - border, rect.max.y - border),
        );

        let focused = state.main.state.focused;

        theme.push_clip();
        theme.clip_rect(rect);

        // Step 1: Draw full edit-field area (white background + sunken border).
        // A 9-patch background replaces the back and the body components; the
        // arrow button and focus highlight stay drawable-based.
        let ninepatch = self.base_draw_ninepatch(theme, rect);
        if !ninepatch {
            theme.draw_component("edit.back", rect, state.main.state);
        }

        // Step 2: When focused, highlight the text area with the selection colour
        // and draw the dashed focus rectangle inside it (rather than on the arrow
        // button — see step 5).
        if focused {
            let field_rect = crate::types::rect(
                (rect.min.x + border, rect.min.y + border),
                (button_rect.min.x, rect.max.y - border),
            );
            theme.draw_component("combo.focus", field_rect, state.main.state);
        }

        // Step 3: Draw selected item text (left-aligned inside edit area)
        if let Some(text) = &state.cached_text {
            let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as f32;
            let x = rect.min.x as f32 + border as f32 + pad_left;
            let y = rect.min.y as f32 + (self.get_rect_height() as f32 - text.height()) / 2.0;
            let color = if focused {
                crate::themes::selection_text_color(theme.color("selection"))
            } else {
                theme.get_text_color(state.main.state, state.main.foreground.as_ref())
            };
            theme.draw_text(x.round(), y.round(), color, text);
        }

        // Step 4: Draw sunken border over entire rect
        if !ninepatch {
            theme.draw_component("edit.body", rect, state.main.state);
        }

        // Step 5: Draw raised button with arrow inside the sunken area. Clear the
        // focused flag so the arrow button doesn't draw its own focus rectangle;
        // focus is shown in the text field above.
        let mut button_state = state.main.state;
        button_state.focused = false;
        theme.draw_component("button.back", button_rect, button_state);
        theme.draw_component("combo.arrow", button_rect, button_state);
        theme.draw_component("button.body", button_rect, button_state);

        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().main.state)
    }

    fn get_rect(&self) -> Rect<i32> {
        self.base_get_rect()
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.base_set_rect(rect);
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.base_get_padding(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_padding(top, left, right, bottom);
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.base_get_margin(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_margin(top, left, right, bottom);
    }

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let state = self.state.borrow();
        let scale = state.main.scale.round() as i32;
        let arrow_w = ARROW_AREA_WIDTH * scale;
        match &state.cached_text {
            None => (BUTTON_MIN_WIDTH.max(arrow_w + 20), BUTTON_MIN_HEIGHT),
            Some(text) => {
                let width = text.width().ceil() as i32 + arrow_w + ITEM_PADDING_LEFT * scale + ITEM_PADDING_RIGHT * scale;
                let height = max(text.height().ceil() as i32, BUTTON_MIN_HEIGHT);
                (width, height)
            }
        }
    }

    fn is_focused(&self) -> bool {
        self.base_is_focused()
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn set_focused(&self, focused: bool) {
        self.base_set_focused(focused);
    }

    fn set_focusable(&self, focusable: bool) {
        self.base_set_focusable(focusable);
    }

    fn set_width(&mut self, width: Dimension) {
        self.base_set_width(width);
    }

    fn set_height(&mut self, height: Dimension) {
        self.base_set_height(height);
    }

    fn set_scale(&mut self, scale: f64) {
        self.base_set_scale(scale);
    }

    fn set_id(&mut self, id: &str) {
        self.base_set_id(id);
    }

    fn get_id(&self) -> String {
        self.base_get_id()
    }
    fn get_tooltip(&self) -> Option<String> {
        self.base_get_tooltip()
    }

    fn get_content_description(&self) -> Option<String> {
        self.base_get_content_description()
    }

    fn set_content_description(&mut self, description: Option<String>) {
        self.base_set_content_description(description);
    }

    fn get_labelled_by(&self) -> Option<String> {
        self.base_get_labelled_by()
    }

    fn set_labelled_by(&mut self, view_id: Option<String>) {
        self.base_set_labelled_by(view_id);
    }
    fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.base_set_tooltip(tooltip);
    }

    fn get_background(&self) -> Option<u32> {
        self.base_get_background()
    }
    fn set_background(&mut self, color: Option<u32>) {
        self.base_set_background(color);
    }
    fn get_border_color(&self) -> Option<u32> {
        self.base_get_border_color()
    }
    fn set_border_color(&mut self, color: Option<u32>) {
        self.base_set_border_color(color);
    }

    fn is_enabled(&self) -> bool {
        self.base_is_enabled()
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.base_set_enabled(enabled);
    }
    fn get_visibility(&self) -> Visibility {
        self.base_get_visibility()
    }
    fn set_visibility(&mut self, visibility: Visibility) {
        self.base_set_visibility(visibility);
    }

    fn on_event(&mut self, event: EventType, func: EventCallback) {
        self.base_on_event(event, func);
    }

    fn has_listener(&self, event: EventType) -> bool {
        self.base_has_listener(event)
    }

    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool {
        self.base_fire_event(ui, event, data)
    }

    fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::ComboBox);
        if let Some(text) = self.get_selected_text() {
            node.set_value(text);
        }
        node.set_expanded(self.is_open());
        node.add_action(accesskit::Action::Click);
        node
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.open_dropdown(ui);
        true
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        // A dropdown dismissed without a pick (Escape, or a click outside) is
        // dropped by the UI, which has no way to tell this box. Let go of the
        // stale handle, or `is_open()` stays true forever and the keyboard can
        // never reopen the list.
        let stale = match &*self.dropdown_id.borrow() {
            Some(id) => !ui.is_popup_open(id),
            None => false,
        };
        if stale {
            *self.dropdown_id.borrow_mut() = None;
        }

        let pending = self.pending_selection.borrow_mut().take();
        if let Some(index) = pending {
            let items = self.items.borrow();
            if index < items.len() {
                let text = items[index].clone();
                drop(items);
                *self.selected.borrow_mut() = Some(index);
                self.set_display_text(&text);
                *self.dropdown_id.borrow_mut() = None;

                // This view is mutably borrowed by the update tree-walk, so
                // the event must fire after the walk (handlers use get_view).
                ui.defer_event(&self.get_id(), EventType::SelectionChanged, EventData::Selected(index));
                return true;
            }
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Point<i32>) -> bool {
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        let old_state = self.state.borrow().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow().main.state != old_state
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        if hit {
            let mut state = self.state.borrow_mut();
            if matches!(button, MouseButton::Left) {
                state.main.state.pressed = true;
            }
            state.main.state.focused = true;
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        if matches!(button, MouseButton::Left) {
            if self.state.borrow().main.state.pressed {
                if hit {
                    self.click(ui);
                }
                self.state.borrow_mut().main.state.pressed = false;
                return true;
            }
        }
        false
    }

    // Space, Enter or Alt+Down open the dropdown when the box is focused;
    // the dropdown itself handles arrow keys and Enter as an overlay.
    // With the dropdown closed, plain Up/Down step through the items in place,
    // as they do on Windows.
    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        let open = match virtual_key_code {
            Some(VirtualKeyCode::Space | VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => true,
            Some(VirtualKeyCode::Down) => state.alt(),
            _ => false,
        };
        if open && !self.is_open() {
            self.open_dropdown(ui);
            return true;
        }
        if !self.is_open() && !state.alt() {
            let step = match virtual_key_code {
                Some(VirtualKeyCode::Down) => 1i32,
                Some(VirtualKeyCode::Up) => -1i32,
                _ => return false,
            };
            let count = self.items.borrow().len();
            if count == 0 {
                return false;
            }
            // Step off the pending index when one is queued: two key presses can
            // land in the same update tick, and the first has not been applied
            // to `selected` yet.
            let current = self.pending_selection.borrow().or(*self.selected.borrow());
            let next = match current {
                Some(i) => (i as i32 + step).clamp(0, count as i32 - 1) as usize,
                None => if step > 0 { 0 } else { count - 1 },
            };
            if Some(next) != current {
                // The same route a dropdown click takes: `update()` applies it
                // and fires SelectionChanged once all borrows are free.
                *self.pending_selection.borrow_mut() = Some(next);
            }
            return true;
        }
        false
    }
}

impl Default for ComboBox {
    fn default() -> Self {
        let rect = rect((0, 0), (120, 24));
        ComboBox::new(rect, crate::drawing::current_text_size("text"))
    }
}

// ─── ComboDropdown (private) ─────────────────────────────────────────────────

struct ComboDropdown {
    state: RefCell<FieldsMain>,
    items: Vec<String>,
    cached_texts: RefCell<Vec<Option<TextBlock>>>,
    hovered: RefCell<Option<usize>>,
    pressed: RefCell<Option<usize>>,
    pending_selection: Rc<RefCell<Option<usize>>>,
    typeface: Option<Typeface>,
    combo_width: i32,
    /// The room the ComboBox found for the list on the side it chose. Layout
    /// never grows past it, so the popup stays clear of the window edge.
    max_height: i32,
    /// Vertical scroll offset, 0 at the top and negative once scrolled down
    /// (the sign convention the other scrolling views use).
    scroll_y: Cell<i32>,
    v_scroll_visible: Cell<bool>,
    dragging_thumb: Cell<bool>,
    drag_anchor_y: Cell<i32>,
    drag_anchor_scroll: Cell<i32>,
}

impl HasMainFields for ComboDropdown {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for ComboDropdown {}

impl ComboDropdown {
    fn new(
        items: Vec<String>,
        typeface: Option<Typeface>,
        scale: f64,
        combo_width: i32,
        pending_selection: Rc<RefCell<Option<usize>>>,
        selected: Option<usize>,
    ) -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (combo_width, 100)), Dimension::Min, Dimension::Min);
        main.padding = Borders::with_padding(POPUP_BORDER);
        main.state.focusable = false;
        main.scale = scale;
        let cached_texts = vec![None; items.len()];
        ComboDropdown {
            state: RefCell::new(main),
            items,
            cached_texts: RefCell::new(cached_texts),
            // Keyboard navigation starts from the current selection.
            hovered: RefCell::new(selected),
            pressed: RefCell::new(None),
            pending_selection,
            typeface,
            combo_width,
            max_height: i32::MAX,
            scroll_y: Cell::new(0),
            v_scroll_visible: Cell::new(false),
            dragging_thumb: Cell::new(false),
            drag_anchor_y: Cell::new(0),
            drag_anchor_scroll: Cell::new(0),
        }
    }

    fn layout_texts(&self, scale: f64) {
        let typeface = match &self.typeface {
            Some(t) => t,
            None => return,
        };
        let base_size = typeface.font_size.unwrap_or_else(|| crate::drawing::current_text_size("text"));
        let text_size = base_size * scale as f32;
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            let mut cached = self.cached_texts.borrow_mut();
            for (i, item) in self.items.iter().enumerate() {
                if cached[i].is_none() {
                    let options = TextOptions::new();
                    let block = font.layout_text(item, text_size, options);
                    cached[i] = Some(block);
                }
            }
        }
    }

    fn item_height(&self) -> i32 {
        let scale = self.state.borrow().scale;
        (ITEM_HEIGHT as f64 * scale).round() as i32
    }

    /// The height the list wants: every item, plus the popup's padding.
    fn natural_height(&self) -> i32 {
        let state = self.state.borrow();
        let padding = state.padding.scaled(state.scale);
        drop(state);
        padding.top + self.content_height() + padding.bottom
    }

    fn set_max_height(&mut self, height: i32) {
        self.max_height = height;
    }

    fn scrollbar_thickness(&self) -> i32 {
        let scale = self.state.borrow().scale;
        (crate::drawing::current_dimension("scrollbar.thickness") as f64 * scale).round() as i32
    }

    /// Where the rows live: the popup rect minus its border, and minus the
    /// scrollbar when one is showing. Rows fill it edge to edge, so a
    /// highlighted one touches the border on every side. `origin` shifts it
    /// into window space.
    fn body_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);
        let padding = state.padding.scaled(state.scale);
        drop(state);
        let mut max_x = r.max.x - padding.right;
        if self.v_scroll_visible.get() {
            max_x -= self.scrollbar_thickness();
        }
        let min_x = r.min.x + padding.left;
        rect((min_x, r.min.y + padding.top), (max_x.max(min_x), r.max.y - padding.bottom))
    }

    fn body_height(&self) -> i32 {
        let state = self.state.borrow();
        let padding = state.padding.scaled(state.scale);
        (state.rect.height() - padding.top - padding.bottom).max(0)
    }

    fn content_height(&self) -> i32 {
        self.items.len() as i32 * self.item_height()
    }

    fn clamp_scroll(&self) {
        let max_neg = -(self.content_height() - self.body_height()).max(0);
        self.scroll_y.set(self.scroll_y.get().clamp(max_neg, 0));
    }

    /// Scrolls the least amount that brings item `idx` fully into view.
    fn ensure_visible(&self, idx: usize) {
        let item_h = self.item_height().max(1);
        let bh = self.body_height();
        let top = idx as i32 * item_h;
        let bottom = top + item_h;
        let cur = self.scroll_y.get();
        if top + cur < 0 {
            self.scroll_y.set(-top);
        } else if bottom + cur > bh {
            self.scroll_y.set(bh - bottom);
        }
        self.clamp_scroll();
    }

    // Scrollbar geometry (vertical only), matching the TreeView chrome.

    fn v_scrollbar_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);
        let padding = state.padding.scaled(state.scale);
        drop(state);
        let thickness = self.scrollbar_thickness();
        let x_max = r.max.x - padding.right;
        rect((x_max - thickness, r.min.y + padding.top), (x_max, r.max.y - padding.bottom))
    }

    fn v_arrow_top_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        rect((sb.min.x, sb.min.y), (sb.max.x, sb.min.y + size))
    }

    fn v_arrow_bottom_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        rect((sb.min.x, sb.max.y - size), (sb.max.x, sb.max.y))
    }

    fn v_track_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let sb = self.v_scrollbar_rect(origin);
        let size = self.scrollbar_thickness();
        if sb.height() < 2 * size {
            return rect((sb.min.x, sb.min.y), (sb.max.x, sb.min.y));
        }
        rect((sb.min.x, sb.min.y + size), (sb.max.x, sb.max.y - size))
    }

    fn v_thumb_rect(&self, origin: Point<i32>) -> Rect<i32> {
        let track = self.v_track_rect(origin);
        let bh = self.body_height().max(1);
        let ch = self.content_height().max(1);
        let track_len = track.height();
        if track_len <= 0 {
            return track;
        }
        let thumb_len = ((bh as f64 / ch as f64) * track_len as f64).round() as i32;
        let thumb_len = thumb_len.max(MIN_THUMB_SIZE).min(track_len.max(MIN_THUMB_SIZE));
        let scroll_range = (ch - bh).max(0);
        let thumb_range = (track_len - thumb_len).max(0);
        let pos = if scroll_range > 0 {
            (-self.scroll_y.get() as f64 / scroll_range as f64 * thumb_range as f64).round() as i32
        } else { 0 };
        rect((track.min.x, track.min.y + pos), (track.max.x, track.min.y + pos + thumb_len))
    }

    fn get_hit_item(&self, x: i32, y: i32) -> Option<usize> {
        let rows = self.body_rect(point(0, 0));
        if !rows.hit((x, y)) {
            return None;
        }
        let item_h = self.item_height().max(1);
        let local_y = y - rows.min.y - self.scroll_y.get();
        if local_y < 0 {
            return None;
        }
        let index = (local_y / item_h) as usize;
        if index < self.items.len() {
            Some(index)
        } else {
            None
        }
    }

    /// Moves the keyboard highlight to `index` and scrolls it into view.
    fn highlight(&self, index: usize) {
        *self.hovered.borrow_mut() = Some(index);
        self.ensure_visible(index);
    }
}

impl View for ComboDropdown {
    fn set_any(&mut self, _name: &str, _value: &str) {}

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, _typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.base_set_scale(scale);
        self.layout_texts(scale);

        let padding = self.get_padding(scale);
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;

        let content_w = self.combo_width - padding.left - padding.right;
        let content_h = item_h * self.items.len() as i32;

        // A list too tall for the room the box found shows whole items only and
        // scrolls the rest, the way the Windows dropdown does. One item always
        // shows, however little room there is — a sliver of a popup is no use.
        let available_h = (height.min(self.max_height) - padding.top - padding.bottom).max(0);
        let visible_h = if content_h > available_h {
            (available_h / item_h.max(1) * item_h).max(item_h)
        } else {
            content_h
        };

        let total_w = (padding.left + content_w + padding.right).min(width);
        let total_h = padding.top + visible_h + padding.bottom;

        let r = rect((x, y), (x + total_w, y + total_h));
        self.set_rect(r);

        self.v_scroll_visible.set(content_h > visible_h);
        // Open showing the current selection, not always the top of the list.
        if let Some(index) = *self.hovered.borrow() {
            self.ensure_visible(index);
        }
        self.clamp_scroll();
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let (cw, ch) = self.get_content_size();
        cw <= width && ch <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Renderer) {
        let view_state = self.state.borrow().state;
        let scale = self.state.borrow().scale;
        let mut r = self.state.borrow().rect;
        r.move_by(origin);

        theme.push_clip();
        theme.clip_rect(r);

        // Background
        theme.draw_component("edit.back", r, view_state);

        // Rows fill the popup up to its border, but their text has to line up
        // with the closed box's, which clears the wider sunken bevel — so the
        // text is measured from the popup's outer edge, not from the row's.
        let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as i32;
        let text_x = r.min.x + (FIELD_BORDER as f64 * scale).round() as i32 + pad_left;
        let item_h = self.item_height().max(1);
        let rows = self.body_rect(origin);
        let scroll_y = self.scroll_y.get();

        theme.push_clip();
        theme.clip_rect(rows);

        let hovered = *self.hovered.borrow();
        let cached = self.cached_texts.borrow();

        // Only the items the body can show are worth drawing.
        let n = self.items.len();
        let first = ((-scroll_y) / item_h).max(0) as usize;
        let last = (((rows.height() - scroll_y).max(0) / item_h) as usize + 1).min(n);

        for i in first..last {
            let top = rows.min.y + i as i32 * item_h + scroll_y;
            let item_rect = rect((rows.min.x, top), (rows.max.x, top + item_h));

            let text_color = if hovered == Some(i) {
                theme.draw_rect(item_rect, theme.color("item_highlight"));
                theme.color("item_highlight_text")
            } else {
                theme.color("text")
            };

            if let Some(Some(text)) = cached.get(i) {
                let text_y = top + (item_h as f32 - text.height()) as i32 / 2;
                theme.draw_text(text_x as f32, text_y as f32, text_color, text);
            }
        }
        drop(cached);

        theme.pop_clip();

        // ---- Scrollbar ----
        if self.v_scroll_visible.get() {
            let unfocused = ViewState::no_focus();
            let track = self.v_track_rect(origin);
            let thumb = self.v_thumb_rect(origin);
            let arrow_top = self.v_arrow_top_rect(origin);
            let arrow_bottom = self.v_arrow_bottom_rect(origin);
            for (arrow_rect, role) in [(arrow_top, "scrollbar.arrow.up"), (arrow_bottom, "scrollbar.arrow.down")] {
                theme.draw_component("button.back", arrow_rect, unfocused);
                theme.draw_component("button.body", arrow_rect, unfocused);
                theme.draw_component(role, arrow_rect, unfocused);
            }
            theme.draw_component("scrollbar.track", track, unfocused);
            let mut thumb_state = unfocused;
            thumb_state.pressed = self.dragging_thumb.get();
            theme.draw_component("button.back", thumb, thumb_state);
            theme.draw_component("button.body", thumb, thumb_state);
        }

        // Border: plain solid outline (a dropdown is a floating popup, not a
        // sunken field).
        theme.draw_component("popup.body", r, view_state);

        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().state)
    }

    fn get_rect(&self) -> Rect<i32> {
        self.base_get_rect()
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.base_set_rect(rect);
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.base_get_padding(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_padding(top, left, right, bottom);
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.base_get_margin(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_margin(top, left, right, bottom);
    }

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let state = self.state.borrow();
        let scale = state.scale;
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;
        let w = self.combo_width;
        let h = item_h * self.items.len() as i32;
        (w, h)
    }

    fn is_focused(&self) -> bool { false }
    fn is_break(&self) -> bool { false }
    fn set_focused(&self, _focused: bool) {}

    fn set_focusable(&self, focusable: bool) {
        self.base_set_focusable(focusable);
    }

    fn set_width(&mut self, width: Dimension) {
        self.base_set_width(width);
    }

    fn set_height(&mut self, height: Dimension) {
        self.base_set_height(height);
    }

    fn set_scale(&mut self, scale: f64) {
        self.base_set_scale(scale);
    }

    fn set_id(&mut self, id: &str) {
        self.base_set_id(id);
    }

    fn get_id(&self) -> String {
        self.base_get_id()
    }
    fn get_tooltip(&self) -> Option<String> {
        self.base_get_tooltip()
    }

    fn get_content_description(&self) -> Option<String> {
        self.base_get_content_description()
    }

    fn set_content_description(&mut self, description: Option<String>) {
        self.base_set_content_description(description);
    }

    fn get_labelled_by(&self) -> Option<String> {
        self.base_get_labelled_by()
    }

    fn set_labelled_by(&mut self, view_id: Option<String>) {
        self.base_set_labelled_by(view_id);
    }
    fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.base_set_tooltip(tooltip);
    }

    fn get_background(&self) -> Option<u32> {
        self.base_get_background()
    }
    fn set_background(&mut self, color: Option<u32>) {
        self.base_set_background(color);
    }
    fn get_border_color(&self) -> Option<u32> {
        self.base_get_border_color()
    }
    fn set_border_color(&mut self, color: Option<u32>) {
        self.base_set_border_color(color);
    }

    fn on_event(&mut self, event: EventType, func: EventCallback) {
        self.base_on_event(event, func);
    }

    fn has_listener(&self, event: EventType) -> bool {
        self.base_has_listener(event)
    }

    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool {
        self.base_fire_event(ui, event, data)
    }

    fn click(&self, _ui: &mut UI) -> bool { false }

    fn on_mouse_move(&self, _ui: &mut UI, position: Point<i32>) -> bool {
        if self.dragging_thumb.get() {
            let r = self.state.borrow().rect;
            let local_y = position.y - r.min.y;
            let bh = self.body_height().max(1);
            let ch = self.content_height().max(1);
            let track_len = self.v_track_rect(point(0, 0)).height().max(1);
            let thumb_len = ((bh as f64 / ch as f64) * track_len as f64).round() as i32;
            let thumb_len = thumb_len.max(MIN_THUMB_SIZE).min(track_len.max(MIN_THUMB_SIZE));
            let scroll_range = (ch - bh).max(1) as f64;
            let thumb_range = (track_len - thumb_len).max(1) as f64;
            let dy = (local_y - self.drag_anchor_y.get()) as f64;
            let new_scroll = self.drag_anchor_scroll.get() as f64 - dy * (scroll_range / thumb_range);
            self.scroll_y.set(new_scroll.round() as i32);
            self.clamp_scroll();
            return true;
        }
        let hit_item = self.get_hit_item(position.x, position.y);
        let old = *self.hovered.borrow();
        *self.hovered.borrow_mut() = hit_item;
        old != hit_item
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        // A press anywhere inside the popup is the popup's own — consuming it
        // keeps the scrollbar and the border from dismissing the dropdown.
        if !self.state.borrow().rect.hit((position.x, position.y)) {
            return false;
        }

        if self.v_scroll_visible.get() {
            let zero = point(0, 0);
            let item_h = self.item_height().max(1);
            let thumb = self.v_thumb_rect(zero);
            if thumb.hit((position.x, position.y)) {
                self.dragging_thumb.set(true);
                self.drag_anchor_y.set(position.y - self.state.borrow().rect.min.y);
                self.drag_anchor_scroll.set(self.scroll_y.get());
                return true;
            }
            if self.v_arrow_top_rect(zero).hit((position.x, position.y)) {
                self.scroll_y.set(self.scroll_y.get() + item_h);
                self.clamp_scroll();
                return true;
            }
            if self.v_arrow_bottom_rect(zero).hit((position.x, position.y)) {
                self.scroll_y.set(self.scroll_y.get() - item_h);
                self.clamp_scroll();
                return true;
            }
            if self.v_scrollbar_rect(zero).hit((position.x, position.y)) {
                // Track click beside the thumb: page-scroll toward it.
                let dir = if position.y < thumb.min.y { 1 } else { -1 };
                self.scroll_y.set(self.scroll_y.get() + dir * self.body_height());
                self.clamp_scroll();
                return true;
            }
        }

        *self.pressed.borrow_mut() = self.get_hit_item(position.x, position.y);
        true
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        if self.dragging_thumb.replace(false) {
            return true;
        }
        let pressed = self.pressed.borrow_mut().take();
        let hit = self.get_hit_item(position.x, position.y);
        if let (Some(p), Some(h)) = (pressed, hit) {
            if p == h {
                *self.pending_selection.borrow_mut() = Some(h);
                let id = self.get_id();
                ui.close_popup(&id);
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        if !self.state.borrow().rect.hit((position.x, position.y)) {
            return false;
        }
        if !self.v_scroll_visible.get() {
            // Nothing to scroll, but the list is under the pointer: the view
            // behind the popup must not scroll instead.
            return true;
        }
        let item_h = self.item_height().max(1);
        let bh = self.body_height();
        let dy = match distance {
            MouseScrollDistance::Lines { y, .. } => y as i32 * item_h,
            MouseScrollDistance::Pixels { y, .. } => y as i32,
            MouseScrollDistance::Pages { y, .. } => y as i32 * bh,
        };
        self.scroll_y.set(self.scroll_y.get() + dy);
        self.clamp_scroll();
        true
    }

    // Keyboard navigation while the dropdown is open (dispatched as an
    // overlay, before the root tree): arrows, Page and Home/End move the
    // highlight — scrolling it into view when the list is longer than the
    // popup — and Enter commits it. Esc is handled by the generic
    // popup-dismiss path.
    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        let count = self.items.len();
        if count == 0 {
            return false;
        }
        let Some(code) = virtual_key_code else { return false; };
        let current = *self.hovered.borrow();
        // A page is what the popup can show at once, one item at the very least.
        let page = (self.body_height() / self.item_height().max(1)).max(1) as usize;
        match code {
            VirtualKeyCode::Down => {
                self.highlight(current.map(|i| (i + 1).min(count - 1)).unwrap_or(0));
                true
            }
            VirtualKeyCode::Up => {
                self.highlight(current.map(|i| i.saturating_sub(1)).unwrap_or(count - 1));
                true
            }
            VirtualKeyCode::PageDown => {
                self.highlight(current.map(|i| (i + page).min(count - 1)).unwrap_or(count - 1));
                true
            }
            VirtualKeyCode::PageUp => {
                self.highlight(current.map(|i| i.saturating_sub(page)).unwrap_or(0));
                true
            }
            VirtualKeyCode::Home => {
                self.highlight(0);
                true
            }
            VirtualKeyCode::End => {
                self.highlight(count - 1);
                true
            }
            VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                if let Some(i) = current {
                    *self.pending_selection.borrow_mut() = Some(i);
                    let id = self.get_id();
                    ui.close_popup(&id);
                }
                true
            }
            _ => false,
        }
    }
}
