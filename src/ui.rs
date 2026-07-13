use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use super::input::{KeyScancode, ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance, VirtualKeyCode};

use super::containers::Frame;
use super::events::{EventData, EventType};
use super::shortcut::Shortcut;
use super::themes::Theme;
use super::traits::{Element, View};
use super::types::Point;
use super::themes::Typeface;

use super::views::{Button, Edit, Label, CheckBox, RadioButton, ComboBox, ScrollView, ProgressBar, TabView, List, RecyclerView, ImageButton, ImageView, PopupMenu, Separator, SplitPanel, StatusBar, Memo, NotificationStack, TableView, TableColumn, TableRow, Grid, RichText, MenuBar, Menu, MenuItemTag, Slider, TreeView, IconList};
use super::views::{Dimension, Visibility};
use std::time::Duration;

/// Controls how a popup interacts with the rest of the UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupMode {
    /// Dismisses when the user clicks outside the popup.
    Popup,
    /// Blocks all input to the root tree until closed.
    Modal,
    /// Lets unhandled input fall through to overlays/root below. Never auto-dismissed.
    Transparent,
}

/// Controls which direction the popup expands from the anchor point (x, y).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopupDirection {
    /// (x, y) is the top-left corner of the popup.
    BottomRight,
    /// (x, y) is the top-right corner of the popup.
    BottomLeft,
    /// (x, y) is the bottom-left corner of the popup.
    TopRight,
    /// (x, y) is the bottom-right corner of the popup.
    TopLeft,
    /// (x, y) is the center of the popup.
    Center,
}

struct PopupEntry {
    element: Element,
    x: i32,
    y: i32,
    mode: PopupMode,
    /// Stable identity for embedders that mirror overlays into their own surfaces
    /// (see [`UI::overlay_snapshot`]); assigned from a per-UI monotonic counter.
    token: u64,
}

/// One overlay's geometry for an embedder that presents overlays in its own surfaces (an
/// "external popups" host, see [`UI::set_external_popups`]). Coordinates are in window space:
/// the overlay's on-screen rect is `(x, y, width, height)`; paint it via
/// `crate::render::render_overlay_to_pixmap` (software builds) and route its input through
/// [`UI::overlay_origin`]-translated coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayDesc {
    /// Stable identity of the overlay across snapshots (a new popup = a new token).
    pub token: u64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The reserved [`OverlayDesc::token`] for the tooltip (it is not an overlay entry internally,
/// but external-popup embedders present it the same way).
pub const TOOLTIP_TOKEN: u64 = u64::MAX;

const TOOLTIP_DELAY_MS: u128 = 700;
const TOOLTIP_ID: &str = "__tooltip__";
/// Two left clicks within this window (and within `DOUBLE_CLICK_DISTANCE`
/// of each other, on the same view) fire a `DoubleClick` event. Matches the
/// threshold `Edit` uses internally for word selection.
const DOUBLE_CLICK_MS: u128 = 400;
const DOUBLE_CLICK_DISTANCE: i32 = 4;

struct TooltipPopup {
    element: Element,
    x: i32,
    y: i32,
}

/// A closure queued from any thread, executed on the UI thread with
/// `&mut UI` at the start of the next update tick (~15 ms latency worst case).
pub type UiTask = Box<dyn FnOnce(&mut UI) + Send + 'static>;

/// A `Send + Sync + Clone` handle to a UI's task queue. Obtain it via
/// [`UI::handle`] before the window starts and clone it into worker threads;
/// queued tasks run on the UI thread, Android's `runOnUiThread` style.
#[derive(Clone)]
pub struct UiHandle {
    tasks: Arc<Mutex<VecDeque<UiTask>>>,
}

impl UiHandle {
    /// Queues `f` to run on the UI thread with `&mut UI` at the next update
    /// tick. Tasks queued from inside a running task execute on the
    /// following tick. Any executed task requests a redraw.
    pub fn run_on_ui_thread<F: FnOnce(&mut UI) + Send + 'static>(&self, f: F) {
        self.tasks.lock().unwrap().push_back(Box::new(f));
    }
}

impl Drop for UI {
    fn drop(&mut self) {
        if let Some(on_close) = self.on_close.take() {
            on_close(self);
        }
    }
}

pub struct UI {
    width: u32,
    height: u32,
    scale: f64,
    typeface: Typeface,
    root: Option<Element>,
    types: HashMap<String, fn() -> Element>,
    on_start: Option<Box<dyn FnMut(&mut UI)>>,
    /// Handler for OS file drag-and-drop onto this window; returns true to
    /// request a redraw. One path per call (winit delivers drops one by one).
    on_file_drop: Option<Box<dyn FnMut(&mut UI, std::path::PathBuf) -> bool>>,
    /// Global left-button-release hook, called before positional dispatch.
    /// Lets app code implement press-and-hold interactions that must see the
    /// release wherever it happens (e.g. hold-to-record). Returns true to
    /// request a redraw; the event still reaches views afterwards.
    on_mouse_up: Option<Box<dyn FnMut(&mut UI, Point<i32>) -> bool>>,
    overlays: Vec<PopupEntry>,
    mouse_pos: Point<i32>,
    /// Last known keyboard-modifier state, kept current by the window loop
    /// (`ModifiersChanged`) and by key dispatch. Lets mouse handlers (which
    /// carry no modifier argument) implement Ctrl/Shift+Click behavior.
    modifiers: ModifiersState,
    tooltip_view_id: Option<String>,
    tooltip_hover_start: Option<Instant>,
    tooltip_showing: bool,
    tooltip_popup: Option<TooltipPopup>,
    needs_relayout: bool,
    notification_stack: Option<Element>,
    /// Ids queued for removal during event dispatch. Processed at the next
    /// `update()` tick so removal can be requested from inside handlers
    /// without invalidating the iterator the dispatcher is walking.
    pending_removals: Vec<String>,
    /// The cursor shape requested by the view under the pointer during the
    /// current `on_mouse_move` dispatch. Reset each move; resolved via
    /// [`UI::current_cursor`] and applied by the window handler.
    requested_cursor: Option<MouseCursorType>,
    /// A palette change requested from app code (e.g. inside an event
    /// handler); picked up by the window handler before the next paint.
    pending_palette: Option<crate::drawing::Palette>,
    /// Named attribute bundles, applied to a view via `style="name"` in
    /// layout XML before the view's own attributes (own attributes win).
    /// Registered from `<Style name="..." .../>` elements or [`UI::add_style`].
    styles: HashMap<String, Vec<(String, String)>>,
    /// Id of the view that currently holds focus, as observed by the last
    /// `sync_focus()` sweep. Drives `FocusGained`/`FocusLost` events.
    focus_owner: Option<String>,
    /// Id of the deepest view under the cursor with a hover listener, as
    /// observed by the last `sync_hover()`. Drives `HoverEnter`/`HoverExit`.
    hover_owner: Option<String>,
    /// Time, position and DoubleClick-listener target of the last left
    /// mouse-button press, for central double-click detection.
    last_click: Option<(Instant, Point<i32>, Option<String>)>,
    /// True while dispatching a right-click whose `ContextMenu` listener
    /// returned true: built-in context menus (Edit, Memo, Label, RichText)
    /// check it and stay closed, and the click-missed-overlays popup
    /// dismissal is skipped so a menu the handler opened survives.
    context_menu_suppressed: bool,
    /// Application-wide keyboard accelerators, dispatched when a key-down
    /// was not consumed by the focused view / overlays.
    shortcuts: HashMap<Shortcut, Box<dyn FnMut(&mut UI) -> bool>>,
    /// New OS windows queued via [`UI::open_window`]; drained by the window
    /// handler on the next update tick.
    window_requests: Vec<WindowRequest>,
    /// Set by [`UI::close_window`]; the window handler closes this UI's
    /// window when it sees the flag.
    close_requested: bool,
    /// Set by [`UI::request_show`] / [`request_hide`] / [`request_quit`]; the
    /// window handler applies it (via the `WindowHelper`) on the next tick.
    /// Used to drive window visibility from a system-tray handler.
    pending_window_command: Option<WindowCommand>,
    /// Cross-thread task queue, shared with [`UiHandle`]s; drained and
    /// executed at the start of every `update()` tick.
    tasks: Arc<Mutex<VecDeque<UiTask>>>,
    /// Events queued via [`UI::defer_event`] during the update tree-walk
    /// (when the firing view is mutably borrowed); fired at the end of
    /// `update()` once all borrows are released.
    deferred_events: Vec<(String, EventType, EventData)>,
    /// Runs once when this UI is dropped (the window handler is dropped on
    /// window close). Set it on the main window's UI for app shutdown work.
    on_close: Option<Box<dyn FnOnce(&mut UI)>>,
    /// Last known window geometry, kept current by the window loop:
    /// outer position + inner size while un-maximized, plus the maximized
    /// flag. Lets apps persist window state from `on_close`.
    window_pos: (i32, i32),
    window_normal_size: (u32, u32),
    window_maximized: bool,
    window_focused: bool,
    /// External-popups mode ([`UI::set_external_popups`]): the embedder presents
    /// non-`Transparent` overlays and the tooltip in its own surfaces — `paint()` draws the
    /// root (+ transparent overlays) only, and popup/tooltip geometry is NOT clamped to the
    /// window (it may extend past it; that's the point).
    external_popups: bool,
    /// Monotonic source of [`PopupEntry::token`]s.
    next_overlay_token: u64,
}

