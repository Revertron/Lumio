//! Backend-neutral window loop: a winit 0.30 `ApplicationHandler` that owns the
//! window map, app-modal stack, input dispatch, escape/cursor policy, and the
//! per-tick UI command pump. The only backend-specific piece is the per-window
//! [`RenderSurface`] (how a laid-out UI is painted and presented); today the sole
//! impl is the tiny-skia/softbuffer [`surface_software::SoftwareSurface`], with a
//! GL surface planned (see docs/unified_window_loop.md). Gated to
//! `backend-software` until the GL surface lands.

mod input_winit;
#[cfg(feature = "backend-software")]
mod surface_software;
#[cfg(feature = "backend-gl")]
mod surface_gl;

use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Size};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowButtons, WindowId};

use self::input_winit::{key_event_to_vk, to_cursor_icon};
#[cfg(feature = "backend-software")]
use self::surface_software::{SoftwareBackend as Backend, SoftwareSurface as Surface};
#[cfg(feature = "backend-gl")]
use self::surface_gl::{GlBackend as Backend, GlSurface as Surface};
use crate::app::WindowConfig;
use crate::drawing::{DrawableRegistry, Palette};
use crate::input::{ModifiersState, MouseCursorType, VirtualKeyCode};
use crate::types::Point;
use crate::ui::{EscapeAction, WindowCommand, UI};

/// Per-window render target: how a laid-out UI is painted into this window's
/// backing store and presented. The backend (software / GL) chooses the concrete
/// type via a `cfg` alias; the loop only ever calls these two methods.
pub trait RenderSurface {
    /// Resize the backing store to `width`×`height` physical pixels.
    fn resize(&mut self, width: u32, height: u32);
    /// Paint the already-laid-out `ui` and present it. The surface owns its
    /// caches and per-frame eviction; `scale` is the current DPI scale.
    fn paint(&mut self, ui: &UI, palette: &Palette, registry: &DrawableRegistry, scale: f64);
}

/// UI update cadence (matches the GL backend's 15ms ticker).
const TICK: Duration = Duration::from_millis(15);

/// A window awaiting creation (the main window before `resumed`, or a child from
/// `take_window_requests`). Wraps the public [`WindowConfig`] (title/size/center/
/// visibility/buttons/palette) plus the loop-internal bits.
struct PendingWindow {
    config: WindowConfig,
    ui: UI,
    is_child: bool,
    modal: bool,
}

struct WindowState {
    window: Rc<Window>,
    surface: Surface,
    ui: UI,
    drawable_registry: DrawableRegistry,
    palette: Palette,
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
        self.surface.resize(self.width, self.height);
        // Defer the (relatively expensive) relayout to the next paint so a burst
        // of resize events coalesces into a single layout pass.
        self.pending_layout = true;
        self.window.request_redraw();
    }

    /// Render one frame: coalesced relayout + pending palette (both neutral),
    /// then hand off to the backend surface to paint and present.
    fn render(&mut self) {
        // Coalesced relayout from a resize/scale-factor burst.
        if self.pending_layout {
            self.ui.layout(self.width, self.height, self.scale);
            self.pending_layout = false;
        }

        if let Some(palette) = self.ui.take_pending_palette() {
            crate::drawing::set_current_palette(palette.clone());
            self.palette = palette;
            self.ui.layout(self.width, self.height, self.scale);
        }

        self.surface.paint(&self.ui, &self.palette, &self.drawable_registry, self.scale);
    }
}

