//! Software window backend: a winit 0.30 event loop that drives the tiny-skia
//! [`SoftwareTheme`] and blits the result to a softbuffer surface, feeding the
//! `crate::input` events Lumio expects. Multi-window + app-modal, mirroring the
//! GL handler in `src/win.rs`. Available under the `backend-software` feature.

mod input_winit;

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use tiny_skia::Pixmap;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Size};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowButtons, WindowId};

use self::input_winit::{key_event_to_vk, to_cursor_icon};
use crate::app::WindowConfig;
use crate::drawing::{DrawableRegistry, Palette};
use crate::input::{ModifiersState, MouseCursorType, VirtualKeyCode};
use crate::themes::{GlyphCache, SoftwareImageCache, SoftwareTheme};
use crate::types::Point;
use crate::ui::{EscapeAction, WindowCommand, UI};

type SbContext = softbuffer::Context<Rc<Window>>;
type SbSurface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

/// UI update cadence (matches the GL backend's 15ms ticker).
const TICK: Duration = Duration::from_millis(15);

/// A window awaiting creation (the main window before `resumed`, or a child from
/// `take_window_requests`).
struct PendingWindow {
    ui: UI,
    title: String,
    width: u32,
    height: u32,
    palette: Palette,
    is_child: bool,
    modal: bool,
    /// `width`/`height` are logical (scaled) pixels when true, physical when false.
    logical_size: bool,
    center: bool,
    visible: bool,
    hide_on_close: bool,
    resizable: bool,
    minimizable: bool,
    maximizable: bool,
}

struct WindowState {
    window: Rc<Window>,
    surface: SbSurface,
    pixmap: Pixmap,
    ui: UI,
    drawable_registry: DrawableRegistry,
    palette: Palette,
    image_cache: SoftwareImageCache,
    glyph_cache: GlyphCache,
    width: u32,
    height: u32,
    scale: f64,
    mouse_pos: Point<i32>,
    mod_state: ModifiersState,
    last_cursor: Option<MouseCursorType>,
    /// A resize/scale change happened; relayout once before the next paint
    /// (coalesces a burst of resize events into one layout pass).
    pending_layout: bool,
    is_child: bool,
    /// Main-window-only: the close button hides the window instead of exiting.
    hide_on_close: bool,
}

impl WindowState {
    /// Push the UI's requested cursor to the OS, only on a real change.
    fn apply_cursor(&mut self) {
        if let Some(cursor) = crate::input::cursor_transition(self.ui.current_cursor(), &mut self.last_cursor) {
            self.window.set_cursor(to_cursor_icon(cursor));
        }
    }

    fn on_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 || (size.width == self.width && size.height == self.height) {
            return;
        }
        self.width = size.width;
        self.height = size.height;
        if let Some(pm) = Pixmap::new(self.width, self.height) {
            self.pixmap = pm;
        }
        // Defer the (relatively expensive) relayout to the next paint so a burst
        // of resize events coalesces into a single layout pass.
        self.pending_layout = true;
        self.window.request_redraw();
    }

    /// Render one frame: evict stale textures, apply pending palette, paint into
    /// the pixmap, then blit it (RGBA → 0RGB) to the softbuffer surface.
    fn render(&mut self) {
        // Coalesced relayout from a resize/scale-factor burst.
        if self.pending_layout {
            self.ui.layout(self.width, self.height, self.scale);
            self.pending_layout = false;
        }

        crate::image_source::drain_evictions(&mut self.image_cache);

        if let Some(palette) = self.ui.take_pending_palette() {
            crate::drawing::set_current_palette(palette.clone());
            self.palette = palette;
            self.ui.layout(self.width, self.height, self.scale);
        }

        {
            let mut theme = SoftwareTheme::new(
                &mut self.pixmap,
                &self.drawable_registry,
                &self.palette,
                &mut self.image_cache,
                &mut self.glyph_cache,
                self.width as i32,
                self.height as i32,
                self.scale,
            );
            self.ui.paint(&mut theme);
        }

        let (w, h) = (self.width, self.height);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else {
            return;
        };
        if self.surface.resize(nw, nh).is_err() {
            return;
        }
        let Ok(mut buf) = self.surface.buffer_mut() else {
            return;
        };
        let src = self.pixmap.data(); // premultiplied RGBA8; opaque bg ⇒ premult == straight
        let n = (w * h) as usize;
        for i in 0..n {
            let s = i * 4;
            let r = src[s] as u32;
            let g = src[s + 1] as u32;
            let b = src[s + 2] as u32;
            buf[i] = (r << 16) | (g << 8) | b;
        }
        let _ = buf.present();
    }
}