/// A request to open a new OS window, queued via [`UI::open_window`] and
/// applied by the window handler on the next update tick.
pub struct WindowRequest {
    pub title: String,
    /// Inner window size in device-independent pixels.
    pub width: u32,
    pub height: u32,
    /// The fully built UI for the new window, with event handlers wired.
    pub ui: UI,
    /// Application-modal: until this window closes, all other windows ignore
    /// mouse/keyboard/close input, and clicking them refocuses this window.
    pub modal: bool,
    /// Whether the user can resize the window. A non-resizable window also
    /// can't be maximized on most platforms.
    pub resizable: bool,
    /// Whether the window shows an enabled minimize button.
    pub minimizable: bool,
    /// Whether the window shows an enabled maximize button.
    pub maximizable: bool,
}

/// A window-visibility action requested from outside the window handler
/// (e.g. a system-tray click), queued via [`UI::request_show`] /
/// [`UI::request_hide`] / [`UI::request_quit`] and applied by the window
/// handler on the next update tick.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    Show,
    Hide,
    Quit,
}

/// What a window backend should do about an unconsumed Escape key *press*, as
/// decided by [`UI::escape_press_action`]. The action is backend-neutral; each
/// loop maps it to its own redraw/close primitives.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EscapeAction {
    /// Nothing to do (no dismissable popups, and this is the main window).
    None,
    /// Dismissable popups were just closed; the window should redraw.
    DismissedPopups,
    /// A child/dialog window should close. The actual destroy is deferred to the
    /// Escape *release*, not done here: destroying the focused window while Esc
    /// is still physically held makes the OS move focus to the next window and
    /// re-deliver the held key to it as a fresh press (verified outside Lumio
    /// with a bare winit program), which would cascade-close a whole stack of
    /// nested dialogs on one press. Closing on release destroys the window when
    /// no key is held, so nothing is re-delivered. (Popups don't move OS focus,
    /// so `DismissedPopups` acts immediately on the press.)
    CloseChildWindow,
}

#[allow(dead_code)]
impl UI {
    pub fn new(width: u32, height: u32, typeface: Typeface, scale: f64) -> Self {
        let mut ui = UI {
            width, height, typeface, scale, root: None, types: HashMap::new(),
            on_start: None, on_file_drop: None, on_mouse_up: None, overlays: Vec::new(), mouse_pos: Point::new(0, 0),
            modifiers: ModifiersState::default(),
            tooltip_view_id: None, tooltip_hover_start: None, tooltip_showing: false, tooltip_popup: None, needs_relayout: false,
            notification_stack: None,
            pending_removals: Vec::new(),
            requested_cursor: None,
            pending_palette: None,
            styles: HashMap::new(),
            focus_owner: None,
            hover_owner: None,
            last_click: None,
            context_menu_suppressed: false,
            shortcuts: HashMap::new(),
            window_requests: Vec::new(),
            close_requested: false,
            pending_window_command: None,
            tasks: Arc::new(Mutex::new(VecDeque::new())),
            deferred_events: Vec::new(),
            on_close: None,
            window_pos: (0, 0),
            window_normal_size: (0, 0),
            window_maximized: false,
            window_focused: true,
            external_popups: false,
            next_overlay_token: 0,
        };
        ui.register::<Label>("Label");
        ui.register::<Button>("Button");
        ui.register::<CheckBox>("CheckBox");
        ui.register::<RadioButton>("RadioButton");
        ui.register::<ComboBox>("ComboBox");
        ui.register::<ScrollView>("ScrollView");
        ui.register::<ProgressBar>("ProgressBar");
        ui.register::<TabView>("TabView");
        ui.register::<Edit>("Edit");
        ui.register::<List>("List");
        ui.register::<RecyclerView>("RecyclerView");
        ui.register::<ImageButton>("ImageButton");
        ui.register::<ImageView>("ImageView");
        ui.register::<PopupMenu>("PopupMenu");
        ui.register::<Separator>("Separator");
        ui.register::<SplitPanel>("SplitPanel");
        ui.register::<StatusBar>("StatusBar");
        ui.register::<Memo>("Memo");
        ui.register::<Frame>("Frame");
        ui.register::<NotificationStack>("NotificationStack");
        ui.register::<TableView>("TableView");
        ui.register::<TableColumn>("TableColumn");
        ui.register::<TableRow>("TableRow");
        ui.register::<Grid>("Grid");
        ui.register::<RichText>("RichText");
        ui.register::<MenuBar>("MenuBar");
        ui.register::<Menu>("Menu");
        ui.register::<MenuItemTag>("MenuItem");
        ui.register::<Slider>("Slider");
        ui.register::<TreeView>("TreeView");
        ui.register::<IconList>("IconList");
        ui
    }

    pub fn add_view(&mut self, view: Element) {
        match &self.root {
            None => {
                self.root = Some(view);
            }
            Some(root) => {
                let mut root = root.try_borrow_mut().unwrap();
                root.as_container_mut().unwrap().add_view(view);
            }
        }
    }

    /// Queue a view for removal from its parent. Safe to call from inside an
    /// event handler — the actual tree mutation happens at the next `update()`
    /// tick, after the dispatcher has released its borrow on the firing view.
    /// Returns silently if no view with this id exists at flush time.
    pub fn remove_view(&mut self, id: &str) {
        self.pending_removals.push(id.to_owned());
    }

    fn process_pending_removals(&mut self) -> bool {
        if self.pending_removals.is_empty() {
            return false;
        }
        let ids: Vec<String> = std::mem::take(&mut self.pending_removals);
        let mut any_removed = false;
        for id in ids {
            if self.do_remove_view(&id) {
                any_removed = true;
            }
        }
        if any_removed {
            self.needs_relayout = true;
        }
        any_removed
    }

