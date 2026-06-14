use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use speedy2d::dimen::Vector2;
use speedy2d::Graphics2D;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance, UserEventSender, VirtualKeyCode, WindowCreationOptions, WindowHandler, WindowHelper, WindowPosition, WindowSize, WindowStartupInfo};
use crate::drawing::{DrawableRegistry, Palette};
use super::ui::{UI, WindowCommand};
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
    /// Last cursor shape pushed to the OS, so we only call `set_cursor` on a
    /// real transition (avoids per-move churn).
    last_cursor: Option<MouseCursorType>,
    /// Cleared on drop so this window's update ticker thread stops.
    alive: Arc<AtomicBool>,
    /// Child windows close on Esc instead of terminating the app.
    is_child: bool,
    /// When set on the main window, the close gesture (Esc) hides the window
    /// instead of terminating the loop — pairs with speedy2d's
    /// `with_hide_on_close` (which handles the X button) for tray apps.
    close_hides: bool,
    t: PhantomData<T>
}

impl<T> Win<T> {
    /// The sender parameter is unused since the update ticker switched to a
    /// per-window sender created in `on_start`; kept for API compatibility.
    pub fn new(ui: UI, _sender: UserEventSender<WinEvent>) -> Self {
        Self::build(ui, false)
    }

    /// A handler for a child window (opened via [`UI::open_window`]).
    pub fn new_child(ui: UI) -> Self {
        Self::build(ui, true)
    }

    fn build(ui: UI, is_child: bool) -> Self {
        Win {
            ui,
            drawable_registry: DrawableRegistry::new(),
            palette: Palette::classic(),
            image_cache: ImageCache::new(),
            width: 0,
            height: 0,
            mouse_pos: Vector2::new(-1, -1),
            mod_state: ModifiersState::default(),
            last_cursor: None,
            alive: Arc::new(AtomicBool::new(true)),
            is_child,
            close_hides: false,
            t: PhantomData
        }
    }

    /// When `true`, the close gesture (Esc) on the main window hides it instead
    /// of terminating the app. Pair with speedy2d's
    /// [`WindowCreationOptions::with_hide_on_close`] (for the X button) so an
    /// app can live in the system tray. No effect on child windows.
    pub fn set_close_hides(&mut self, value: bool) {
        self.close_hides = value;
    }

    /// Choose the palette the window starts with (default: `Palette::classic()`).
    pub fn set_palette(&mut self, palette: Palette) {
        crate::drawing::set_current_palette(palette.clone());
        self.palette = palette;
    }

    /// Pushes the cursor shape the UI currently wants to the OS, but only on a
    /// real transition (avoids per-event churn). Called after moves and after
    /// button events, since a popup opened/closed by a click changes the
    /// cursor without generating a mouse move.
    fn apply_cursor(&mut self, helper: &mut WindowHelper<T>) {
        let cursor = self.ui.current_cursor();
        if self.last_cursor != Some(cursor) {
            helper.set_cursor(cursor);
            self.last_cursor = Some(cursor);
        }
    }
}

