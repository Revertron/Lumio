use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use speedy2d::dimen::Vector2;
use speedy2d::{Graphics2D, Window};
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, UserEventSender, VirtualKeyCode, WindowCreationOptions, WindowHandler, WindowHelper, WindowPosition, WindowSize, WindowStartupInfo};
use crate::app::WindowConfig;
use crate::input::MouseCursorType;
use crate::types::Point;
use crate::drawing::{DrawableRegistry, Palette};
use super::ui::{UI, WindowCommand};
use super::themes::*;
use super::themes::ImageCache;

/// Backend entry point behind [`crate::run`] for the GL backend: build the
/// speedy2d window from `config`, install the [`Win`] handler, and run the
/// event loop. Never returns (the loop owns the thread until the app exits).
pub(crate) fn run_gl(ui: UI, config: WindowConfig) -> ! {
    let size = if config.logical_size {
        WindowSize::ScaledPixels(Vector2::new(config.width as f32, config.height as f32))
    } else {
        WindowSize::PhysicalPixels(Vector2::new(config.width, config.height))
    };
    let position = if config.center { Some(WindowPosition::Center) } else { None };
    let options = WindowCreationOptions::new_windowed(size, position)
        .with_visible(config.visible)
        .with_hide_on_close(config.hide_on_close)
        .with_resizable(config.resizable)
        .with_minimizable(config.minimizable)
        .with_maximizable(config.maximizable);
    let window: Window<WinEvent> =
        Window::new_with_user_events(&config.title, options).expect("Failed to create the window");
    let sender = window.create_user_event_sender();
    let mut win = Win::new(ui, sender);
    win.set_palette(config.palette);
    window.run_loop(win)
}

pub struct Win<T> {
    ui: UI,
    drawable_registry: DrawableRegistry,
    palette: Palette,
    image_cache: ImageCache,
    width: u32,
    height: u32,
    mouse_pos: Point<i32>,
    mod_state: ModifiersState,
    /// Last cursor shape pushed to the OS, so we only call `set_cursor` on a
    /// real transition (avoids per-move churn).
    last_cursor: Option<MouseCursorType>,
    /// Cleared on drop so this window's update ticker thread stops.
    alive: Arc<AtomicBool>,
    /// Child/dialog windows close on Esc; the main window never does.
    is_child: bool,
    /// An Esc *press* on a child window asked to close it; the close is executed
    /// on the Esc *release*. Destroying the focused window while Esc is still
    /// physically held makes the OS move focus to the next window and re-deliver
    /// the held key to it as a fresh press (verified outside Lumio with a bare
    /// winit program), cascade-closing a stack of nested dialogs on one press.
    /// Closing on release destroys the window when no key is held.
    esc_pending_close: bool,
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
            mouse_pos: Point::new(-1, -1),
            mod_state: ModifiersState::default(),
            last_cursor: None,
            alive: Arc::new(AtomicBool::new(true)),
            is_child,
            esc_pending_close: false,
            t: PhantomData
        }
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
            helper.set_cursor(cursor.into());
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
            let options = WindowCreationOptions::new_windowed(size, Some(WindowPosition::CenterOnParent))
                .with_resizable(request.resizable)
                .with_minimizable(request.minimizable)
                .with_maximizable(request.maximizable);

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
        crate::image_source::drain_evictions(&mut self.image_cache);

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
        let position = Point::new(position.x.round() as i32, position.y.round() as i32);
        self.mouse_pos = position;
        let redraw = self.ui.on_mouse_move(position);
        self.apply_cursor(helper);
        if redraw {
            helper.request_redraw();
        }
    }

    fn on_mouse_button_down(&mut self, helper: &mut WindowHelper<T>, button: MouseButton) {
        let redraw = self.ui.on_mouse_button_down(self.mouse_pos, button.into());
        // A popup opened/closed by the click changes the cursor without a move.
        self.apply_cursor(helper);
        if redraw {
            helper.request_redraw();
        }
    }

    fn on_mouse_button_up(&mut self, helper: &mut WindowHelper<T>, button: MouseButton) {
        let redraw = self.ui.on_mouse_button_up(self.mouse_pos, button.into());
        self.apply_cursor(helper);
        if redraw {
            helper.request_redraw();
        }
    }

    fn on_mouse_wheel_scroll(&mut self, helper: &mut WindowHelper<T>, distance: MouseScrollDistance) {
        if self.ui.on_mouse_wheel_scroll(self.mouse_pos, distance.into()) {
            helper.request_redraw();
        }
    }


    fn on_key_down(&mut self, helper: &mut WindowHelper<T>, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode) {
        println!("KeyCode: {:?}, scancode: {:?} down", virtual_key_code, scancode);
        let consumed = self.ui.on_key_down(
            virtual_key_code.and_then(crate::input::VirtualKeyCode::from_speedy2d),
            scancode,
            self.mod_state.clone().into(),
        );
        // Escape policy runs AFTER dispatch so a dialog or view consuming Esc
        // (e.g. a cancel button) is not followed by closing here. Esc only
        // dismisses popups and closes child/dialog windows; it never closes or
        // quits the main window — that's up to the app's own handler/shortcut.
        // A child window is closed on the Esc *release* (see `esc_pending_close`),
        // not the press; popups are dismissed immediately (they don't move focus).
        if !consumed && virtual_key_code == Some(VirtualKeyCode::Escape) {
            if self.ui.has_dismissable_popups() {
                self.ui.close_all_popups();
                helper.request_redraw();
            } else if self.is_child {
                self.esc_pending_close = true;
            }
            return;
        }
        if consumed {
            helper.request_redraw();
        }
    }

    fn on_key_up(&mut self, helper: &mut WindowHelper<T>, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode) {
        println!("KeyCode: {:?}, scancode: {:?} up", virtual_key_code, scancode);
        // Execute a close requested by an earlier Esc press now that the key is
        // released (so destroying this window doesn't re-deliver the held key to
        // the next one — see `esc_pending_close`).
        if virtual_key_code == Some(VirtualKeyCode::Escape) && self.esc_pending_close {
            self.esc_pending_close = false;
            helper.close_window();
            return;
        }
        if self.ui.on_key_up(
            virtual_key_code.and_then(crate::input::VirtualKeyCode::from_speedy2d),
            scancode,
            self.mod_state.clone().into(),
        ) {
            helper.request_redraw();
        }
    }

    fn on_keyboard_char(&mut self, helper: &mut WindowHelper<T>, unicode_codepoint: char) {
        println!("Codepoint {:?}", unicode_codepoint);
        if self.ui.on_key_char(unicode_codepoint, self.mod_state.clone().into()) {
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