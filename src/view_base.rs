use std::cell::RefCell;
use crate::events::{EventCallback, EventData, EventType};
use crate::styles::selector::{DrawState, MainSelector};
use crate::themes::{FontStyle, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::Rect;
use crate::ui::UI;
use crate::views::{Borders, Dimension, Dock, FieldsMain, Gravity, LayoutParams, Visibility};

/// Manages font/typeface inheritance and manipulation
#[derive(Clone, Default)]
pub struct FontManager {
    typeface: Option<Typeface>
}

impl FontManager {
    pub fn new() -> Self {
        Self { typeface: None }
    }

    /// Get the effective typeface, inheriting from parent if not fully specified.
    /// Each field inherits independently when this view did not set it.
    pub fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        match &self.typeface {
            None => parent_typeface.clone(),
            Some(t) => {
                let font_name = if t.font_name.is_empty() {
                    parent_typeface.font_name.clone()
                } else {
                    t.font_name.clone()
                };
                let font_size = t.font_size.or(parent_typeface.font_size);
                Typeface {
                    font_name,
                    font_style: t.font_style,
                    font_size
                }
            }
        }
    }

    pub fn set_font(&mut self, font_name: &str) {
        let typeface = match self.typeface.take() {
            None => Typeface {
                font_name: font_name.to_owned(),
                font_style: FontStyle::Regular,
                font_size: None
            },
            Some(mut t) => {
                t.font_name = font_name.to_owned();
                t
            }
        };
        self.typeface = Some(typeface);
    }

    pub fn set_font_style(&mut self, style: &str) {
        let font_style = FontStyle::from(style);
        let typeface = match self.typeface.take() {
            None => Typeface {
                font_name: String::new(),
                font_style,
                font_size: None
            },
            Some(t) => Typeface {
                font_name: t.font_name,
                font_style,
                font_size: t.font_size
            },
        };
        self.typeface = Some(typeface);
    }

    pub fn set_font_size(&mut self, size: f32) {
        let typeface = match self.typeface.take() {
            None => Typeface {
                font_name: String::new(),
                font_style: FontStyle::Regular,
                font_size: Some(size)
            },
            Some(mut t) => {
                t.font_size = Some(size);
                t
            }
        };
        self.typeface = Some(typeface);
    }

    pub fn get(&self) -> Option<Typeface> {
        self.typeface.clone()
    }

    pub fn set(&mut self, typeface: Option<Typeface>) {
        self.typeface = typeface;
    }
}

/// Trait for views that have main fields
pub trait HasMainFields {
    fn main_fields(&self) -> &RefCell<FieldsMain>;
}

/// Provides default implementations for common View methods
pub trait ViewBasics: HasMainFields {
    fn base_set_parent(&self, parent: Option<WeakElement>) {
        self.main_fields().borrow_mut().parent = parent;
    }

    fn base_get_parent(&self) -> Option<Element> {
        match &self.main_fields().borrow().parent {
            None => None,
            Some(weak) => weak.upgrade()
        }
    }

    fn base_get_rect(&self) -> Rect<i32> {
        self.main_fields().borrow().rect
    }

    fn base_set_rect(&self, rect: Rect<i32>) {
        self.main_fields().borrow_mut().rect = rect;
    }

    fn base_get_padding(&self, scale: f64) -> Borders {
        let fields = self.main_fields().borrow();
        // A 9-patch background carries its own content padding (right/bottom
        // markers); it applies unless padding was set explicitly.
        if !fields.padding_explicit
            && let Some(np) = fields.background_ninepatch.borrow_mut().as_mut()
            && let Some(pad) = np.content_padding()
        {
            return pad.scaled(scale);
        }
        fields.padding.scaled(scale)
    }