struct App {
    backend: Backend,
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
        let cfg = &pw.config;
        let inner: Size = if cfg.logical_size {
            LogicalSize::new(cfg.width as f64, cfg.height as f64).into()
        } else {
            PhysicalSize::new(cfg.width, cfg.height).into()
        };
        let mut enabled_buttons = WindowButtons::CLOSE;
        if cfg.minimizable {
            enabled_buttons |= WindowButtons::MINIMIZE;
        }
        if cfg.maximizable {
            enabled_buttons |= WindowButtons::MAXIMIZE;
        }
        let attrs = WindowAttributes::default()
            .with_title(&cfg.title)
            // Always create hidden; we show the window only after positioning it
            // (below). winit has no "center on creation" attribute, so a centered
            // window would otherwise appear at the OS default corner and visibly
            // jump to center. Mirrors what the vendored speedy2d did. A tray app
            // (`visible: false`) is created hidden and stays hidden until shown.
            .with_visible(false)
            .with_resizable(cfg.resizable)
            .with_enabled_buttons(enabled_buttons)
            .with_inner_size(inner);
        // On Windows, use the icon embedded in the executable's resources (e.g.
        // set at build time via the `winres` crate, whose default embeds the app
        // icon at resource id 1). Mirrors what the vendored speedy2d did before
        // Lumio owned the window loop; without it the window keeps the default
        // icon. A missing resource just leaves the default icon.
        #[cfg(target_os = "windows")]
        let attrs = {
            use winit::platform::windows::IconExtWindows;
            use winit::window::Icon;
            match Icon::from_resource(1, None) {
                Ok(icon) => attrs.with_window_icon(Some(icon)),
                Err(_) => attrs,
            }
        };
        // Make this window's palette the active one for `@token`/typeface
        // resolution before any layout runs.
        crate::drawing::set_current_palette(pw.config.palette.clone());
        // The backend creates the window and its render surface together: the GL
        // backend must create the window alongside a matching GL config, so window
        // creation can't be hoisted out of the backend.
        let Some((window, surface)) = self.backend.create(event_loop, attrs) else {
            return;
        };
        // Position the (still-hidden) window before revealing it, so a centered
        // window never flashes at the corner first.
        if pw.config.center {
            center_on_primary(event_loop, &window);
        }
        // Now that it's positioned, reveal it (honoring the requested visibility;
        // a tray app stays hidden until it asks to be shown).
        if pw.config.visible {
            window.set_visible(true);
            // Some Linux WMs ignore a position set before the window is shown;
            // re-apply it after showing (speedy2d does the same).
            if pw.config.center {
                center_on_primary(event_loop, &window);
            }
        }
        let id = window.id();
        let scale = window.scale_factor();
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));

        let mut ui = pw.ui;
        ui.layout(w, h, scale);
        ui.start();
        window.request_redraw();

        let ws = WindowState {
            window,
            surface,
            ui,
            drawable_registry: DrawableRegistry::new(),
            palette: pw.config.palette,
            width: w,
            height: h,
            scale,
            mouse_pos: Point::new(-1, -1),
            mod_state: ModifiersState::default(),
            last_cursor: None,
            pending_layout: false,
            is_child: pw.is_child,
            hide_on_close: pw.config.hide_on_close,
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
                // Dialogs size in logical pixels, center on screen, and always
                // close (never hide) — matching their builder's intent. `visible`
                // and `hide_on_close` keep their `WindowConfig` defaults.
                let config = WindowConfig::new(req.title, req.width, req.height)
                    .logical_size()
                    .center()
                    .resizable(req.resizable)
                    .minimizable(req.minimizable)
                    .maximizable(req.maximizable)
                    .palette(ws.palette.clone());
                new_windows.push(PendingWindow { config, ui: req.ui, is_child: true, modal: req.modal });
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

/// Center `window` on the primary monitor (a no-op if there's no primary
/// monitor). winit has no "center on creation" attribute, so the loop positions
/// the window explicitly — while it's still hidden — before showing it.
fn center_on_primary(event_loop: &ActiveEventLoop, window: &Window) {
    if let Some(mon) = event_loop.primary_monitor() {
        let m = mon.size();
        let o = window.outer_size();
        let mp = mon.position();
        let x = mp.x + ((m.width.saturating_sub(o.width)) / 2) as i32;
        let y = mp.y + ((m.height.saturating_sub(o.height)) / 2) as i32;
        window.set_outer_position(PhysicalPosition::new(x, y));
    }
}

/// Open a window for `ui` and run the event loop until the app exits.
/// Child/modal windows (dialogs) are created on demand from the UI's window
/// requests. Blocks until the last window closes.
///
/// This is the backend implementation behind the neutral [`crate::run`]; prefer
/// calling that. Provided for direct/advanced use.
pub fn run_with_config(ui: UI, config: WindowConfig) -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        backend: Backend::new(),
        windows: HashMap::new(),
        main_window: None,
        modal_stack: Vec::new(),
        next_tick: Instant::now(),
        esc_pending_close: None,
        pending_main: Some(PendingWindow { config, ui, is_child: false, modal: false }),
    };
    event_loop.run_app(&mut app)
}

/// Back-compat launcher: opens a window sized in logical pixels. Equivalent to
/// `run_with_config(ui, WindowConfig::new(title, w, h).logical_size())`.
pub fn run(ui: UI, title: &str, width: u32, height: u32) -> Result<(), winit::error::EventLoopError> {
    run_with_config(ui, WindowConfig::new(title, width, height).logical_size())
}
