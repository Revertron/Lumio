use std::cell::RefCell;
use std::rc::{Rc, Weak};
use downcast_rs::Downcast;
use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};
use super::events::EventType;
use super::ui::UI;
use super::themes::{Theme, ViewState};
use super::types::{Rect, Point};
use super::themes::Typeface;
use super::views::{Borders, Dimension, Gravity, Visibility};

pub type Element = Rc<RefCell<dyn View>>;
pub type WeakElement = Weak<RefCell<dyn View>>;

//pub type Parent = Rc<RefCell<dyn Container>>;
//pub type WeakParent = Weak<RefCell<dyn Container>>;

pub trait View: Downcast {
    fn set_any(&mut self, name: &str, value: &str);
    fn set_parent(&self, parent: Option<WeakElement>);
    fn get_parent(&self) -> Option<Element>;
    #[allow(unused)]
    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32>;
    fn layout_in_rect(&mut self, rect: &Rect<i32>, scale: f64) {
        let (width, height) = self.get_content_size();
        let padding = self.get_padding(scale).scaled(scale);
        let mut my_rect = self.get_rect();
        my_rect.min.x = rect.min.x;
        my_rect.min.y = rect.min.y;
        my_rect.max.x = rect.min.x + width + padding.left + padding.right;
        my_rect.max.y = rect.min.y + height + padding.top + padding.bottom;
        self.set_rect(my_rect);
    }
    fn fits_in_rect(&self, width: i32, height: i32, scale: f64) -> bool;
    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme);
    fn get_state(&self) -> Option<ViewState>;
    fn get_rect(&self) -> Rect<i32>;
    fn set_rect(&mut self, rect: Rect<i32>);
    fn get_padding(&self, scale: f64) -> Borders { Borders::default().scaled(scale) }
    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32);
    fn get_margin(&self, scale: f64) -> Borders { Borders::default().scaled(scale) }
    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32);
    fn get_gravity(&self) -> Gravity { Gravity::default() }
    #[allow(unused_variables)]
    fn set_gravity(&self, gravity: Gravity) {}
    fn get_x(&self) -> i32 { self.get_rect().min.x }
    fn get_y(&self) -> i32 { self.get_rect().min.y }
    fn get_rect_width(&self) -> i32 { self.get_rect().width() }
    fn get_rect_height(&self) -> i32 { self.get_rect().height() }
    fn get_bounds(&self) -> (Dimension, Dimension);
    /// Returns unscaled content sizes
    fn get_content_size(&self) -> (i32, i32);
    fn is_focused(&self) -> bool { false }
    fn is_break(&self) -> bool { false }
    #[allow(unused_variables)]
    fn set_focused(&self, focused: bool) {}
    fn set_focusable(&self, focusable: bool);
    fn calculate_full_size(&self, scale: f64) -> (i32, i32) {
        let (width, height) = self.get_content_size();
        let padding = self.get_padding(scale);
        let width = padding.left + width + padding.right;
        let height = padding.top + height + padding.bottom;
        (width, height)
    }
    fn calculate_size(&mut self, width: i32, height: i32, scale: f64) -> (i32, i32) {
        let (b_width, b_height) = self.get_bounds();
        let margins = self.get_margin(scale);
        let width = match b_width {
            Dimension::Min => width, // TODO change this after all children layout themselves
            Dimension::Max => width - margins.left - margins.right,
            Dimension::Dip(dip) => (dip as f64 * scale).round() as i32,
            Dimension::Percent(p) => (width as f32 * p / 100f32).round() as i32
        };
        let height = match b_height {
            Dimension::Min => height, // TODO change this after all children layout themselves
            Dimension::Max => height - margins.top - margins.bottom,
            Dimension::Dip(dip) => (dip as f64 * scale).round() as i32,
            Dimension::Percent(p) => (height as f32 * p / 100f32).round() as i32
        };
        (width, height)
    }
    fn set_x(&mut self, x: i32) {
        let mut rect = self.get_rect();
        rect.move_to((x, rect.min.y));
        self.set_rect(rect);
    }
    fn set_y(&mut self, y: i32) {
        let mut rect = self.get_rect();
        rect.move_to((rect.min.x, y));
        self.set_rect(rect);
    }
    fn set_width(&mut self, width: Dimension);
    fn set_height(&mut self, height: Dimension);
    fn set_scale(&mut self, scale: f64);
    fn set_id(&mut self, id: &str);
    fn get_id(&self) -> String;
    /// Returns the absolute (window) position of this view's top-left corner
    /// by walking up the parent chain and accumulating offsets.
    fn get_absolute_position(&self) -> Point<i32> {
        let rect = self.get_rect();
        let mut x = rect.min.x;
        let mut y = rect.min.y;
        let mut current = self.get_parent();
        while let Some(parent) = current {
            let parent_ref = parent.borrow();
            let pr = parent_ref.get_rect();
            x += pr.min.x;
            y += pr.min.y;
            current = parent_ref.get_parent();
        }
        Point { x, y }
    }

    fn is_enabled(&self) -> bool { true }
    #[allow(unused_variables)]
    fn set_enabled(&mut self, enabled: bool) {}
    fn get_visibility(&self) -> Visibility { Visibility::Visible }
    #[allow(unused_variables)]
    fn set_visibility(&mut self, visibility: Visibility) {}

    fn get_tooltip(&self) -> Option<String> { None }
    #[allow(unused_variables)]
    fn set_tooltip(&mut self, tooltip: Option<String>) {}

    fn get_background(&self) -> Option<u32> { None }
    #[allow(unused_variables)]
    fn set_background(&mut self, color: Option<u32>) {}
    fn get_border_color(&self) -> Option<u32> { None }
    #[allow(unused_variables)]
    fn set_border_color(&mut self, color: Option<u32>) {}

    fn as_container(&self) -> Option<&dyn Container> { None }
    fn as_container_mut(&mut self) -> Option<&mut dyn Container> { None }

    /// When `true`, the XML parser does not treat this view's child tags as
    /// nested views. Instead it captures the literal inner markup of the
    /// element (e.g. `Hello <b>world</b>`) and hands it to the view via
    /// `set_any("html", ...)`. Used by `RichText` so inline tags become spans
    /// rather than being instantiated as views. Default `false`.
    fn wants_raw_content(&self) -> bool { false }

    // Events and listeners
    fn on_event(&mut self, event: EventType, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>);
    fn click(&self, ui: &mut UI) -> bool;
    #[allow(unused_variables)]
    fn update(&mut self, ui: &mut UI) -> bool { false }

    #[allow(unused_variables)]
    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool { false }
    #[allow(unused_variables)]
    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool { false }
    #[allow(unused_variables)]
    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool { false }
    #[allow(unused_variables)]
    fn on_mouse_wheel_scroll(&self, ui: &mut UI, position: Vector2<i32>, distance: MouseScrollDistance) -> bool { false }
    #[allow(unused_variables)]
    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool { false }
    #[allow(unused_variables)]
    fn on_key_up(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, state: ModifiersState) -> bool { false }
    #[allow(unused_variables)]
    fn on_key_char(&self, ui: &mut UI, unicode_codepoint: char, state: ModifiersState) -> bool { false }
    #[allow(unused_variables)]
    fn on_key_mod_changed(&self, ui: &mut UI, state: ModifiersState) -> bool { false }
}

impl_downcast!(View);

pub trait Container: View {
    fn add_view(&mut self, view: Element);
    fn get_view(&self, id: &str) -> Option<Element>;
    fn get_view_count(&self) -> usize;
    fn get_views(&self) -> Vec<Element> { Vec::new() }

    /// Remove the view with the given id from this container's subtree.
    /// Returns true if a view was removed. Default impl does nothing —
    /// containers that own children should override.
    #[allow(unused_variables)]
    fn remove_view(&mut self, id: &str) -> bool { false }
}
