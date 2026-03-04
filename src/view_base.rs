use std::cell::RefCell;
use crate::themes::{FontStyle, Typeface};
use crate::traits::{Element, WeakElement};
use crate::types::Rect;
use crate::views::{Borders, Dimension, FieldsMain};

/// Manages font/typeface inheritance and manipulation
#[derive(Clone, Default)]
pub struct FontManager {
    typeface: Option<Typeface>
}

impl FontManager {
    pub fn new() -> Self {
        Self { typeface: None }
    }

    /// Get the effective typeface, inheriting from parent if not fully specified
    pub fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        match &self.typeface {
            None => parent_typeface.clone(),
            Some(t) => {
                if t.font_name.is_empty() {
                    let mut parent = parent_typeface.clone();
                    parent.font_style = t.font_style.clone();
                    parent
                } else {
                    t.clone()
                }
            }
        }
    }

    pub fn set_font(&mut self, font_name: &str) {
        let typeface = match self.typeface.take() {
            None => Typeface {
                font_name: font_name.to_owned(),
                font_style: FontStyle::Regular
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
                font_style
            },
            Some(t) => Typeface {
                font_name: t.font_name,
                font_style
            },
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
        self.main_fields().borrow().padding.scaled(scale)
    }

    fn base_set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        let mut fields = self.main_fields().borrow_mut();
        fields.padding.top = top;
        fields.padding.left = left;
        fields.padding.right = right;
        fields.padding.bottom = bottom;
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
                fields.borrow_mut().padding.set_all(value.parse().unwrap_or(0));
                true
            }
            "padding_top" => {
                fields.borrow_mut().padding.top = value.parse().unwrap_or(0);
                true
            }
            "padding_left" => {
                fields.borrow_mut().padding.left = value.parse().unwrap_or(0);
                true
            }
            "padding_right" => {
                fields.borrow_mut().padding.right = value.parse().unwrap_or(0);
                true
            }
            "padding_bottom" => {
                fields.borrow_mut().padding.bottom = value.parse().unwrap_or(0);
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
            _ => false
        }
    }
}