    fn base_set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut fields = self.main_fields().borrow_mut();
        fields.padding.top = top;
        fields.padding.left = left;
        fields.padding.right = right;
        fields.padding.bottom = bottom;
        fields.padding_explicit = true;
    }

    fn base_get_margin(&self, scale: f64) -> Borders {
        self.main_fields().borrow().margin.scaled(scale)
    }

    fn base_set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut fields = self.main_fields().borrow_mut();
        fields.margin.top = top;
        fields.margin.left = left;
        fields.margin.right = right;
        fields.margin.bottom = bottom;
    }

    fn base_get_bounds(&self) -> (Dimension, Dimension) {
        let fields = self.main_fields().borrow();
        (fields.width, fields.height)
    }

    fn base_set_width(&self, width: Dimension) {
        self.main_fields().borrow_mut().width = width;
    }

    fn base_set_height(&self, height: Dimension) {
        self.main_fields().borrow_mut().height = height;
    }

    fn base_set_scale(&self, scale: f64) {
        self.main_fields().borrow_mut().scale = scale;
    }

    fn base_set_id(&self, id: &str) {
        self.main_fields().borrow_mut().id = id.to_owned();
    }

    fn base_get_id(&self) -> String {
        self.main_fields().borrow().id.clone()
    }

    fn base_is_break(&self) -> bool {
        self.main_fields().borrow().break_line
    }

    fn base_set_focusable(&self, focusable: bool) {
        self.main_fields().borrow_mut().state.focusable = focusable;
    }

    fn base_is_focused(&self) -> bool {
        self.main_fields().borrow().state.focused
    }

    fn base_set_focused(&self, focused: bool) {
        self.main_fields().borrow_mut().state.focused = focused;
    }

    fn base_is_enabled(&self) -> bool {
        self.main_fields().borrow().state.enabled
    }

    fn base_set_enabled(&self, enabled: bool) {
        self.main_fields().borrow_mut().state.enabled = enabled;
    }

    fn base_get_visibility(&self) -> Visibility {
        self.main_fields().borrow().visibility
    }

    fn base_set_visibility(&self, visibility: Visibility) {
        self.main_fields().borrow_mut().visibility = visibility;
    }

    fn base_set_x(&self, x: i32) {
        let mut fields = self.main_fields().borrow_mut();
        fields.rect.min.x = x;
        fields.rect.max.x = x + fields.rect.width();
    }

    fn base_set_y(&self, y: i32) {
        let mut fields = self.main_fields().borrow_mut();
        fields.rect.min.y = y;
        fields.rect.max.y = y + fields.rect.height();
    }

    fn base_get_tooltip(&self) -> Option<String> {
        self.main_fields().borrow().tooltip.clone()
    }

    fn base_set_tooltip(&self, tooltip: Option<String>) {
        self.main_fields().borrow_mut().tooltip = tooltip;
    }

    fn base_get_content_description(&self) -> Option<String> {
        self.main_fields().borrow().content_description.clone()
    }

    fn base_set_content_description(&self, description: Option<String>) {
        self.main_fields().borrow_mut().content_description = description;
    }

    fn base_get_labelled_by(&self) -> Option<String> {
        self.main_fields().borrow().labelled_by.clone()
    }

    fn base_set_labelled_by(&self, view_id: Option<String>) {
        self.main_fields().borrow_mut().labelled_by = view_id;
    }

    /// Draws the 9-patch background into `rect` (screen-space) if one is set.
    /// Returns `true` when drawn so the caller skips its default back/body
    /// drawing. Takes only an immutable borrow of `FieldsMain` (the composite
    /// cache lives in a nested `RefCell`) — safe to call while the view's
    /// `paint` holds its own state borrow.
    fn base_draw_ninepatch(&self, theme: &mut dyn crate::themes::Theme, rect: Rect<i32>) -> bool {
        let fields = self.main_fields().borrow();
        let mut np = fields.background_ninepatch.borrow_mut();
        match np.as_mut() {
            Some(np) => {
                np.paint(theme, rect, &fields.state, fields.scale);
                true
            }
            None => false,
        }
    }

    fn base_get_background(&self) -> Option<u32> {
        let fields = self.main_fields().borrow();
        if let Some(ref selector) = fields.background {
            if let Some(DrawState::Color(c)) = selector.get_state(&ViewState::no_focus()) {
                return Some(*c);
            }
        }
        None
    }

    fn base_set_background(&self, color: Option<u32>) {
        let mut fields = self.main_fields().borrow_mut();
        match color {
            Some(c) => {
                let mut selector = MainSelector::new();
                selector.add_state(ViewState::no_focus(), DrawState::Color(c));
                fields.background = Some(selector);
            }
            None => fields.background = None,
        }
    }

    fn base_get_border_color(&self) -> Option<u32> {
        self.main_fields().borrow().border_color
    }

    fn base_set_border_color(&self, color: Option<u32>) {
        self.main_fields().borrow_mut().border_color = color;
    }

    fn base_get_gravity(&self) -> Gravity {
        self.main_fields().borrow().gravity
    }

    fn base_set_gravity(&self, gravity: Gravity) {
        self.main_fields().borrow_mut().gravity = gravity;
    }

    fn base_on_event(&self, event: EventType, func: EventCallback) {
        self.main_fields().borrow_mut().listeners.insert(event, func);
    }

    fn base_has_listener(&self, event: EventType) -> bool {
        self.main_fields().borrow().listeners.contains_key(&event)
    }

    /// Fires the listener registered for `event`, if any. The handler runs
    /// with the listener taken out of the map (so an event cannot recursively
    /// fire itself) and is put back afterwards unless the handler registered
    /// a replacement. The handler must NOT `borrow_mut` this view (the caller
    /// may hold an immutable borrow of the element); it mutates the view via
    /// the `&dyn View` argument and `&self` setters.
    fn base_fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool
        where Self: View + Sized
    {
        let handler = self.main_fields().borrow_mut().listeners.remove(&event);
        if let Some(mut handler) = handler {
            let result = handler(ui, self as &dyn View, data);
            self.main_fields().borrow_mut().listeners.entry(event).or_insert(handler);
            result
        } else {
            false
        }
    }

    fn base_get_layout_params(&self) -> LayoutParams {
        self.main_fields().borrow().layout_params
    }

    fn base_set_layout_params(&self, params: LayoutParams) {
        self.main_fields().borrow_mut().layout_params = params;
    }

    /// Handle common properties in set_any. Returns true if handled, false if not.
    fn base_set_any(&self, name: &str, value: &str) -> bool {
        let fields = self.main_fields();
        match name {
            "id" => {
                fields.borrow_mut().id = value.to_owned();
                true
            }
            "left" => {
                if let Ok(x) = value.parse() {
                    self.base_set_x(x);
                }
                true
            }
            "top" => {
                if let Ok(y) = value.parse() {
                    self.base_set_y(y);
                }
                true
            }
            "width" => {
                if let Ok(w) = value.parse() {
                    self.base_set_width(w);
                }
                true
            }
            "height" => {
                if let Ok(h) = value.parse() {
                    self.base_set_height(h);
                }
                true
            }
            "padding" => {
                let mut f = fields.borrow_mut();
                f.padding.set_all(value.parse().unwrap_or(0));
                f.padding_explicit = true;
                true
            }
            "padding_top" => {
                let mut f = fields.borrow_mut();
                f.padding.top = value.parse().unwrap_or(0);
                f.padding_explicit = true;
                true
            }
            "padding_left" => {
                let mut f = fields.borrow_mut();
                f.padding.left = value.parse().unwrap_or(0);
                f.padding_explicit = true;
                true
            }
            "padding_right" => {
                let mut f = fields.borrow_mut();
                f.padding.right = value.parse().unwrap_or(0);
                f.padding_explicit = true;
                true
            }
            "padding_bottom" => {
                let mut f = fields.borrow_mut();
                f.padding.bottom = value.parse().unwrap_or(0);
                f.padding_explicit = true;
                true
            }
            "margin" => {
                fields.borrow_mut().margin.set_all(value.parse().unwrap_or(0));
                true
            }
            "margin_left" => {
                fields.borrow_mut().margin.left = value.parse().unwrap_or(0);
                true
            }
            "margin_right" => {
                fields.borrow_mut().margin.right = value.parse().unwrap_or(0);
                true
            }
            "margin_top" => {
                fields.borrow_mut().margin.top = value.parse().unwrap_or(0);
                true
            }
            "margin_bottom" => {
                fields.borrow_mut().margin.bottom = value.parse().unwrap_or(0);
                true
            }
            "break" => {
                fields.borrow_mut().break_line = value.parse().unwrap_or(false);
                true
            }
            "enabled" => {
                fields.borrow_mut().state.enabled = value.parse().unwrap_or(true);
                true
            }
            "visibility" => {
                fields.borrow_mut().visibility = value.parse().unwrap_or(Visibility::Visible);
                true
            }
            "tooltip" => {
                fields.borrow_mut().tooltip = Some(value.to_owned());
                true
            }
            "content_description" => {
                fields.borrow_mut().content_description = Some(value.to_owned());
                true
            }
            "labelled_by" => {
                fields.borrow_mut().labelled_by = Some(value.to_owned());
                true
            }
            "background" => {
                // Three value forms share this attribute: a `.9.png` path
                // (single 9-patch), an `.xml` path (per-state 9-patch
                // selector), or the classic `@token`/`#hex` fill. Last write
                // wins across forms.
                let lower = value.to_ascii_lowercase();
                if lower.ends_with(".9.png") {
                    let mut f = fields.borrow_mut();
                    *f.background_ninepatch.borrow_mut() =
                        Some(crate::ninepatch::NinePatchBackground::from_png(value));
                    f.background = None;
                } else if lower.ends_with(".xml") {
                    match crate::ninepatch::NinePatchBackground::from_selector(value) {
                        Ok(np) => {
                            let mut f = fields.borrow_mut();
                            *f.background_ninepatch.borrow_mut() = Some(np);
                            f.background = None;
                        }
                        Err(e) => log::warn!("background selector '{}': {}", value, e),
                    }
                } else if let Some(draw_state) = parse_draw_state(value) {
                    let mut selector = MainSelector::new();
                    selector.add_state(ViewState::no_focus(), draw_state);
                    let mut f = fields.borrow_mut();
                    f.background = Some(selector);
                    *f.background_ninepatch.borrow_mut() = None;
                }
                true
            }
            "text_color" => {
                if let Some(draw_state) = parse_draw_state(value) {
                    fields.borrow_mut().foreground = Some(uniform_color_selector(draw_state));
                }
                true
            }
            "border_color" => {
                if let Some(color) = parse_color_value(value) {
                    fields.borrow_mut().border_color = Some(color);
                }
                true
            }
            "gravity" => {
                if let Ok(g) = value.parse::<Gravity>() {
                    fields.borrow_mut().gravity = g;
                }
                true
            }
            "dock" => {
                if let Ok(d) = value.parse::<Dock>() {
                    fields.borrow_mut().layout_params.dock = d;
                }
                true
            }
            "weight" => {
                if let Ok(w) = value.parse::<f32>() && w > 0.0 {
                    fields.borrow_mut().layout_params.weight = w;
                }
                true
            }
            _ => false
        }
    }
}

