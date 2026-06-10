use std::marker::PhantomData;
use std::time::Duration;
use speedy2d::dimen::Vector2;
use speedy2d::Graphics2D;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance, UserEventSender, VirtualKeyCode, WindowHandler, WindowHelper, WindowStartupInfo};
use crate::drawing::{DrawableRegistry, Palette};
use super::ui::UI;
use super::themes::*;
use super::themes::ImageCache;

pub struct Win<T> {
    ui: UI,
    drawable_registry: DrawableRegistry,
    palette: Palette,
    image_cache: ImageCache,
    width: u32,
    height: u32,
    mouse_pos: Vector2<i32>,
    mod_state: ModifiersState,
    sender: UserEventSender<WinEvent>,
    /// Last cursor shape pushed to the OS, so we only call `set_cursor` on a
    /// real transition (avoids per-move churn).
    last_cursor: Option<MouseCursorType>,
    t: PhantomData<T>
}

impl<T> Win<T> {
    pub fn new(ui: UI, sender: UserEventSender<WinEvent>) -> Self {
        Win {
            ui,
            drawable_registry: DrawableRegistry::new(),
            palette: Palette::classic(),
            image_cache: ImageCache::new(),
            width: 0,
            height: 0,
            mouse_pos: Vector2::new(-1, -1),
            mod_state: ModifiersState::default(),
            sender,
            last_cursor: None,
            t: PhantomData::default()
        }
    }
}

impl<T> WindowHandler<T> for Win<T> {
    fn on_start(&mut self, helper: &mut WindowHelper<T>, info: WindowStartupInfo) {
        println!("on_start");
        self.width = info.viewport_size_pixels().x;
        self.height = info.viewport_size_pixels().y;
        self.ui.layout(self.width, self.height, info.scale_factor());
        helper.request_redraw();

        let user_event_sender = self.sender.clone();

        std::thread::spawn(move || {
            loop {
                // Send a message every 16ms
                user_event_sender.send_event(WinEvent::Update).unwrap();
                std::thread::sleep(Duration::from_millis(15));
            }
        });
        self.ui.start();
    }

    fn on_user_event(&mut self, helper: &mut WindowHelper<T>, _event: T) {
        if self.ui.update() {
            helper.request_redraw();
        }
    }

    fn on_resize(&mut self, helper: &mut WindowHelper<T>, size_pixels: Vector2<u32>) {
        if size_pixels.x == 0 || size_pixels.y == 0 {
            return;
        }
        if self.width == size_pixels.x && self.height == size_pixels.y {
            return;
        }
        self.width = size_pixels.x;
        self.height = size_pixels.y;
        self.ui.layout(size_pixels.x, size_pixels.y, helper.get_scale_factor());
        helper.request_redraw();
    }

    fn on_draw(&mut self, helper: &mut WindowHelper<T>, graphics: &mut Graphics2D) {
        let scale = helper.get_scale_factor();
        let mut theme = Classic::new(graphics, &self.drawable_registry, &self.palette, &mut self.image_cache, self.width as i32, self.height as i32, scale);
        self.ui.paint(&mut theme);
    }

    fn on_mouse_move(&mut self, helper: &mut WindowHelper<T>, position: Vector2<f32>) {
        //println!("Position: {} x {}", position.x, position.y);
        let position = Vector2::new(position.x.round() as i32, position.y.round() as i32);
        self.mouse_pos = position;
        let redraw = self.ui.on_mouse_move(position);
        // Apply the cursor regardless of the redraw flag, only on a transition.
        let cursor = self.ui.current_cursor();
        if self.last_cursor != Some(cursor) {
            helper.set_cursor(cursor);
            self.last_cursor = Some(cursor);
        }
        if redraw {
            helper.request_redraw();
        }
    }

    fn on_mouse_button_down(&mut self, helper: &mut WindowHelper<T>, button: MouseButton) {
        if self.ui.on_mouse_button_down(self.mouse_pos, button) {
            helper.request_redraw();
        }
    }

    fn on_mouse_button_up(&mut self, helper: &mut WindowHelper<T>, button: MouseButton) {
        if self.ui.on_mouse_button_up(self.mouse_pos, button) {
            helper.request_redraw();
        }
    }

    fn on_mouse_wheel_scroll(&mut self, helper: &mut WindowHelper<T>, distance: MouseScrollDistance) {
        if self.ui.on_mouse_wheel_scroll(self.mouse_pos, distance) {
            helper.request_redraw();
        }
    }


    fn on_key_down(&mut self, helper: &mut WindowHelper<T>, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode) {
        println!("KeyCode: {:?}, scancode: {:?} down", virtual_key_code, scancode);
        if self.ui.on_key_down(virtual_key_code, scancode, self.mod_state.clone()) {
            helper.request_redraw();
        }
    }

    fn on_key_up(&mut self, helper: &mut WindowHelper<T>, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode) {
        println!("KeyCode: {:?}, scancode: {:?} up", virtual_key_code, scancode);
        if self.ui.on_key_up(virtual_key_code, scancode, self.mod_state.clone()) {
            helper.request_redraw();
        }
    }

    fn on_keyboard_char(&mut self,helper: &mut WindowHelper<T>, unicode_codepoint: char) {
        println!("Codepoint {:?}", unicode_codepoint);
        if unicode_codepoint == 27 as char {
            if self.ui.has_popups() {
                self.ui.close_all_popups();
                helper.request_redraw();
                return;
            }
            helper.terminate_loop();
        }
        if self.ui.on_key_char(unicode_codepoint, self.mod_state.clone()) {
            helper.request_redraw();
        }
    }

    fn on_keyboard_modifiers_changed(&mut self, _helper: &mut WindowHelper<T>, state: ModifiersState) {
        self.mod_state = state;
    }
}

#[derive(Copy, Clone)]
pub enum WinEvent {
    Update
}