struct App {
    context: Option<SbContext>,
    windows: HashMap<WindowId, WindowState>,
    main_window: Option<WindowId>,
    modal_stack: Vec<WindowId>,
    next_tick: Instant,
    pending_main: Option<PendingWindow>,
    /// A child/dialog window that an Escape *press* asked to close, executed on
    /// the Escape *release*. Destroying the focused window while Esc is still
    /// physically held makes the OS move focus to the next window and
    /// re-deliver the held key to it as a fresh `Pressed` (verified outside
    /// Lumio with a bare winit program) — which would cascade-close a whole
    /// stack of nested dialogs on one press. Closing on release destroys the
    /// window when no key is held, so nothing is re-delivered.
    esc_pending_close: Option<WindowId>,
}

impl App {
    fn create_window(&mut self, event_loop: &ActiveEventLoop, pw: PendingWindow) {
        let inner: Size = if pw.logical_size {
            LogicalSize::new(pw.width as f64, pw.height as f64).into()
        } else {
            PhysicalSize::new(pw.width, pw.height).into()
        };
        let mut enabled_buttons = WindowButtons::CLOSE;
        if pw.minimizable {
            enabled_buttons |= WindowButtons::MINIMIZE;
        }
        if pw.maximizable {
            enabled_buttons |= WindowButtons::MAXIMIZE;
        }
        let attrs = WindowAttributes::default()
            .with_title(&pw.title)
            .with_visible(pw.visible)
            .with_resizable(pw.resizable)
            .with_enabled_buttons(enabled_buttons)
            .with_inner_size(inner);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("software_window: failed to create window: {e}");
                return;
            }
        };
        // The GL backend centers via `WindowPosition::Center`; winit has no such
        // attribute, so place the window on the primary monitor's center here.
        if pw.center && let Some(mon) = event_loop.primary_monitor() {
            let m = mon.size();
            let o = window.outer_size();
            let mp = mon.position();
            let x = mp.x + ((m.width.saturating_sub(o.width)) / 2) as i32;
            let y = mp.y + ((m.height.saturating_sub(o.height)) / 2) as i32;
            window.set_outer_position(PhysicalPosition::new(x, y));
        }
        // Make this window's palette the active one for `@token`/typeface
        // resolution (mirrors `Win::set_palette` on the GL backend).
        crate::drawing::set_current_palette(pw.palette.clone());
        let id = window.id();
        let scale = window.scale_factor();
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));

        if self.context.is_none() {
            self.context = Some(softbuffer::Context::new(window.clone()).expect("softbuffer context"));
        }
        let mut surface = softbuffer::Surface::new(self.context.as_ref().unwrap(), window.clone())
            .expect("softbuffer surface");
        let _ = surface.resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap());
        let pixmap = Pixmap::new(w, h).expect("pixmap");

        let mut ui = pw.ui;
        ui.layout(w, h, scale);
        ui.start();
        window.request_redraw();

        let ws = WindowState {
            window,
            surface,
            pixmap,
            ui,
            drawable_registry: DrawableRegistry::new(),
            palette: pw.palette,
            image_cache: SoftwareImageCache::new(),
            glyph_cache: GlyphCache::new(),
            width: w,
            height: h,
            scale,
            mouse_pos: Point::new(-1, -1),
            mod_state: ModifiersState::default(),
            last_cursor: None,
            pending_layout: false,
            is_child: pw.is_child,
            hide_on_close: pw.hide_on_close,
        };
        if self.main_window.is_none() {
            self.main_window = Some(id);
        }
        if pw.modal {
            self.modal_stack.push(id);
        }
        self.windows.insert(id, ws);
    }

    /// Close a window. Closing the main window (or the last window) exits.
    fn handle_close(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        if Some(id) == self.main_window {
            event_loop.exit();
            return;
        }
        self.windows.remove(&id);
        self.modal_stack.retain(|x| *x != id);
        // Refocus whatever modal is now on top (mirrors the GL backend), so the
        // revealed dialog takes input and a subsequent Esc targets it.
        if let Some(&top) = self.modal_stack.last()
            && let Some(ws) = self.windows.get(&top)
        {
            ws.window.focus_window();
        }
        if self.windows.is_empty() {
            event_loop.exit();
        }
    }

    /// Per-tick UI update across all windows: drain tasks (`update`), spawn
    /// child/modal windows, and process close/show/hide/quit requests.
    fn tick(&mut self, event_loop: &ActiveEventLoop) {
        let ids: Vec<WindowId> = self.windows.keys().copied().collect();
        let mut new_windows: Vec<PendingWindow> = Vec::new();
        let mut to_close: Vec<WindowId> = Vec::new();
        let mut quit = false;
        for id in ids {
            let Some(ws) = self.windows.get_mut(&id) else { continue };
            if ws.ui.update() {
                ws.window.request_redraw();
            }
            for req in ws.ui.take_window_requests() {
                new_windows.push(PendingWindow {
                    ui: req.ui,
                    title: req.title,
                    width: req.width,
                    height: req.height,
                    palette: ws.palette.clone(),
                    is_child: true,
                    modal: req.modal,
                    // Dialogs size in logical pixels, center on screen, and always
                    // close (never hide) — matching their builder's intent.
                    logical_size: true,
                    center: true,
                    visible: true,
                    hide_on_close: false,
                    resizable: req.resizable,
                    minimizable: req.minimizable,
                    maximizable: req.maximizable,
                });
            }
            if ws.ui.take_close_request() {
                to_close.push(id);
            }
            match ws.ui.take_window_command() {
                Some(WindowCommand::Show) => {
                    ws.window.set_visible(true);
                    ws.window.request_redraw();
                }
                Some(WindowCommand::Hide) => ws.window.set_visible(false),
                Some(WindowCommand::Quit) => quit = true,
                None => {}
            }
        }
        if quit {
            event_loop.exit();
            return;
        }
        for id in to_close {
            self.handle_close(event_loop, id);
        }
        for pw in new_windows {
            self.create_window(event_loop, pw);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }
        if let Some(pw) = self.pending_main.take() {
            self.create_window(event_loop, pw);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        // Self-routed events (need the window map / event loop, not a held borrow).
        match event {
            WindowEvent::RedrawRequested => {
                if let Some(ws) = self.windows.get_mut(&window_id) {
                    ws.render();
                }
                return;
            }
            WindowEvent::CloseRequested => {
                // Tray apps: the main window's close button hides it instead of
                // exiting (mirrors speedy2d's `with_hide_on_close`).
                if let Some(ws) = self.windows.get(&window_id)
                    && ws.hide_on_close
                    && Some(window_id) == self.main_window
                {
                    ws.window.set_visible(false);
                    return;
                }
                self.handle_close(event_loop, window_id);
                return;
            }
            _ => {}
        }

        // App-modal gating: while a modal is open, non-top windows get no input
        // (resize/scale still apply; close/redraw handled above).
        let blocked = self.modal_stack.last().is_some_and(|&top| top != window_id);

        // Esc closes a child/dialog window on key-UP, not key-down (see below).
        // These flag the press (deferred close) and the release (do the close).
        let mut esc_down_close = false;
        let mut esc_up = false;
        {
            let Some(ws) = self.windows.get_mut(&window_id) else { return };
            match event {
                WindowEvent::Resized(size) => ws.on_resize(size),
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    ws.scale = scale_factor;
                    ws.pending_layout = true;
                    ws.window.request_redraw();
                }
                WindowEvent::ModifiersChanged(m) => ws.mod_state = m.state().into(),
                _ if blocked => {}
                WindowEvent::CursorMoved { position, .. } => {
                    ws.mouse_pos = Point::new(position.x.round() as i32, position.y.round() as i32);
                    let redraw = ws.ui.on_mouse_move(ws.mouse_pos);
                    ws.apply_cursor();
                    if redraw {
                        ws.window.request_redraw();
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let redraw = match state {
                        ElementState::Pressed => ws.ui.on_mouse_button_down(ws.mouse_pos, button.into()),
                        ElementState::Released => ws.ui.on_mouse_button_up(ws.mouse_pos, button.into()),
                    };
                    ws.apply_cursor();
                    if redraw {
                        ws.window.request_redraw();
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if ws.ui.on_mouse_wheel_scroll(ws.mouse_pos, delta.into()) {
                        ws.window.request_redraw();
                    }
                }
                WindowEvent::KeyboardInput { event: ke, .. } => {
                    let mut redraw = false;
                    if let (ElementState::Pressed, Some(text)) = (ke.state, &ke.text) {
                        for c in text.chars() {
                            if ws.ui.on_key_char(c, ws.mod_state.clone()) {
                                redraw = true;
                            }
                        }
                    }
                    let vk = key_event_to_vk(&ke);
                    let consumed = match ke.state {
                        // scancode is opaque to Lumio (never inspected) → 0.
                        ElementState::Pressed if !ke.repeat => ws.ui.on_key_down(vk, 0, ws.mod_state.clone()),
                        ElementState::Pressed => false,
                        ElementState::Released => ws.ui.on_key_up(vk, 0, ws.mod_state.clone()),
                    };
                    redraw |= consumed;
                    // Escape policy (after dispatch, only if not consumed). The
                    // decision is centralized in `UI::escape_press_action`; a
                    // child window closes on the Escape *release*, not the press
                    // (see `EscapeAction::CloseChildWindow` / `esc_pending_close`).
                    if vk == Some(VirtualKeyCode::Escape) {
                        match ke.state {
                            ElementState::Pressed if !ke.repeat && !consumed => {
                                match ws.ui.escape_press_action(ws.is_child) {
                                    EscapeAction::DismissedPopups => redraw = true,
                                    EscapeAction::CloseChildWindow => esc_down_close = true,
                                    EscapeAction::None => {}
                                }
                            }
                            ElementState::Released => esc_up = true,
                            _ => {}
                        }
                    }
                    if redraw {
                        ws.window.request_redraw();
                    }
                }
                _ => {}
            }
        }

        // Esc-press on a child window only *requests* the close; the actual
        // destroy waits for the Esc release (so the held key isn't re-delivered
        // to the next window — see the note in the Escape policy above).
        if esc_down_close {
            self.esc_pending_close = Some(window_id);
        }
        if esc_up {
            if let Some(id) = self.esc_pending_close.take() {
                self.handle_close(event_loop, id);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_tick {
            self.next_tick = now + TICK;
            self.tick(event_loop);
        }
        if self.windows.is_empty() {
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }
}

/// Open a window for `ui` and run the software event loop until the app exits.
/// Child/modal windows (dialogs) are created on demand from the UI's window
/// requests. Blocks until the last window closes.
///
/// This is the backend implementation behind the neutral [`crate::run`]; prefer
/// calling that. Provided for direct/advanced use.
pub fn run_with_config(ui: UI, config: WindowConfig) -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        context: None,
        windows: HashMap::new(),
        main_window: None,
        modal_stack: Vec::new(),
        next_tick: Instant::now(),
        esc_pending_close: None,
        pending_main: Some(PendingWindow {
            ui,
            title: config.title,
            width: config.width,
            height: config.height,
            palette: config.palette,
            is_child: false,
            modal: false,
            logical_size: config.logical_size,
            center: config.center,
            visible: config.visible,
            hide_on_close: config.hide_on_close,
            resizable: config.resizable,
            minimizable: config.minimizable,
            maximizable: config.maximizable,
        }),
    };
    event_loop.run_app(&mut app)
}

/// Back-compat launcher: opens a window sized in logical pixels. Equivalent to
/// `run_with_config(ui, WindowConfig::new(title, w, h).logical_size())`.
pub fn run(ui: UI, title: &str, width: u32, height: u32) -> Result<(), winit::error::EventLoopError> {
    run_with_config(ui, WindowConfig::new(title, width, height).logical_size())
}