impl<T> Drop for Win<T> {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl<T: From<WinEvent> + Send + 'static> WindowHandler<T> for Win<T> {
    fn on_start(&mut self, helper: &mut WindowHelper<T>, info: WindowStartupInfo) {
        println!("on_start");
        self.width = info.viewport_size_pixels().x;
        self.height = info.viewport_size_pixels().y;
        self.ui.layout(self.width, self.height, info.scale_factor());
        helper.request_redraw();

        // Per-window sender: its events come back to this window's handler.
        let user_event_sender = helper.create_user_event_sender();
        let alive = Arc::clone(&self.alive);

        std::thread::spawn(move || {
            // Send an update tick every 16ms until the window is gone.
            while alive.load(Ordering::Relaxed) {
                if user_event_sender.send_event(T::from(WinEvent::Update)).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        });
        self.ui.start();
    }

    fn on_user_event(&mut self, helper: &mut WindowHelper<T>, _event: T) {
        if self.ui.update() {
            helper.request_redraw();
        }

        for request in self.ui.take_window_requests() {
            let mut win = Win::<T>::new_child(request.ui);
            // The new window starts with the palette this window uses now.
            win.palette = self.palette.clone();

            let size = WindowSize::ScaledPixels(
                Vector2::new(request.width as f32, request.height as f32));
            // Child windows open centered over the window that opened them.
            let options = WindowCreationOptions::new_windowed(size, Some(WindowPosition::CenterOnParent));

            if request.modal {
                helper.create_modal_window(&request.title, options, Box::new(win));
            } else {
                helper.create_window(&request.title, options, Box::new(win));
            }
        }

        if self.ui.take_close_request() {
            helper.close_window();
        }

        // Window-visibility actions requested from outside the handler (e.g. a
        // system-tray click marshaled via `run_on_ui_thread`). The closure ran
        // at the top of `update()` above, so the command is set by now.
        match self.ui.take_window_command() {
            Some(WindowCommand::Show) => {
                helper.set_visible(true);
                helper.request_redraw();
            }
            Some(WindowCommand::Hide) => helper.set_visible(false),
            Some(WindowCommand::Quit) => helper.terminate_loop(),
            None => {}
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
        // Free textures whose ImageSource was dropped or re-rasterized since the
        // last frame. Done here (not in ImageSource::drop) because deleting a GL
        // texture needs this window's context current, which it is during on_draw.
        // A texture id lives in exactly one window's cache; requeue any that
        // aren't ours so the owning window frees them on its next paint.
        let mut not_mine = Vec::new();
        for id in crate::image_source::take_pending_evictions() {
            if self.image_cache.remove(&id).is_none() {
                not_mine.push(id);
            }
        }
        crate::image_source::requeue_evictions(not_mine);

        if let Some(palette) = self.ui.take_pending_palette() {
            crate::drawing::set_current_palette(palette.clone());
            self.palette = palette;
            // Dimensions may differ between palettes; re-run layout with them.
            self.ui.layout(self.width, self.height, helper.get_scale_factor());
        }
        let scale = helper.get_scale_factor();
        let mut theme = Classic::new(graphics, &self.drawable_registry, &self.palette, &mut self.image_cache, self.width as i32, self.height as i32, scale);
        self.ui.paint(&mut theme);
    }

    fn on_mouse_move(&mut self, helper: &mut WindowHelper<T>, position: Vector2<f32>) {
        //println!("Position: {} x {}", position.x, position.y);
        let position = Vector2::new(position.x.round() as i32, position.y.round() as i32);
        self.mouse_pos = position;
        let redraw = self.ui.on_mouse_move(position);
        self.apply_cursor(helper);
        if redraw {
            helper.request_redraw();
        }
    }

    fn on_mouse_button_down(&mut self, helper: &mut WindowHelper<T>, button: MouseButton) {
        let redraw = self.ui.on_mouse_button_down(self.mouse_pos, button);
        // A popup opened/closed by the click changes the cursor without a move.
        self.apply_cursor(helper);
        if redraw {
            helper.request_redraw();
        }
    }

    fn on_mouse_button_up(&mut self, helper: &mut WindowHelper<T>, button: MouseButton) {
        let redraw = self.ui.on_mouse_button_up(self.mouse_pos, button);
        self.apply_cursor(helper);
        if redraw {
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
        let consumed = self.ui.on_key_down(virtual_key_code, scancode, self.mod_state.clone());
        // Escape policy runs AFTER dispatch so a dialog or view consuming Esc
        // (e.g. a cancel button) is not followed by closing/terminating here.
        if !consumed && virtual_key_code == Some(VirtualKeyCode::Escape) {
            if self.ui.has_dismissable_popups() {
                self.ui.close_all_popups();
                helper.request_redraw();
                return;
            }
            if self.is_child {
                // Esc closes a child window; only the main window exits the app.
                helper.close_window();
            } else if self.close_hides {
                // Tray app: Esc hides the main window instead of quitting,
                // matching the X button (speedy2d's `with_hide_on_close`).
                helper.set_visible(false);
            } else {
                helper.terminate_loop();
            }
            return;
        }
        if consumed {
            helper.request_redraw();
        }
    }

    fn on_key_up(&mut self, helper: &mut WindowHelper<T>, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode) {
        println!("KeyCode: {:?}, scancode: {:?} up", virtual_key_code, scancode);
        if self.ui.on_key_up(virtual_key_code, scancode, self.mod_state.clone()) {
            helper.request_redraw();
        }
    }

    fn on_keyboard_char(&mut self, helper: &mut WindowHelper<T>, unicode_codepoint: char) {
        println!("Codepoint {:?}", unicode_codepoint);
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