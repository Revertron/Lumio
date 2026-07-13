use std::cell::RefCell;
use std::cmp::max;

use crate::text::{TextAlignment, TextOptions};
use crate::input::{KeyScancode, ModifiersState, MouseButton, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::themes::{Theme, Typeface, ViewState};
use crate::view_base::{HasMainFields, ViewBasics};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Gravity, Visibility};
use crate::styles::selector::FontSelector;
use crate::views::{FieldsMain, FieldsTexted};
use crate::views::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH};

pub struct CheckBox {
    state: RefCell<FieldsTexted>,
    text_margin: i32
}

impl HasMainFields for CheckBox {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for CheckBox {}

const DEFAULT_TEXT_MARGIN: i32 = 6;


#[allow(dead_code)]
impl CheckBox {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> CheckBox {
        let main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        CheckBox {
            state: RefCell::new(FieldsTexted {
                main,
                text: text.to_owned(),
                text_size,
                line_height: 0f32,
                single_line: true,
                cached_text: None,
                font: FontSelector::new()
            }),
            text_margin: DEFAULT_TEXT_MARGIN
        }
    }

    pub fn set_text(&self, text: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.text.clear();
            state.text.push_str(text);
            state.cached_text = None;
        }
        let scale = self.state.borrow().main.scale;
        let single_line = self.state.borrow().single_line;
        self.layout_text(self.get_rect_width(), single_line, scale);
    }

    pub fn get_text(&self) -> String {
        self.state.borrow().text.clone()
    }

    pub fn is_checked(&self) -> bool {
        self.state.borrow().main.state.checked
    }

    pub fn set_checked(&self, checked: bool) {
        self.state.borrow_mut().main.state.checked = checked;
    }

    pub fn set_single_line(&self, single_line: bool) {
        let mut state = self.state.borrow_mut();
        state.single_line = single_line;
        state.cached_text = None;
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

    fn layout_text(&self, max_width: i32, single_line: bool, scale: f64) {
        if max_width <= 0 {
            self.state.borrow_mut().cached_text = None;
            return;
        }
        let typeface = self.state.borrow().main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let scale = scale.round() as i32;
                let box_size = (crate::drawing::current_dimension("checkbox.box_size") as i32) * scale;
                let text_margin = self.text_margin * scale;
                let width = max_width - box_size - text_margin;
                let options = match single_line {
                    true => TextOptions::new(),
                    false => TextOptions::new().with_wrap_to_width(width as f32, TextAlignment::Left)
                };
                let base_size = typeface.font_size.unwrap_or(self.state.borrow().text_size);
                let size = base_size * scale as f32;
                let text = font.layout_text(&self.state.borrow().text, size, options);
                self.state.borrow_mut().cached_text = Some(text);
            }
        }
    }
}

impl View for CheckBox {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "text" => { self.set_text(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
            }
            "single_line" => { self.state.borrow_mut().single_line = value.parse().unwrap_or(true) }
            "checked" => { self.set_checked(value.parse().unwrap_or(false)) }
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
        //println!("{} for width {}", self.get_id(), width);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface));
        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let max_width = width.max(crate::drawing::current_dimension("checkbox.box_size") as i32) - horizontal;
        let max_height = height.max(crate::drawing::current_dimension("checkbox.box_size") as i32) - vertical;
        let (new_width, _new_height) = self.calculate_size(max_width, max_height, scale);
        let single_line = self.state.borrow().single_line;
        self.layout_text(new_width, single_line, scale);
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

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let state = self.state.borrow();
        let box_size = (crate::drawing::current_dimension("checkbox.box_size") as i32) * state.main.scale.round() as i32;
        let mut rect = state.main.rect;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        let box_y = (self.get_rect_height() - box_size) / 2;
        let box_rect = super::super::types::rect((rect.min.x, rect.min.y + box_y), (rect.min.x + box_size, rect.min.y + box_y + box_size));

        // A 9-patch background paints behind the whole view; the check box
        // itself stays drawable-based.
        self.base_draw_ninepatch(theme, rect);