    fn do_remove_view(&mut self, id: &str) -> bool {
        // Try overlays first; an overlay element could itself match by id, in
        // which case we drop the whole overlay entry. Otherwise recurse into
        // the overlay's container children.
        if let Some(pos) = self.overlays.iter().position(|e| e.element.borrow().get_id() == id) {
            self.overlays.remove(pos);
            return true;
        }
        for entry in &self.overlays {
            if let Some(container) = entry.element.borrow_mut().as_container_mut() {
                if container.remove_view(id) {
                    return true;
                }
            }
        }
        if let Some(root) = &self.root {
            if root.borrow().get_id() == id {
                self.root = None;
                return true;
            }
            if let Some(container) = root.borrow_mut().as_container_mut() {
                if container.remove_view(id) {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_view(&self, id: &str) -> Option<Element> {
        // Search overlays first (topmost)
        for entry in self.overlays.iter().rev() {
            let element = &entry.element;
            if element.borrow().get_id() == id {
                return Some(Rc::clone(element));
            }
            if let Some(container) = element.borrow().as_container() {
                if let Some(found) = container.get_view(id) {
                    return Some(found);
                }
            }
        }
        // Then search root
        match &self.root {
            None => None,
            Some(root) => {
                if root.borrow().get_id() == id {
                    return Some(Rc::clone(root));
                }
                if let Some(root) = root.borrow().as_container() {
                    return root.get_view(id);
                }
                None
            }
        }
    }

    /// Recursively search the entire view tree — every overlay plus the root
    /// hierarchy — and collect every view for which `predicate` returns `true`.
    ///
    /// The predicate receives each view as `&dyn View`; downcast inside it to
    /// inspect a concrete type. Matches are returned as cloned [`Element`]
    /// handles in pre-order, so a matching container precedes its children.
    ///
    /// ```ignore
    /// // Every checked RadioButton in the "color" group.
    /// let checked = ui.find_with(&|v| {
    ///     v.as_any().downcast_ref::<RadioButton>()
    ///         .is_some_and(|rb| rb.get_group() == "color" && rb.is_checked())
    /// });
    /// ```
    pub fn find_with(&self, predicate: &dyn Fn(&dyn View) -> bool) -> Vec<Element> {
        let mut result = Vec::new();
        // Search overlays
        for entry in &self.overlays {
            Self::collect_matching(&entry.element, predicate, &mut result);
        }
        // Search root
        if let Some(root) = &self.root {
            Self::collect_matching(root, predicate, &mut result);
        }
        result
    }

    fn collect_matching(element: &Element, predicate: &dyn Fn(&dyn View) -> bool, result: &mut Vec<Element>) {
        let view = element.borrow();
        if predicate(&*view) {
            result.push(Rc::clone(element));
        }
        if let Some(container) = view.as_container() {
            for child in container.get_views() {
                Self::collect_matching(&child, predicate, result);
            }
        }
    }

    /// The root element of the view tree, for the accessibility tree builder.
    pub(crate) fn root_element(&self) -> Option<Element> {
        self.root.clone()
    }

    /// Overlay elements with their window-space origins (bottom-to-top order),
    /// for the accessibility tree builder. Child rects inside an overlay are
    /// parent-relative; the returned `(x, y)` is the offset its subtree is
    /// painted and hit-tested at.
    pub(crate) fn overlay_elements(&self) -> Vec<(Element, Point<i32>)> {
        self.overlays.iter()
            .map(|e| (Rc::clone(&e.element), Point::from((e.x, e.y))))
            .collect()
    }

    /// Id of the view holding focus, as observed by the last `sync_focus()`.
    pub(crate) fn access_focus_owner(&self) -> Option<String> {
        self.focus_owner.clone()
    }

    /// Move keyboard focus to `element`, clearing it everywhere else (root
    /// and overlays) and firing the usual `FocusLost`/`FocusGained` events.
    /// The target must be focusable, enabled and visible; returns whether
    /// focus actually changed. Used by assistive-technology focus requests;
    /// also the programmatic-focus primitive for app code.
    pub fn set_focus_to(&mut self, element: &Element) -> bool {
        {
            let view = element.borrow();
            let focusable = view.get_state().map(|s| s.focusable && s.enabled).unwrap_or(false);
            if !focusable || view.get_visibility() != Visibility::Visible {
                return false;
            }
        }
        // Containers cascade `set_focused(false)` to their children, so
        // clearing the roots clears the whole tree.
        for entry in &self.overlays {
            entry.element.borrow().set_focused(false);
        }
        if let Some(root) = &self.root {
            root.borrow().set_focused(false);
        }
        element.borrow().set_focused(true);
        self.sync_focus()
    }

    /// Moves keyboard focus to the next focusable view in document order,
    /// wrapping at the end; when nothing has focus yet the first focusable
    /// view is focused. Bound to Tab. Returns true if focus changed.
    pub fn focus_next_view(&mut self) -> bool {
        self.move_focus(false)
    }

    /// Moves keyboard focus to the previous focusable view in document order,
    /// wrapping at the start. Bound to Shift+Tab. Returns true if focus changed.
    pub fn focus_prev_view(&mut self) -> bool {
        self.move_focus(true)
    }

    fn move_focus(&mut self, backwards: bool) -> bool {
        // Under a modal overlay keyboard focus is confined to it.
        let scope = match self.overlays.iter().rev().find(|e| e.mode == PopupMode::Modal) {
            Some(entry) => Rc::clone(&entry.element),
            None => match &self.root {
                Some(root) => Rc::clone(root),
                None => return false,
            },
        };
        let mut focusables = Vec::new();
        Self::collect_focusables(&scope, &mut focusables);
        if focusables.is_empty() {
            return false;
        }
        let current = focusables.iter()
            .position(|el| el.borrow().get_state().map(|s| s.focused).unwrap_or(false));
        let len = focusables.len();
        let next = match current {
            Some(i) if backwards => (i + len - 1) % len,
            Some(i) => (i + 1) % len,
            None if backwards => len - 1,
            None => 0,
        };
        let target = Rc::clone(&focusables[next]);
        self.set_focus_to(&target);
        // Report the move by element identity, not by set_focus_to's result:
        // its sync_focus diff compares view ids, which reads as "no change"
        // when two views share an id — the key must still count as consumed
        // (and trigger a redraw) because focus really moved.
        current != Some(next)
    }

    /// Collects focusable, visible, enabled views in document order. Descends
    /// only into children a container currently shows (`hit_test_views`), so
    /// views on inactive `TabView` tabs are not reachable with Tab.
    fn collect_focusables(element: &Element, result: &mut Vec<Element>) {
        let view = element.borrow();
        if view.get_visibility() != Visibility::Visible || !view.is_enabled() {
            return;
        }
        if view.get_state().map(|s| s.focusable).unwrap_or(false) {
            result.push(Rc::clone(element));
        }
        if let Some(container) = view.as_container() {
            for child in container.hit_test_views() {
                Self::collect_focusables(&child, result);
            }
        }
    }

    /// Clear the text selection in every view (overlays + root). Selectable
    /// views (`Label`, `RichText`) drop their highlight; everything else is a
    /// no-op. Called when a view starts a new selection so only one view holds
    /// a selection at a time. Uses immutable borrows only, so it is safe to call
    /// from inside a mouse handler mid-dispatch.
    pub fn deselect_text(&self) {
        for entry in &self.overlays {
            Self::deselect_recursive(&entry.element);
        }
        if let Some(root) = &self.root {
            Self::deselect_recursive(root);
        }
    }

    fn deselect_recursive(element: &Element) {
        let view = element.borrow();
        view.deselect_text();
        if let Some(container) = view.as_container() {
            for child in container.get_views() {
                Self::deselect_recursive(&child);
            }
        }
    }

    pub fn register<T: Default + View + 'static>(&mut self, name: &str) {
        self.types.insert(name.to_owned(), || Rc::new(RefCell::from(T::default())));
    }

    pub fn create(&self, name: &str) -> Element {
        self.types.get(name).expect("No type!")()
    }

    pub fn on_start(&mut self, func: Box<dyn FnMut(&mut UI)>) {
        self.on_start = Some(func);
    }

    /// Register a handler for OS file drops onto this window. The handler
    /// returns `true` to request a redraw.
    pub fn set_on_file_drop(&mut self, func: impl FnMut(&mut UI, std::path::PathBuf) -> bool + 'static) {
        self.on_file_drop = Some(Box::new(func));
    }

    /// Register a hook invoked on every left-button release, before the event
    /// is dispatched to views (which happens regardless of the hook's result).
    /// Return true to request a redraw. Use for press-and-hold interactions
    /// that must observe the release even outside the pressed view.
    pub fn set_on_mouse_up(&mut self, func: impl FnMut(&mut UI, Point<i32>) -> bool + 'static) {
        self.on_mouse_up = Some(Box::new(func));
    }

    /// Dispatch one dropped file to the registered handler (called by the
    /// window loop). Take-call-restore so the handler can use `&mut UI`.
    pub fn on_file_dropped(&mut self, path: std::path::PathBuf) -> bool {
        if let Some(mut f) = self.on_file_drop.take() {
            let redraw = f(self, path);
            if self.on_file_drop.is_none() {
                self.on_file_drop = Some(f);
            }
            redraw
        } else {
            false
        }
    }

    /// Returns a `Send + Sync + Clone` handle for posting closures to this
    /// UI's thread from workers ([`UiHandle::run_on_ui_thread`]). Obtain it
    /// before `run_loop` and clone it freely.
    pub fn handle(&self) -> UiHandle {
        UiHandle { tasks: Arc::clone(&self.tasks) }
    }

    /// Registers a closure that runs once when this UI is dropped — for the
    /// main window that means its event loop ended. Use it for app shutdown
    /// work; child/dialog UIs normally don't set it.
    ///
    /// Note: if the window was created with `with_hide_on_close` (or the
    /// handler's close-hides policy), the X button / Escape only *hide* the
    /// window and do not drop the UI, so this fires only on a real terminate
    /// (e.g. a tray "Quit" → [`UI::request_quit`]).
    pub fn set_on_close(&mut self, func: impl FnOnce(&mut UI) + 'static) {
        self.on_close = Some(Box::new(func));
    }

    /// Last known window geometry: outer position + inner size (both frozen
    /// while the window is maximized) and the maximized flag. Kept current by
    /// the window loop; use from `set_on_close` to persist window state.
    pub fn window_state(&self) -> (i32, i32, u32, u32, bool) {
        (
            self.window_pos.0,
            self.window_pos.1,
            self.window_normal_size.0,
            self.window_normal_size.1,
            self.window_maximized,
        )
    }

    /// Whether this UI's window currently has input focus (kept current by
    /// the window loop). Lets apps suppress notifications for the open chat.
    pub fn window_focused(&self) -> bool {
        self.window_focused
    }

    /// Window-loop plumbing for [`UI::window_focused`] — not for app code.
    #[doc(hidden)]
    pub fn set_window_focused(&mut self, focused: bool) {
        self.window_focused = focused;
    }

    /// Window-loop plumbing for [`UI::window_state`] — not for app code.
    #[doc(hidden)]
    pub fn update_window_geometry(
        &mut self,
        pos: Option<(i32, i32)>,
        size: Option<(u32, u32)>,
        maximized: bool,
    ) {
        self.window_maximized = maximized;
        if !maximized {
            if let Some(p) = pos {
                self.window_pos = p;
            }
            if let Some(s) = size {
                self.window_normal_size = s;
            }
        }
    }

    pub fn layout(&mut self, width: u32, height: u32, scale: f64) {
        self.width = width;
        self.height = height;
        self.scale = scale;
        let root = self.root.clone();
        if let Some(root) = root {
            root.borrow_mut().layout_content(0, 0, width as i32, height as i32, &self.typeface.clone(), scale);
        }
        // Transparent overlays (e.g. notification stack) cover the whole window —
        // resize them with the window so newly-added items lay out into the right slot.
        let typeface = self.typeface.clone();
        let entries: Vec<(Element, PopupMode)> = self.overlays.iter()
            .map(|e| (Rc::clone(&e.element), e.mode))
            .collect();
        for (el, mode) in entries {
            if mode == PopupMode::Transparent {
                el.borrow_mut().layout_content(0, 0, width as i32, height as i32, &typeface, scale);
            }
        }
    }

    pub fn relayout(&mut self) {
        self.needs_relayout = true;
    }

    /// Run the relayout synchronously, immediately. Useful when subsequent
    /// code in the same callback needs to read post-layout state (e.g. a
    /// RecyclerView's updated `max_scroll` after inserting an item).
    pub fn force_layout(&mut self) {
        self.needs_relayout = false;
        self.do_relayout();
    }

    fn do_relayout(&mut self) {
        let root = self.root.clone();
        if let Some(root) = root {
            root.borrow_mut().layout_content(0, 0, self.width as i32, self.height as i32, &self.typeface.clone(), self.scale);
        }
    }

    /// Shows a popup at the given anchor point, expanding in the given direction.
    pub fn show_popup(&mut self, popup: Element, x: i32, y: i32, direction: PopupDirection, mode: PopupMode) {
        // Layout the popup to determine its size. In external-popups mode the popup lives in its
        // own surface and may be larger than this window (a menu on a thin taskbar), so measure
        // against a generous bound instead of the window.
        let typeface = self.typeface.clone();
        let w = self.width as i32;
        let h = self.height as i32;
        let (bw, bh) = if self.external_popups { (8192, 8192) } else { (w, h) };
        popup.borrow_mut().layout_content(0, 0, bw, bh, &typeface, self.scale);

        let rect = popup.borrow().get_rect();
        let pw = rect.width();
        let ph = rect.height();

        // Compute origin based on direction
        let (mut ox, mut oy) = match direction {
            PopupDirection::BottomRight => (x, y),
            PopupDirection::BottomLeft => (x - pw, y),
            PopupDirection::TopRight => (x, y - ph),
            PopupDirection::TopLeft => (x - pw, y - ph),
            PopupDirection::Center => (x - pw / 2, y - ph / 2),
        };

        // Clamp to window bounds — unless the embedder presents popups in its own surfaces
        // (external mode), where extending past the window is exactly the intent and any
        // screen-edge clamping is the host's job.
        if !self.external_popups {
            ox = ox.max(0).min(w - pw);
            oy = oy.max(0).min(h - ph);
        }

        let token = self.next_overlay_token;
        self.next_overlay_token += 1;
        self.overlays.push(PopupEntry {
            element: popup,
            x: ox,
            y: oy,
            mode,
            token,
        });
    }

    /// Switch the UI into "external popups" mode: an embedder that composites windows itself
    /// (rather than winit) presents every non-`Transparent` overlay and the tooltip in its own
    /// surface. In this mode `paint()` draws only the root and `Transparent` overlays (e.g. the
    /// notification stack), and popup/tooltip positions are not clamped to the window — they
    /// may extend past it. Enumerate the overlays each frame via [`UI::overlay_snapshot`] /
    /// [`UI::tooltip_snapshot`], paint them with
    /// `crate::render::render_overlay_to_pixmap` (software builds), and feed their input back
    /// through window-space coordinates (surface-local + the overlay's `(x, y)`).
    pub fn set_external_popups(&mut self, external: bool) {
        self.external_popups = external;
    }

    /// The non-`Transparent` overlays (popups, menus, modals), bottom to top, for an
    /// external-popups embedder: window-space rects with stable tokens. The tooltip is separate
    /// ([`UI::tooltip_snapshot`]).
    pub fn overlay_snapshot(&self) -> Vec<OverlayDesc> {
        self.overlays
            .iter()
            .filter(|e| e.mode != PopupMode::Transparent)
            .map(|e| {
                let r = e.element.borrow().get_rect();
                OverlayDesc {
                    token: e.token,
                    x: e.x + r.min.x,
                    y: e.y + r.min.y,
                    width: r.width(),
                    height: r.height(),
                }
            })
            .collect()
    }

    /// The tooltip's window-space rect for an external-popups embedder (token =
    /// [`TOOLTIP_TOKEN`]), or `None` when no tooltip is showing.
    pub fn tooltip_snapshot(&self) -> Option<OverlayDesc> {
        self.tooltip_popup.as_ref().map(|t| {
            let r = t.element.borrow().get_rect();
            OverlayDesc {
                token: TOOLTIP_TOKEN,
                x: t.x + r.min.x,
                y: t.y + r.min.y,
                width: r.width(),
                height: r.height(),
            }
        })
    }

    /// The window-space top-left of a live overlay's rect by token ([`TOOLTIP_TOKEN`] for the
    /// tooltip) — for translating surface-local input back into window space. `None` if the
    /// overlay is gone.
    pub fn overlay_origin(&self, token: u64) -> Option<(i32, i32)> {
        if token == TOOLTIP_TOKEN {
            return self.tooltip_snapshot().map(|d| (d.x, d.y));
        }
        self.overlays.iter().find(|e| e.token == token).map(|e| {
            let r = e.element.borrow().get_rect();
            (e.x + r.min.x, e.y + r.min.y)
        })
    }

    /// Paint one overlay (or the tooltip, token = [`TOOLTIP_TOKEN`]) with its rect's top-left
    /// at the theme's origin — for an external-popups embedder rendering it into its own
    /// surface. `false` if the token is gone.
    pub fn paint_overlay(&self, token: u64, theme: &mut dyn Theme) -> bool {
        if token == TOOLTIP_TOKEN {
            if let Some(t) = &self.tooltip_popup {
                let r = t.element.borrow().get_rect();
                t.element.borrow().paint(Point::from((-r.min.x, -r.min.y)), theme);
                return true;
            }
            return false;
        }
        if let Some(e) = self.overlays.iter().find(|e| e.token == token) {
            let r = e.element.borrow().get_rect();
            e.element.borrow().paint(Point::from((-r.min.x, -r.min.y)), theme);
            return true;
        }
        false
    }

    /// Closes a popup by its view ID.
    pub fn close_popup(&mut self, id: &str) {
        self.overlays.retain(|entry| entry.element.borrow().get_id() != id);
    }

    /// Returns true if a popup overlay with this view ID is currently shown.
    pub fn is_popup_open(&self, id: &str) -> bool {
        !id.is_empty() && self.overlays.iter().any(|e| e.element.borrow().get_id() == id)
    }

    /// Returns true if this exact element is currently shown as an overlay.
    pub(crate) fn overlay_exists(&self, element: &Element) -> bool {
        self.overlays.iter().any(|e| Rc::ptr_eq(&e.element, element))
    }

    /// Removes this exact element from the overlays (pointer identity, so
    /// safe even when several popups share an empty ID).
    pub(crate) fn remove_overlay(&mut self, element: &Element) {
        self.overlays.retain(|e| !Rc::ptr_eq(&e.element, element));
    }

    /// Finds the overlay entry holding exactly this view instance (pointer
    /// identity), returning its element and window position. Lets a popup
    /// locate itself — overlays are not part of the root tree, so a view
    /// shown as a popup cannot learn its window position via parent pointers.
    pub(crate) fn find_self_overlay(&self, view: &dyn View) -> Option<(Element, i32, i32)> {
        let target = view.as_any() as *const dyn std::any::Any as *const ();
        self.overlays.iter()
            .find(|e| {
                let b = e.element.borrow();
                std::ptr::eq(b.as_any() as *const dyn std::any::Any as *const (), target)
            })
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
    }

    /// Closes all `Popup`-mode overlays. `Modal` and `Transparent` overlays are preserved.
    pub fn close_all_popups(&mut self) {
        self.overlays.retain(|entry| entry.mode != PopupMode::Popup);
    }

    /// Returns true if there are any active popups/overlays.
    pub fn has_popups(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// Returns true if any overlay would be dismissed by Escape or by a click
    /// outside it (`PopupMode::Popup`). Transparent overlays (notification
    /// stack) and modal dialogs do not count.
    pub fn has_dismissable_popups(&self) -> bool {
        self.overlays.iter().any(|e| e.mode == PopupMode::Popup)
    }

    /// Centralized Escape-key policy for the window backends. Call on an
    /// *unconsumed* Escape key press (the caller already checked it wasn't
    /// consumed by a view/dialog, and on winit that it isn't a key repeat).
    /// Dismisses popups immediately and reports what the window should do via
    /// [`EscapeAction`]. Escape never closes or quits the main window — that's
    /// left to the app's own handler/shortcut.
    pub fn escape_press_action(&mut self, is_child: bool) -> EscapeAction {
        if self.has_dismissable_popups() {
            self.close_all_popups();
            EscapeAction::DismissedPopups
        } else if is_child {
            EscapeAction::CloseChildWindow
        } else {
            EscapeAction::None
        }
    }

    /// Lazily ensures the notification stack overlay exists, returning a clone of its Element.
    fn ensure_notification_stack(&mut self) -> Element {
        if let Some(el) = &self.notification_stack {
            return Rc::clone(el);
        }
        let stack: Element = Rc::new(RefCell::new(NotificationStack::new()));
        // Lay out at full window size so it can place items at absolute coords.
        let typeface = self.typeface.clone();
        stack.borrow_mut().layout_content(0, 0, self.width as i32, self.height as i32, &typeface, self.scale);
        let token = self.next_overlay_token;
        self.next_overlay_token += 1;
        self.overlays.push(PopupEntry {
            element: Rc::clone(&stack),
            x: 0,
            y: 0,
            mode: PopupMode::Transparent,
            token,
        });
        self.notification_stack = Some(Rc::clone(&stack));
        stack
    }

    /// Push a notification view onto the stack with the given id and optional auto-dismiss timeout.
    /// If `id` already exists it is replaced. Clicking a child of `element` that calls
    /// `dismiss_notification(id)` (or `dismiss_notification_for(view)`) removes it.
    pub fn show_notification(&mut self, element: Element, id: &str, timeout: Option<Duration>) {
        let stack = self.ensure_notification_stack();
        let typeface = self.typeface.clone();
        let scale = self.scale;
        let s = stack.borrow();
        if let Some(stack_ref) = s.as_any().downcast_ref::<NotificationStack>() {
            stack_ref.push_item(id, element, timeout, &typeface, scale);
        }
    }

    /// Animate an item out and remove it.
    pub fn dismiss_notification(&mut self, id: &str) {
        if let Some(stack) = &self.notification_stack {
            if let Some(s) = stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                s.dismiss(id);
            }
        }
    }

    /// Remove an item without animation.
    pub fn dismiss_notification_immediate(&mut self, id: &str) {
        if let Some(stack) = &self.notification_stack {
            if let Some(s) = stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                s.dismiss_immediate(id);
            }
        }
    }

    /// Animate every active notification out of the stack.
    pub fn dismiss_all_notifications(&mut self) {
        if let Some(stack) = &self.notification_stack {
            if let Some(s) = stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                s.dismiss_all();
            }
        }
    }

    pub fn has_notification(&self, id: &str) -> bool {
        match &self.notification_stack {
            Some(stack) => stack.borrow()
                .as_any()
                .downcast_ref::<NotificationStack>()
                .map(|s| s.has(id))
                .unwrap_or(false),
            None => false,
        }
    }

    /// Walks `view`'s parent chain looking for an id that matches a current
    /// notification, then dismisses it. Convenient inside close-button callbacks
    /// so the caller doesn't need to capture the id by closure.
    pub fn dismiss_notification_for(&mut self, view: &dyn View) -> bool {
        let id = match &self.notification_stack {
            Some(stack) => match stack.borrow().as_any().downcast_ref::<NotificationStack>() {
                Some(s) => s.id_for_descendant(view),
                None => None,
            },
            None => None,
        };
        if let Some(id) = id {
            self.dismiss_notification(&id);
            true
        } else {
            false
        }
    }

    pub fn start(&mut self) {
        if let Some(mut start) = self.on_start.take() {
            start(self);
        }
    }

    pub fn update(&mut self) -> bool {
        let mut redraw = false;
        // Run tasks posted from other threads via UiHandle. Drain under the
        // lock, run outside it: a task (or a worker racing us) may queue more
        // tasks; those run on the next tick.
        let tasks: Vec<UiTask> = {
            let mut queue = self.tasks.lock().unwrap();
            queue.drain(..).collect()
        };
        if !tasks.is_empty() {
            redraw = true;
        }
        for task in tasks {
            task(self);
        }
        // Process queued removals first; they may flip needs_relayout.
        if self.process_pending_removals() {
            redraw = true;
        }
        // Perform deferred relayout if requested
        if self.needs_relayout {
            self.needs_relayout = false;
            self.do_relayout();
            redraw = true;
        }
        // Update overlays first
        let overlays: Vec<Element> = self.overlays.iter().map(|e| Rc::clone(&e.element)).collect();
        for element in overlays {
            redraw |= element.borrow_mut().update(self);
        }
        // Then update root
        let root = self.root.clone();
        if let Some(root) = root {
            redraw |= root.borrow_mut().update(self);
        }
        // Fire events deferred from inside the tree walk above (a view's
        // update() runs under its own borrow_mut, so handlers — which may
        // call get_view — could not run there). All borrows are free here.
        // Handlers may defer more events; those fire on the next pass.
        while !self.deferred_events.is_empty() {
            let deferred = std::mem::take(&mut self.deferred_events);
            for (id, event, data) in deferred {
                if let Some(element) = self.get_view(&id) {
                    redraw |= element.borrow().fire_event(self, event, &data);
                }
            }
        }
        // Catch focus changes made programmatically or via Frame::focus_next/prev
        // (paths that never pass through an input dispatch).
        redraw |= self.sync_focus();
        // Update tooltip
        redraw |= self.update_tooltip();
        redraw
    }

    /// Queues an event to fire after the current `update()` tree-walk, once
    /// the firing view's `borrow_mut` is released. Use this instead of
    /// `fire_event` when dispatching from inside `View::update` — handlers
    /// are then free to call `get_view` on any view. The view is resolved by
    /// id at fire time (views without an id cannot use this).
    pub fn defer_event(&mut self, view_id: &str, event: EventType, data: EventData) {
        if !view_id.is_empty() {
            self.deferred_events.push((view_id.to_owned(), event, data));
        }
    }

    pub fn paint(&self, theme: &mut dyn Theme) {
        theme.clear_screen();
        if let Some(root) = &self.root {
            root.borrow().paint(Point::from((0, 0)), theme);
        }
        // Paint overlays on top, in order (last = topmost). In external-popups mode the
        // embedder paints non-Transparent overlays and the tooltip into its own surfaces
        // (paint_overlay); only Transparent overlays (notification stack) stay in-window.
        for entry in &self.overlays {
            if self.external_popups && entry.mode != PopupMode::Transparent {
                continue;
            }
            entry.element.borrow().paint(Point::from((entry.x, entry.y)), theme);
        }
        // Paint tooltip on top of everything
        if !self.external_popups
            && let Some(tooltip) = &self.tooltip_popup
        {
            tooltip.element.borrow().paint(Point::from((tooltip.x, tooltip.y)), theme);
        }
    }

    pub fn from_xml(xml: &str, width: u32, height: u32, typeface: Typeface, scale: f64) -> Option<Self> {
        let mut ui = UI::new(width, height, typeface, scale);
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut txt = Vec::new();
        let mut stack: Vec<Element> = Vec::new();

        // TODO extract parsing views into self contained method
        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let element = UI::parse_element(&mut ui, e);
                    if element.borrow().wants_raw_content() {
                        // Capture the literal inner markup (incl. inline tags like
                        // <b>…</b>) and hand it to the view instead of parsing the
                        // children as nested views. `read_text` returns the inner
                        // slice verbatim and consumes the matching end tag, so this
                        // element gets no `Event::End` — attach it as a leaf here.
                        let inner = reader
                            .read_text(e.name())
                            .ok()
                            .and_then(|t| t.decode().ok().map(|c| c.into_owned()))
                            .unwrap_or_default();
                        element.borrow_mut().set_any("html", &inner);
                        match stack.last() {
                            Some(parent) => {
                                element.borrow_mut().set_parent(Some(Rc::downgrade(parent)));
                                let mut ref_mut = parent.borrow_mut();
                                if let Some(container) = ref_mut.as_container_mut() {
                                    container.add_view(element);
                                }
                            }
                            None => ui.add_view(element),
                        }
                    } else {
                        stack.push(element);
                    }
                },
                Ok(Event::Empty(ref e)) => {
                    let tag_name = String::from_utf8(e.name().0.to_vec()).unwrap();
                    if tag_name == "Style" {
                        // <Style name="..." attr=.../> registers an attribute
                        // bundle; must be self-closing and precede its users.
                        ui.parse_style(e);
                    } else if tag_name == "Item" {
                        // Handle <Item text="..."/> inside ComboBox
                        if let Some(parent) = stack.last() {
                            let text = UI::get_attribute(e, "text").unwrap_or_default();
                            let mut ref_mut = parent.borrow_mut();
                            if let Some(combo) = ref_mut.as_any_mut().downcast_mut::<ComboBox>() {
                                combo.add_item(&text);
                            }
                        }
                    } else {
                        let element = UI::parse_element(&mut ui, e);
                        let parent = stack.pop().unwrap();
                        {
                            element.borrow_mut().set_parent(Some(Rc::downgrade(&parent)));
                            let mut ref_mut = parent.borrow_mut();
                            let container = ref_mut.as_container_mut().unwrap();
                            container.add_view(element);
                        }
                        stack.push(parent);
                    }
                },
                Ok(Event::End(_)) => {
                    // TODO check that it is the same tag
                    let element = stack.pop().unwrap();
                    match stack.pop() {
                        None => {
                            ui.add_view(element);
                        }
                        Some(parent) => {
                            {
                                element.borrow_mut().set_parent(Some(Rc::downgrade(&parent)));
                                let mut ref_mut = parent.borrow_mut();
                                let container = ref_mut.as_container_mut().unwrap();
                                container.add_view(element);
                            }
                            stack.push(parent);
                        }
                    }
                },
                // unescape and decode the text event using the reader encoding
                Ok(Event::Text(e)) => txt.push(e.into_inner().into_owned()),
                Ok(Event::Eof) => break, // exits the loop when reaching end of file
                Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
                _ => (), // There are several other `Event`s we do not consider here
            }
        }
        Some(ui)
    }

    fn parse_element(ui: &mut UI, e: &BytesStart) -> Element {
        let attributes: Vec<(String, String)> = e
            .attributes()
            .map(|a| a.unwrap())
            .map(|a| {
                let name = String::from_utf8(a.key.0.to_vec()).unwrap();
                // Unescape XML entities (&quot;, &amp;, &lt;, ...) in values.
                let value = match a.normalized_value(XmlVersion::Implicit1_0) {
                    Ok(value) => value.into_owned(),
                    Err(_) => match a.value {
                        Cow::Borrowed(c) => String::from_utf8(c.to_vec()).unwrap(),
                        Cow::Owned(c) => String::from_utf8(c).unwrap(),
                    },
                };
                (name, value)
            })
            .collect();
        let view_type = String::from_utf8(e.name().0.to_vec()).unwrap();
        let view = ui.create(&view_type);
        // Apply the style bundle (if any) first, so the element's own
        // attributes override what the style sets.
        if let Some((_, style_name)) = attributes.iter().find(|(name, _)| name == "style") {
            match ui.styles.get(style_name) {
                Some(bundle) => {
                    for (name, value) in bundle {
                        view.borrow_mut().set_any(name, value);
                    }
                }
                None => eprintln!("Unknown style '{}' on <{}>", style_name, view_type),
            }
        }
        for (name, value) in &attributes {
            if name == "style" {
                continue;
            }
            view.borrow_mut().set_any(name, value);
        }
        view
    }

    /// Register the attribute bundle of a `<Style name="..." .../>` element.
    fn parse_style(&mut self, e: &BytesStart) {
        let Some(name) = UI::get_attribute(e, "name") else {
            eprintln!("<Style> element without a name attribute, ignored");
            return;
        };
        let mut bundle = Vec::new();
        for attr in e.attributes().flatten() {
            let key = String::from_utf8(attr.key.0.to_vec()).unwrap();
            if key == "name" {
                continue;
            }
            // Unescape XML entities (&quot;, &amp;, &lt;, ...) in values.
            let value = match attr.normalized_value(XmlVersion::Implicit1_0) {
                Ok(value) => value.into_owned(),
                Err(_) => match attr.value {
                    Cow::Borrowed(c) => String::from_utf8(c.to_vec()).unwrap(),
                    Cow::Owned(c) => String::from_utf8(c).unwrap(),
                },
            };
            bundle.push((key, value));
        }
        self.styles.insert(name, bundle);
    }

    /// Register an attribute bundle usable via `style="name"` in layout XML
    /// parsed afterwards (e.g. item layouts inflated at runtime).
    pub fn add_style(&mut self, name: &str, attributes: &[(&str, &str)]) {
        let bundle = attributes.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.styles.insert(name.to_string(), bundle);
    }

    fn get_attribute(e: &BytesStart, name: &str) -> Option<String> {
        for attr in e.attributes().flatten() {
            let key = String::from_utf8(attr.key.0.to_vec()).unwrap();
            if key == name {
                // Unescape XML entities (&quot;, &amp;, &lt;, ...) in values.
                return Some(match attr.normalized_value(XmlVersion::Implicit1_0) {
                    Ok(value) => value.into_owned(),
                    Err(_) => match attr.value {
                        Cow::Borrowed(c) => String::from_utf8(c.to_vec()).unwrap(),
                        Cow::Owned(c) => String::from_utf8(c.to_vec()).unwrap(),
                    },
                });
            }
        }
        None
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    /// Returns the current mouse position in absolute window coordinates.
    pub fn get_mouse_pos(&self) -> Point<i32> {
        self.mouse_pos
    }

    /// Returns true if any overlay is modal.
    fn has_modal_overlay(&self) -> bool {
        self.overlays.iter().any(|e| e.mode == PopupMode::Modal)
    }

    /// Detects focus changes since the last sweep and fires `FocusLost` /
    /// `FocusGained` on the affected views. Focus is mutated from many places
    /// without `&mut UI` access (mouse clicks, `Frame::focus_next/prev`,
    /// programmatic `set_focused`), so instead of threading UI through all of
    /// them the change is observed here, after each input dispatch and on the
    /// update tick. Only leaf views set their own `state.focused` (a `Frame`
    /// reports focus computed from its children, never its own state), so the
    /// sweep finds exactly the focused leaf. Returns true if focus changed.
    fn sync_focus(&mut self) -> bool {
        let focused = self.find_with(&|v| v.get_state().map(|s| s.focused).unwrap_or(false));
        let new_id = focused.first().map(|el| el.borrow().get_id());
        if new_id == self.focus_owner {
            return false;
        }
        // Update the owner BEFORE firing so a handler that re-focuses
        // converges on the next sweep instead of recursing.
        let old = std::mem::replace(&mut self.focus_owner, new_id);
        if let Some(old_id) = old {
            // The view may have been removed since it had focus — skip then.
            let el = self.get_view(&old_id);
            if let Some(el) = el {
                el.borrow().fire_event(self, EventType::FocusLost, &EventData::None);
            }
        }
        if let Some(el) = focused.into_iter().next() {
            el.borrow().fire_event(self, EventType::FocusGained, &EventData::None);
        }
        true
    }

    /// Walks the view tree to find the deepest visible view under (x, y) that
    /// matches `pred`. `x`/`y` are absolute window coordinates; child rects are
    /// parent-relative, so the parent's absolute origin accumulates through
    /// `offset_x`/`offset_y`. Children are visited in reverse order (topmost
    /// first), matching the mouse dispatch order.
    fn hit_test_element(element: &Element, x: i32, y: i32, offset_x: i32, offset_y: i32,
                        pred: &dyn Fn(&dyn View) -> bool) -> Option<Element> {
        let view = element.borrow();
        if view.get_visibility() != Visibility::Visible {
            return None;
        }
        let rect = view.get_rect();
        let abs_x = rect.min.x + offset_x;
        let abs_y = rect.min.y + offset_y;
        if x < abs_x || x >= rect.max.x + offset_x || y < abs_y || y >= rect.max.y + offset_y {
            return None;
        }

        // Check children first (deepest match wins). Uses `hit_test_views` so
        // containers like `TabView` that only show a subset of their children
        // (the active tab) are not matched on inactive, off-screen children.
        if let Some(container) = view.as_container() {
            for child in container.hit_test_views().iter().rev() {
                if let Some(found) = Self::hit_test_element(child, x, y, abs_x, abs_y, pred) {
                    return Some(found);
                }
            }
        }

        if pred(&*view) {
            return Some(Rc::clone(element));
        }
        None
    }

    /// Hit test honoring overlay semantics: the topmost non-Transparent
    /// overlay containing the point confines the search to itself; Transparent
    /// overlays (e.g. the notification stack) are searched but fall through
    /// when nothing matches; when a non-Transparent overlay exists and the
    /// point is in none of them, the root is NOT searched (the click would
    /// dismiss the popup / be blocked by the modal).
    fn hit_test_listener(&self, x: i32, y: i32, pred: &dyn Fn(&dyn View) -> bool) -> Option<Element> {
        let mut blocked = false;
        for entry in self.overlays.iter().rev() {
            let found = Self::hit_test_element(&entry.element, x, y, entry.x, entry.y, pred);
            if found.is_some() {
                return found;
            }
            if entry.mode != PopupMode::Transparent {
                blocked = true;
                // Point inside this overlay but no match — confined, stop.
                let rect = entry.element.borrow().get_rect();
                if x >= rect.min.x + entry.x && x < rect.max.x + entry.x
                    && y >= rect.min.y + entry.y && y < rect.max.y + entry.y {
                    return None;
                }
            }
        }
        if blocked {
            return None;
        }
        self.root.as_ref().and_then(|root| Self::hit_test_element(root, x, y, 0, 0, pred))
    }

    fn update_tooltip(&mut self) -> bool {
        // Don't show tooltips when blocking popups are open. Transparent overlays
        // (e.g. notification stack) let tooltips on the underlying UI keep working.
        if self.overlays.iter().any(|e| e.mode != PopupMode::Transparent) {
            return self.dismiss_tooltip();
        }

        let hit = self.root.as_ref().and_then(|root| {
            Self::hit_test_element(root, self.mouse_pos.x, self.mouse_pos.y, 0, 0,
                &|v| v.get_tooltip().map(|t| !t.is_empty()).unwrap_or(false))
        }).map(|el| {
            let view = el.borrow();
            (view.get_id(), view.get_tooltip().unwrap_or_default())
        });

        match hit {
            Some((view_id, tooltip_text)) => {
                if self.tooltip_view_id.as_deref() == Some(&view_id) {
                    // Same view — check if it's time to show
                    if !self.tooltip_showing {
                        if let Some(start) = &self.tooltip_hover_start {
                            if start.elapsed().as_millis() >= TOOLTIP_DELAY_MS {
                                self.show_tooltip(&tooltip_text);
                                return true;
                            }
                        }
                    }
                    false
                } else {
                    // Different view — reset timer
                    let dismissed = self.dismiss_tooltip();
                    self.tooltip_view_id = Some(view_id);
                    self.tooltip_hover_start = Some(Instant::now());
                    dismissed
                }
            }
            None => {
                // No tooltip view under cursor
                self.dismiss_tooltip()
            }
        }
    }

    fn show_tooltip(&mut self, text: &str) {
        let label: Element = Rc::new(RefCell::new(Label::default()));
        {
            let mut l = label.borrow_mut();
            l.set_any("text", text);
            l.set_width(Dimension::Min);
            l.set_height(Dimension::Min);
            l.set_any("font_size", "18");
            l.set_any("text_color", &format!("#{:08X}", crate::drawing::current_color("tooltip_text")));
        }

        let frame: Element = Rc::new(RefCell::new(Frame::default()));
        {
            let mut f = frame.borrow_mut();
            f.set_id(TOOLTIP_ID);
            f.set_width(Dimension::Min);
            f.set_height(Dimension::Min);
            f.set_padding(2, 4, 4, 2);
            f.set_background(Some(crate::drawing::current_color("tooltip_back")));
            f.set_border_color(Some(crate::drawing::current_color("tooltip_border")));
            label.borrow_mut().set_parent(Some(Rc::downgrade(&frame)));
            f.as_container_mut().unwrap().add_view(label);
        }

        // Layout to determine size. Like popups, a tooltip in external mode lives in its own
        // surface and must not be size-bounded by a small window (e.g. a thin taskbar).
        let typeface = self.typeface.clone();
        let w = self.width as i32;
        let h = self.height as i32;
        let (bw, bh) = if self.external_popups { (8192, 8192) } else { (w, h) };
        frame.borrow_mut().layout_content(0, 0, bw, bh, &typeface, self.scale);

        let rect = frame.borrow().get_rect();
        let pw = rect.width();
        let ph = rect.height();

        // Position below and to the right of the cursor
        let mut ox = self.mouse_pos.x;
        let mut oy = self.mouse_pos.y + (self.scale * 15f64).round() as i32;

        // Clamp to window bounds — not in external-popups mode, where the tooltip lives in its
        // own surface and may extend past the window (the host clamps to the screen).
        if !self.external_popups {
            ox = ox.max(0).min(w - pw);
            oy = oy.max(0).min(h - ph);
        }

        self.tooltip_popup = Some(TooltipPopup { element: frame, x: ox, y: oy });
        self.tooltip_showing = true;
    }

    fn dismiss_tooltip(&mut self) -> bool {
        if self.tooltip_showing {
            self.tooltip_popup = None;
            self.tooltip_showing = false;
            self.tooltip_view_id = None;
            self.tooltip_hover_start = None;
            true
        } else if self.tooltip_view_id.is_some() {
            // Had a pending tooltip but hadn't shown yet
            self.tooltip_view_id = None;
            self.tooltip_hover_start = None;
            false
        } else {
            false
        }
    }

    pub fn on_mouse_move(&mut self, position: Point<i32>) -> bool {
        self.mouse_pos = position;
        // Re-evaluate the cursor from scratch each move: views over a link
        // re-request `Pointer` during dispatch; anything left is the default.
        self.requested_cursor = None;
        let mut redraw = self.dispatch_mouse_move(position);
        redraw |= self.sync_hover();
        redraw
    }

    fn dispatch_mouse_move(&mut self, position: Point<i32>) -> bool {
        // Dispatch to overlays first (reverse order = topmost first)
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Point::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_move(self, local) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        // A non-transparent overlay (e.g. a popup menu) under the pointer
        // visually covers the views beneath it, so the move must not reach the
        // root — otherwise an Edit below the popup would request the I-beam
        // cursor. Mirrors the confinement in `hit_test_listener`.
        if self.pointer_over_opaque_overlay(position) {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_move(self, position),
        }
    }

    /// True when the pointer lies inside a non-`Transparent` overlay. Such an
    /// overlay covers the views beneath it, so coordinate-based input must not
    /// fall through to the root there (used to confine cursor selection;
    /// `hit_test_listener` applies the same rule for hit testing).
    fn pointer_over_opaque_overlay(&self, position: Point<i32>) -> bool {
        self.overlays.iter().any(|entry| {
            if entry.mode == PopupMode::Transparent {
                return false;
            }
            let rect = entry.element.borrow().get_rect();
            position.x >= rect.min.x + entry.x && position.x < rect.max.x + entry.x
                && position.y >= rect.min.y + entry.y && position.y < rect.max.y + entry.y
        })
    }

    /// Re-evaluates the cursor for `position` when a mouse-button event changed
    /// the overlay set. Opening or closing a popup changes which view sits
    /// under the pointer but generates no mouse move, so without this the OS
    /// cursor lingers until the next move — e.g. the I-beam staying visible
    /// when a context menu opens over an `Edit`. Returns whether a redraw is
    /// needed.
    fn refresh_cursor_if_overlays_changed(&mut self, position: Point<i32>, overlays_before: usize) -> bool {
        if self.overlays.len() == overlays_before {
            return false;
        }
        self.requested_cursor = None;
        // The overlay set changed, so the painted scene changed — a redraw is
        // always needed. Over a covering overlay the cursor is the default arrow
        // and no move dispatch is required; elsewhere re-evaluate the cursor from
        // the views now under the pointer (which may itself request a redraw).
        if !(self.has_modal_overlay() || self.pointer_over_opaque_overlay(position)) {
            self.dispatch_mouse_move(position);
        }
        true
    }

    /// Detects which view with a hover listener is under the cursor and fires
    /// `HoverExit` / `HoverEnter` on ownership changes. Tracked centrally
    /// because `Frame` dispatches moves to all children and views update their
    /// visual `hovered` state at their own transition points — one tracker
    /// covers every view (including containers) with zero per-view code.
    /// Note: the mouse leaving the window fires no event (no leave
    /// notification from the window system), and a popup opening over the
    /// hovered view defers `HoverExit` to the next mouse move.
    fn sync_hover(&mut self) -> bool {
        let target = self.hit_test_listener(self.mouse_pos.x, self.mouse_pos.y,
            &|v| v.is_enabled() && (v.has_listener(EventType::HoverEnter) || v.has_listener(EventType::HoverExit)));
        let new_id = target.as_ref().map(|el| el.borrow().get_id());
        if new_id == self.hover_owner {
            return false;
        }
        let old = std::mem::replace(&mut self.hover_owner, new_id);
        let mut redraw = false;
        if let Some(old_id) = old {
            let el = self.get_view(&old_id);
            if let Some(el) = el {
                redraw |= el.borrow().fire_event(self, EventType::HoverExit, &EventData::None);
            }
        }
        if let Some(el) = target {
            let data = EventData::Position { x: self.mouse_pos.x, y: self.mouse_pos.y };
            redraw |= el.borrow().fire_event(self, EventType::HoverEnter, &data);
        }
        redraw
    }

    /// Requests a cursor shape for the current `on_mouse_move` dispatch. Called
    /// by a view (e.g. a hovered link) from its `on_mouse_move`. First write
    /// wins, so the topmost view under the pointer decides the cursor (dispatch
    /// visits views topmost-first).
    pub fn request_cursor(&mut self, cursor: MouseCursorType) {
        if self.requested_cursor.is_none() {
            self.requested_cursor = Some(cursor);
        }
    }

    /// The cursor shape resolved by the last `on_mouse_move`, falling back to
    /// the default arrow when no view requested one. Read by the window handler
    /// to drive the OS cursor.
    pub fn current_cursor(&self) -> MouseCursorType {
        self.requested_cursor.unwrap_or(MouseCursorType::Default)
    }

    /// Request a palette change (e.g. `Palette::dark()`). Applied by the
    /// window handler before the next paint, so it is safe to call from
    /// inside event handlers.
    pub fn set_palette(&mut self, palette: crate::drawing::Palette) {
        self.pending_palette = Some(palette);
    }

    /// Taken by the window handler each frame; `Some` means the app requested
    /// a palette change since the last paint.
    pub fn take_pending_palette(&mut self) -> Option<crate::drawing::Palette> {
        self.pending_palette.take()
    }

    /// Queues a new OS window with its own UI. Safe to call from event
    /// handlers; the window opens on the next update tick (within ~16 ms).
    pub fn open_window(&mut self, request: WindowRequest) {
        self.window_requests.push(request);
    }

    pub fn take_window_requests(&mut self) -> Vec<WindowRequest> {
        std::mem::take(&mut self.window_requests)
    }

    /// Requests closing this UI's window. Safe to call from event handlers.
    /// Closing the main window exits the application.
    pub fn close_window(&mut self) {
        self.close_requested = true;
    }

    pub fn take_close_request(&mut self) -> bool {
        std::mem::replace(&mut self.close_requested, false)
    }

    /// Requests showing (and focusing) this UI's window. Safe to call from any
    /// thread via [`UiHandle::run_on_ui_thread`]; the window handler shows the
    /// window on the next update tick. Intended for system-tray actions.
    pub fn request_show(&mut self) {
        self.pending_window_command = Some(WindowCommand::Show);
    }

    /// Requests hiding this UI's window (it keeps running, e.g. in the tray).
    pub fn request_hide(&mut self) {
        self.pending_window_command = Some(WindowCommand::Hide);
    }

    /// Requests terminating the app (closes the main window's event loop).
    pub fn request_quit(&mut self) {
        self.pending_window_command = Some(WindowCommand::Quit);
    }

    pub fn take_window_command(&mut self) -> Option<WindowCommand> {
        self.pending_window_command.take()
    }

    /// Opens a modal message dialog with a single "OK" button. Convenience
    /// wrapper over [`crate::dialog::Dialog`].
    pub fn show_message(&mut self, title: &str, message: &str) {
        crate::dialog::Dialog::new(title)
            .message(message)
            .button("OK")
            .default_button("OK")
            .cancel_button("OK")
            .show(self);
    }

    /// Opens a modal confirmation dialog with "OK" and "Cancel" buttons. The
    /// callback receives `true` when OK was pressed, `false` on Cancel/Esc.
    pub fn show_confirm(&mut self, title: &str, message: &str, mut on_result: impl FnMut(&mut UI, bool) + 'static) {
        crate::dialog::Dialog::new(title)
            .message(message)
            .button("OK")
            .button("Cancel")
            .default_button("OK")
            .cancel_button("Cancel")
            .on_result(move |ui, pressed| on_result(ui, pressed == "OK"))
            .show(self);
    }

    /// Opens a modal text-input dialog with "OK" and "Cancel" buttons. The
    /// callback receives `Some(text)` when OK was pressed, `None` on Cancel/Esc.
    pub fn show_input(&mut self, title: &str, prompt: &str, initial: &str, mut on_result: impl FnMut(&mut UI, Option<String>) + 'static) {
        const INPUT_ID: &str = "dialog_input";
        crate::dialog::Dialog::new(title)
            .message(prompt)
            .input(INPUT_ID, initial)
            .button("OK")
            .button("Cancel")
            .default_button("OK")
            .cancel_button("Cancel")
            .on_result(move |ui, pressed| {
                if pressed == "OK" {
                    let text = ui.get_view(INPUT_ID)
                        .and_then(|e| e.borrow().downcast_ref::<Edit>().map(|e| e.get_text()))
                        .unwrap_or_default();
                    on_result(ui, Some(text));
                } else {
                    on_result(ui, None);
                }
            })
            .show(self);
    }

    pub fn on_mouse_button_down(&mut self, position: Point<i32>, button: MouseButton) -> bool {
        self.dismiss_tooltip();
        // Double-click bookkeeping (left button only). The target is the
        // deepest view under the cursor with a DoubleClick listener; the
        // event fires only when both clicks land on the same target.
        let dc_target = self.hit_test_listener(position.x, position.y,
            &|v| v.is_enabled() && v.has_listener(EventType::DoubleClick))
            .map(|el| el.borrow().get_id());
        let is_double = matches!(button, MouseButton::Left)
            && dc_target.is_some()
            && self.last_click.as_ref().is_some_and(|(t, p, id)|
                t.elapsed().as_millis() < DOUBLE_CLICK_MS
                && (p.x - position.x).abs() <= DOUBLE_CLICK_DISTANCE
                && (p.y - position.y).abs() <= DOUBLE_CLICK_DISTANCE
                && *id == dc_target);
        // Reset after a double so a triple click fires exactly one event.
        self.last_click = if is_double || !matches!(button, MouseButton::Left) {
            None
        } else {
            Some((Instant::now(), position, dc_target.clone()))
        };
        // Snapshot before firing ContextMenu: a handler may open a popup right
        // here (not only during dispatch), and that overlay change must still be
        // seen by the redraw/cursor refresh below — otherwise a context menu
        // opened on right-click is not painted until the next input event.
        let overlays_before = self.overlays.len();
        // ContextMenu fires BEFORE dispatch so a consuming handler can
        // suppress the built-in menus that open during dispatch.
        if matches!(button, MouseButton::Right) {
            let target = self.hit_test_listener(position.x, position.y,
                &|v| v.is_enabled() && v.has_listener(EventType::ContextMenu));
            if let Some(el) = target {
                let data = EventData::Position { x: position.x, y: position.y };
                self.context_menu_suppressed = el.borrow().fire_event(self, EventType::ContextMenu, &data);
            }
        }
        let mut redraw = self.dispatch_mouse_button_down(position, button);
        self.context_menu_suppressed = false;
        if is_double {
            // Fire after dispatch: focus/press behavior is already applied.
            if let Some(id) = dc_target {
                let el = self.get_view(&id);
                if let Some(el) = el {
                    let data = EventData::Position { x: position.x, y: position.y };
                    redraw |= el.borrow().fire_event(self, EventType::DoubleClick, &data);
                }
            }
        }
        redraw |= self.sync_focus();
        redraw |= self.refresh_cursor_if_overlays_changed(position, overlays_before);
        redraw
    }

    /// True while dispatching a right-click whose `ContextMenu` listener
    /// consumed the event. Views with built-in context menus check this
    /// before opening them.
    pub fn context_menu_suppressed(&self) -> bool {
        self.context_menu_suppressed
    }

    /// Registers an application-wide keyboard accelerator from a string like
    /// `"Ctrl+Shift+S"`, `"F5"` or `"Alt+Enter"` (see [`Shortcut`]).
    /// Shortcuts fire only when the key-down was not consumed by the focused
    /// view or an overlay (local context wins over global accelerators), and
    /// never while a modal dialog is open. Re-registering the same shortcut
    /// replaces the handler. Prints an error and ignores unparsable strings.
    pub fn add_shortcut(&mut self, accel: &str, handler: Box<dyn FnMut(&mut UI) -> bool>) {
        match accel.parse::<Shortcut>() {
            Ok(shortcut) => self.add_shortcut_keys(shortcut, handler),
            Err(e) => eprintln!("Bad shortcut: {}", e),
        }
    }

    /// Registers an application-wide keyboard accelerator from a typed
    /// [`Shortcut`]. See [`UI::add_shortcut`] for dispatch semantics.
    pub fn add_shortcut_keys(&mut self, shortcut: Shortcut, handler: Box<dyn FnMut(&mut UI) -> bool>) {
        self.shortcuts.insert(shortcut, handler);
    }

    /// Removes the accelerator registered for the given string, if any.
    pub fn remove_shortcut(&mut self, accel: &str) {
        if let Ok(shortcut) = accel.parse::<Shortcut>() {
            self.shortcuts.remove(&shortcut);
        }
    }

    /// Fires the handler registered for this key/modifier combination. The
    /// handler runs with its entry taken out of the registry (so a shortcut
    /// cannot recursively fire itself) and is put back afterwards unless the
    /// handler registered a replacement.
    fn fire_shortcut(&mut self, code: VirtualKeyCode, modifiers: &ModifiersState) -> bool {
        let shortcut = Shortcut::from_state(code, modifiers);
        if let Some(mut handler) = self.shortcuts.remove(&shortcut) {
            let result = handler(self);
            self.shortcuts.entry(shortcut).or_insert(handler);
            return result;
        }
        false
    }

    fn dispatch_mouse_button_down(&mut self, position: Point<i32>, button: MouseButton) -> bool {
        // Dispatch to overlays first
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Point::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_button_down(self, local, button) {
                return true;
            }
        }
        // Click missed all overlays — dismiss Popup-mode overlays only.
        // Transparent overlays (e.g. notification stack) let the click fall through.
        // Skipped when a ContextMenu handler consumed this right-click, so a
        // menu the handler just opened is not immediately dismissed.
        if !self.context_menu_suppressed && self.overlays.iter().any(|e| e.mode == PopupMode::Popup) {
            self.close_all_popups();
            return true;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_button_down(self, position, button),
        }
    }

    pub fn on_mouse_button_up(&mut self, position: Point<i32>, button: MouseButton) -> bool {
        let mut redraw = false;
        if matches!(button, MouseButton::Left)
            && let Some(mut f) = self.on_mouse_up.take() {
                redraw = f(self, position);
                if self.on_mouse_up.is_none() {
                    self.on_mouse_up = Some(f);
                }
            }
        let overlays_before = self.overlays.len();
        redraw |= self.dispatch_mouse_button_up(position, button);
        redraw |= self.refresh_cursor_if_overlays_changed(position, overlays_before);
        redraw
    }

    fn dispatch_mouse_button_up(&mut self, position: Point<i32>, button: MouseButton) -> bool {
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Point::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_button_up(self, local, button) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_button_up(self, position, button),
        }
    }

    pub fn on_mouse_wheel_scroll(&mut self, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        let entries: Vec<(Element, i32, i32)> = self.overlays.iter().rev()
            .map(|e| (Rc::clone(&e.element), e.x, e.y))
            .collect();
        for (element, ox, oy) in &entries {
            let local = Point::new(position.x - ox, position.y - oy);
            if element.borrow().on_mouse_wheel_scroll(self, local, distance) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_mouse_wheel_scroll(self, position, distance),
        }
    }

    /// Last known keyboard-modifier state. Mouse handlers use this to
    /// implement Ctrl/Shift+Click behavior (mouse events carry no modifiers).
    pub fn modifiers(&self) -> &ModifiersState {
        &self.modifiers
    }

    /// Record the current keyboard-modifier state. Called by the window loop
    /// on `ModifiersChanged`; also useful for synthetic dispatch in tests.
    pub fn set_modifiers(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
    }

    pub fn on_key_down(&mut self, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, modifiers: ModifiersState) -> bool {
        self.modifiers = modifiers.clone();
        // A user KeyDown listener on the focused view runs BEFORE built-in
        // handling, so apps can intercept keys the view would otherwise
        // consume; returning false falls through to normal behavior.
        // Skipped under a modal overlay: the focus owner belongs to the
        // blocked root tree there.
        if !self.has_modal_overlay() {
            let focused = match &self.focus_owner {
                Some(id) => self.get_view(id),
                None => None,
            };
            if let Some(el) = focused {
                let has = el.borrow().has_listener(EventType::KeyDown);
                if has {
                    let data = EventData::Key { code: virtual_key_code, modifiers: modifiers.clone() };
                    if el.borrow().fire_event(self, EventType::KeyDown, &data) {
                        self.sync_focus();
                        return true;
                    }
                }
            }
        }
        let mut consumed = self.dispatch_key_down(virtual_key_code, scancode, modifiers.clone());
        // Tab / Shift+Tab move keyboard focus when no view consumed the key
        // (views that want a literal Tab consume it themselves).
        if !consumed && virtual_key_code == Some(VirtualKeyCode::Tab)
            && !modifiers.ctrl() && !modifiers.alt() {
            consumed = if modifiers.shift() {
                self.focus_prev_view()
            } else {
                self.focus_next_view()
            };
        }
        // Global shortcuts are a fallback: anything the focused view or an
        // overlay consumed (e.g. Ctrl+Z in an Edit) keeps priority. Blocked
        // while a modal dialog is open.
        if !consumed && !self.has_modal_overlay() {
            if let Some(code) = virtual_key_code {
                consumed = self.fire_shortcut(code, &modifiers);
            }
        }
        self.sync_focus();
        consumed
    }

    fn dispatch_key_down(&mut self, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, modifiers: ModifiersState) -> bool {
        // Dispatch to overlays first (reverse order)
        let elements: Vec<Element> = self.overlays.iter().rev()
            .map(|e| Rc::clone(&e.element))
            .collect();
        for element in &elements {
            if element.borrow().on_key_down(self, virtual_key_code, scancode, modifiers.clone()) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_key_down(self, virtual_key_code, scancode, modifiers),
        }
    }

    pub fn on_key_up(&mut self, virtual_key_code: Option<VirtualKeyCode>, scancode: KeyScancode, modifiers: ModifiersState) -> bool {
        self.modifiers = modifiers.clone();
        let elements: Vec<Element> = self.overlays.iter().rev()
            .map(|e| Rc::clone(&e.element))
            .collect();
        for element in &elements {
            if element.borrow().on_key_up(self, virtual_key_code, scancode, modifiers.clone()) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_key_up(self, virtual_key_code, scancode, modifiers),
        }
    }

    pub fn on_key_char(&mut self, unicode_codepoint: char, modifiers: ModifiersState) -> bool {
        let consumed = self.dispatch_key_char(unicode_codepoint, modifiers);
        self.sync_focus();
        consumed
    }

    fn dispatch_key_char(&mut self, unicode_codepoint: char, modifiers: ModifiersState) -> bool {
        let elements: Vec<Element> = self.overlays.iter().rev()
            .map(|e| Rc::clone(&e.element))
            .collect();
        for element in &elements {
            if element.borrow().on_key_char(self, unicode_codepoint, modifiers.clone()) {
                return true;
            }
        }
        if self.has_modal_overlay() {
            return false;
        }
        let root = self.root.clone();
        match root {
            None => false,
            Some(root) => root.borrow().on_key_char(self, unicode_codepoint, modifiers),
        }
    }
}