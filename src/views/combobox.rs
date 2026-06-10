use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::font::{FormattedTextBlock, TextLayout, TextOptions};
use speedy2d::window::MouseButton;

use crate::assets::get_font_family;
use crate::common::DEFAULT_TEXT_SIZE;
use crate::events::EventType;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::{PopupDirection, PopupMode, UI};
use crate::view_base::{HasMainFields, ViewBasics};
use crate::views::{Borders, Dimension, FieldsMain, FieldsTexted, Gravity, Visibility};
use crate::views::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH};
use crate::styles::selector::FontSelector;

const ARROW_AREA_WIDTH: i32 = 16;
const ITEM_HEIGHT: i32 = 28;
const ITEM_PADDING_LEFT: i32 = 6;
const ITEM_PADDING_RIGHT: i32 = 6;

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
                listeners: HashMap::new(),
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

    pub fn on_change(&mut self, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        self.state.borrow_mut().listeners.insert(EventType::SelectionChanged, func);
    }

    pub fn get_selected_index(&self) -> Option<usize> {
        *self.selected.borrow()
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let selected = *self.selected.borrow();
        selected.map(|i| self.items.borrow()[i].clone())
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

        let dropdown = ComboDropdown::new(items, typeface, scale, width, Rc::clone(&self.pending_selection));
        let element: Element = Rc::new(RefCell::new(dropdown));

        let pos = self.get_absolute_position();
        let height = self.get_rect_height();

        let id = element.borrow().get_id();
        *self.dropdown_id.borrow_mut() = Some(id);

        ui.show_popup(element, pos.x, pos.y + height, PopupDirection::BottomRight, PopupMode::Popup);
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
        let (width, height) = self.calculate_full_size(scale);
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

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let scale = state.main.scale;
        let mut rect = state.main.rect;
        rect.move_by(origin);

        let arrow_w = (ARROW_AREA_WIDTH as f64 * scale).round() as i32;
        let border = (scale * 2.0).round() as i32;
        let button_rect = crate::types::rect(
            (rect.max.x - arrow_w - border, rect.min.y + border),
            (rect.max.x - border, rect.max.y - border),
        );

        theme.push_clip();
        theme.clip_rect(rect);

        // Step 1: Draw full edit-field area (white background + sunken border)
        theme.draw_component("edit.back", rect, state.main.state);

        // Step 2: Draw selected item text (left-aligned inside edit area)
        if let Some(text) = &state.cached_text {
            let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as f32;
            let x = rect.min.x as f32 + border as f32 + pad_left;
            let y = rect.min.y as f32 + (self.get_rect_height() as f32 - text.height()) / 2.0;
            let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
            theme.draw_text(x.round(), y.round(), color, text);
        }

        // Step 3: Draw sunken border over entire rect
        theme.draw_component("edit.body", rect, state.main.state);

        // Step 4: Draw raised button with arrow inside the sunken area
        theme.draw_component("button.back", button_rect, state.main.state);
        theme.draw_component("combo.arrow", button_rect, state.main.state);
        theme.draw_component("button.body", button_rect, state.main.state);

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

    fn on_event(&mut self, event: EventType, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        self.state.borrow_mut().listeners.insert(event, func);
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.open_dropdown(ui);
        true
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        let pending = self.pending_selection.borrow_mut().take();
        if let Some(index) = pending {
            let items = self.items.borrow();
            if index < items.len() {
                let text = items[index].clone();
                drop(items);
                *self.selected.borrow_mut() = Some(index);
                self.set_display_text(&text);
                *self.dropdown_id.borrow_mut() = None;

                let listener = self.state.borrow_mut().listeners.remove(&EventType::SelectionChanged);
                if let Some(mut handler) = listener {
                    handler(ui, self as &dyn View);
                    self.state.borrow_mut().listeners.insert(EventType::SelectionChanged, handler);
                }
                return true;
            }
        }
        false
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        let old_state = self.state.borrow().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow().main.state != old_state
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
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

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
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
}

impl Default for ComboBox {
    fn default() -> Self {
        let rect = rect((0, 0), (120, 24));
        ComboBox::new(rect, DEFAULT_TEXT_SIZE)
    }
}

// ─── ComboDropdown (private) ─────────────────────────────────────────────────

struct ComboDropdown {
    state: RefCell<FieldsMain>,
    items: Vec<String>,
    cached_texts: RefCell<Vec<Option<FormattedTextBlock>>>,
    hovered: RefCell<Option<usize>>,
    pressed: RefCell<Option<usize>>,
    pending_selection: Rc<RefCell<Option<usize>>>,
    typeface: Option<Typeface>,
    combo_width: i32,
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
    ) -> Self {
        let mut main = FieldsMain::with_rect(rect((0, 0), (combo_width, 100)), Dimension::Min, Dimension::Min);
        main.padding = Borders::with_padding(2);
        main.state.focusable = false;
        main.scale = scale;
        let cached_texts = vec![None; items.len()];
        ComboDropdown {
            state: RefCell::new(main),
            items,
            cached_texts: RefCell::new(cached_texts),
            hovered: RefCell::new(None),
            pressed: RefCell::new(None),
            pending_selection,
            typeface,
            combo_width,
        }
    }

    fn layout_texts(&self, scale: f64) {
        let typeface = match &self.typeface {
            Some(t) => t,
            None => return,
        };
        let base_size = typeface.font_size.unwrap_or(DEFAULT_TEXT_SIZE);
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

    fn get_hit_item(&self, x: i32, y: i32) -> Option<usize> {
        let state = self.state.borrow();
        let r = state.rect;
        if !r.hit((x, y)) {
            return None;
        }
        let scale = state.scale;
        let padding = state.padding.scaled(scale);
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;
        let local_y = y - r.min.y - padding.top;
        if local_y < 0 {
            return None;
        }
        let index = local_y / item_h;
        let count = self.items.len() as i32;
        if index >= 0 && index < count {
            Some(index as usize)
        } else {
            None
        }
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

        let total_w = (padding.left + content_w + padding.right).min(width);
        let total_h = (padding.top + content_h + padding.bottom).min(height);

        let r = rect((x, y), (x + total_w, y + total_h));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let (cw, ch) = self.get_content_size();
        cw <= width && ch <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let mut r = state.rect;
        r.move_by(origin);
        let scale = state.scale;

        theme.push_clip();
        theme.clip_rect(r);

        // Background
        theme.draw_component("edit.back", r, state.state);

        let padding = state.padding.scaled(scale);
        let pad_left = (ITEM_PADDING_LEFT as f64 * scale).round() as i32;
        let item_h = (ITEM_HEIGHT as f64 * scale).round() as i32;

        let hovered = *self.hovered.borrow();
        let cached = self.cached_texts.borrow();

        let content_x = r.min.x + padding.left;
        let mut y = r.min.y + padding.top;

        for (i, _item) in self.items.iter().enumerate() {
            let item_rect = rect(
                (content_x, y),
                (r.max.x - padding.right - 1, y + item_h),
            );

            let text_color = if hovered == Some(i) {
                theme.draw_rect(item_rect, theme.color("item_highlight"));
                theme.color("item_highlight_text")
            } else {
                theme.color("text")
            };

            if let Some(Some(text)) = cached.get(i) {
                let text_x = content_x + pad_left;
                let text_y = y + (item_h as f32 - text.height()) as i32 / 2;
                theme.draw_text(text_x as f32, text_y as f32, text_color, text);
            }

            y += item_h;
        }

        // Border
        theme.draw_component("edit.body", r, state.state);

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

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {}

    fn click(&self, _ui: &mut UI) -> bool { false }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        let hit_item = self.get_hit_item(position.x, position.y);
        let old = *self.hovered.borrow();
        *self.hovered.borrow_mut() = hit_item;
        old != hit_item
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let hit = self.get_hit_item(position.x, position.y);
        *self.pressed.borrow_mut() = hit;
        hit.is_some()
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
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
}
