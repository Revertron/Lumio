use std::marker::PhantomData;
use std::time::Duration;
use speedy2d::dimen::Vector2;
use speedy2d::Graphics2D;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, UserEventSender, VirtualKeyCode, WindowHandler, WindowHelper, WindowStartupInfo};
use crate::drawing::DrawableRegistry;
use super::ui::UI;
use super::themes::*;
use super::themes::ImageCache;

pub struct Win<T> {
    ui: UI,
    drawable_registry: DrawableRegistry,
    image_cache: ImageCache,
    width: u32,
    height: u32,
    mouse_pos: Vector2<i32>,
    mod_state: ModifiersState,
    sender: UserEventSender<WinEvent>,
    t: PhantomData<T>
}

impl<T> Win<T> {
    pub fn new(ui: UI, sender: UserEventSender<WinEvent>) -> Self {
        Win {
            ui,
            drawable_registry: DrawableRegistry::new(),
            image_cache: ImageCache::new(),
            width: 0,
            height: 0,
            mouse_pos: Vector2::new(-1, -1),
            mod_state: ModifiersState::default(),
            sender,
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
        let mut theme = Classic::new(graphics, &self.drawable_registry, &mut self.image_cache, self.width as i32, self.height as i32, scale);
        self.ui.paint(&mut theme);
    }

    fn on_mouse_move(&mut self, helper: &mut WindowHelper<T>, position: Vector2<f32>) {
        //println!("Position: {} x {}", position.x, position.y);
        let position = Vector2::new(position.x.round() as i32, position.y.round() as i32);
        self.mouse_pos = position;
        if self.ui.on_mouse_move(position) {
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