        // Step 1: Draw checkbox background (before text)
        theme.draw_component("edit.back", box_rect, state.main.state);

        // Step 2: Draw text label
        // TODO use padding
        if let Some(text) = &state.cached_text {
            let x = (rect.min.x as f32 + box_size as f32 + self.text_margin as f32 * state.main.scale as f32) as f32;
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
            theme.draw_text(x.round(), (rect.min.y as f32 + y).round(), color, text);
        }

        // Step 3: Draw checkbox borders (after text)
        theme.draw_component("edit.body", box_rect, state.main.state);

        // Step 4: Draw checkmark if checked (on top of borders)
        if state.main.state.checked {
            theme.draw_component("checkbox.checkmark", box_rect, state.main.state);
        }

        // Keyboard-focus indicator: thin outline around the label (or the box
        // when there is no label) — the box drawable has no focused selector.
        // Clamped to the view rect: the paint is clipped to it, and the
        // checkbox rect hugs its content.
        if state.main.state.focused && state.main.state.enabled {
            let pad = (2.0 * state.main.scale).round() as i32;
            let target = match &state.cached_text {
                Some(text) => {
                    let x = rect.min.x + box_size + (self.text_margin as f64 * state.main.scale).round() as i32;
                    let y = rect.min.y + ((self.get_rect_height() as f32 - text.height()) / 2f32) as i32;
                    super::super::types::rect(
                        (x - pad, y - pad),
                        (x + text.width().ceil() as i32 + pad, y + text.height().ceil() as i32 + pad),
                    )
                }
                None => super::super::types::rect(
                    (box_rect.min.x - pad, box_rect.min.y - pad),
                    (box_rect.max.x + pad, box_rect.max.y + pad),
                ),
            };
            let focus_rect = super::super::types::rect(
                (target.min.x.max(rect.min.x), target.min.y.max(rect.min.y)),
                (target.max.x.min(rect.max.x), target.max.y.min(rect.max.y)),
            );
            if focus_rect.width() > 0 && focus_rect.height() > 0 {
                let width = (state.main.scale.round() as i32).max(1);
                theme.draw_rect_outline(focus_rect, theme.color("focus"), width);
            }
        }

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
        let box_size = (crate::drawing::current_dimension("checkbox.box_size") as i32) * scale;
        let text_margin = self.text_margin * scale;
        match &state.cached_text {
            None => (box_size, box_size),
            Some(text) => {
                let width = text.width().ceil() as i32 + box_size + text_margin;
                let height = max(text.height().ceil() as i32, box_size / 2);
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

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        let checked = self.state.borrow().main.state.checked;
        self.state.borrow_mut().main.state.checked = !checked;
        let mut result = false;
        result |= self.base_fire_event(ui, EventType::CheckedChanged, &EventData::Checked(!checked));
        result |= self.base_fire_event(ui, EventType::Click, &EventData::None);
        result
    }

    fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::CheckBox);
        node.set_label(self.get_text());
        node.set_toggled(if self.is_checked() { accesskit::Toggled::True } else { accesskit::Toggled::False });
        node.add_action(accesskit::Action::Click);
        node
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Point<i32>) -> bool {
        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        let old_state = self.state.borrow_mut().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow_mut().main.state != old_state
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
                let mut state = self.state.borrow_mut();
                state.main.state.pressed = false;
                return true;
            }
        }
        false
    }

    // Space toggles the focused checkbox: press on key down, click on key up.
    fn on_key_down(&self, _ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        if matches!(virtual_key_code, Some(VirtualKeyCode::Space)) {
            self.state.borrow_mut().main.state.pressed = true;
            return true;
        }
        false
    }

    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        if matches!(virtual_key_code, Some(VirtualKeyCode::Space))
            && self.state.borrow().main.state.pressed {
            self.state.borrow_mut().main.state.pressed = false;
            self.click(ui);
            return true;
        }
        false
    }
}

impl Default for CheckBox {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        CheckBox::new(rect, "", crate::drawing::current_text_size("text"))
    }
}