/// Build a `MainSelector` that returns the given draw state for every possible
/// `ViewState`. Used by XML attributes like `text_color` where the user wants
/// a single color regardless of focus/hover/press state.
fn uniform_color_selector(draw_state: DrawState) -> MainSelector {
    let mut selector = MainSelector::new();
    for bits in 0u8..64 {
        selector.add_state(ViewState {
            enabled: bits & 1 != 0,
            focusable: bits & 2 != 0,
            focused: bits & 4 != 0,
            hovered: bits & 8 != 0,
            pressed: bits & 16 != 0,
            checked: bits & 32 != 0,
        }, draw_state.clone());
    }
    selector
}

/// Parse a hex color string like `#RRGGBB` or `#AARRGGBB` into a u32.
pub(crate) fn parse_hex_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok().map(|c| 0xFF000000 | c),
        8 => u32::from_str_radix(hex, 16).ok(),
        _ => None,
    }
}

/// Parse a color attribute into a `DrawState`: `@token` becomes a palette
/// reference resolved at paint time (follows theme switches), `#hex` a literal.
fn parse_draw_state(s: &str) -> Option<DrawState> {
    match s.strip_prefix('@') {
        Some(token) if !token.is_empty() => Some(DrawState::Token(token.to_string())),
        Some(_) => None,
        None => parse_hex_color(s).map(DrawState::Color),
    }
}

/// Parse a color attribute into a concrete u32: `@token` resolves against the
/// palette active right now (a later theme switch will NOT update it — used
/// for plain-u32 attributes like `border_color` or `icon_tint`), `#hex` is
/// parsed literally.
pub(crate) fn parse_color_value(s: &str) -> Option<u32> {
    match s.strip_prefix('@') {
        Some(token) if !token.is_empty() => Some(crate::drawing::current_color(token)),
        Some(_) => None,
        None => parse_hex_color(s),
    }
}
