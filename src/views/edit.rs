use std::cell::RefCell;
use std::cmp::{max, min};
use std::rc::Rc;
use std::time::Instant;
use speedy2d::dimen::Vector2;
use crate::text::{TextBlock, TextOptions};
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseCursorType, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::common::{delete_char, delete_range, insert_str, InputFilter, TextEditOp, TextSnapshot, UNDO_LIMIT};
use crate::image_source::ImageSource;
use crate::views::{Borders, Gravity};
use crate::views::popupmenu::PopupMenu;
use crate::styles::selector::FontSelector;
use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::{PopupDirection, PopupMode, UI};
use crate::view_base::{HasMainFields, ViewBasics};
use super::{BUTTON_MIN_HEIGHT, BUTTON_MIN_WIDTH, Dimension, FieldsMain, FieldsTexted, Visibility};

const DOUBLE_CLICK_MS: u128 = 400;
const ICON_GAP_DIP: i32 = 2;



pub struct Edit {
    state: RefCell<FieldsTexted>,
    scroll_x: RefCell<i32>,
    caret_pos: RefCell<usize>,
    caret_rect: RefCell<Rect<i32>>,
    caret_time: RefCell<Instant>,
    caret_visible: RefCell<bool>,
    // Selection: anchor position. If Some and != caret_pos, text is selected.
    selection_anchor: RefCell<Option<usize>>,
    // Multi-click detection
    last_click_time: RefCell<Instant>,
    click_count: RefCell<u32>,
    // Placeholder, read-only, max length
    placeholder: RefCell<String>,
    read_only: RefCell<bool>,
    max_length: RefCell<Option<usize>>,
    // Key repeat for navigation keys (arrows, home, end, delete)
    held_key: RefCell<Option<VirtualKeyCode>>,
    held_shift: RefCell<bool>,
    held_ctrl: RefCell<bool>,
    key_repeat_time: RefCell<Instant>,
    key_repeat_started: RefCell<bool>,
    // Leading/trailing icons + tint. `None` = no icon for that slot.
    icon_left: RefCell<Option<ImageSource>>,
    icon_right: RefCell<Option<ImageSource>>,
    /// None = the theme's "icon_tint" token; Some = user override.
    icon_tint: RefCell<Option<u32>>,
    // Track which icon (if any) absorbed the most recent mouse-down — used to
    // route the click on mouse-up only if release happens over the same icon.
    pressed_icon: RefCell<Option<bool>>, // Some(true)=left, Some(false)=right
    // Error state + colour
    error: RefCell<bool>,
    /// None = the theme's "error" token; Some = user override.
    error_color: RefCell<Option<u32>>,
    /// True while the left button is held after a press inside the text, so
    /// mouse-move extends the selection (even when the pointer leaves the view).
    dragging: RefCell<bool>,
    // Undo/redo: snapshots taken before each mutating operation.
    undo_stack: RefCell<Vec<TextSnapshot>>,
    redo_stack: RefCell<Vec<TextSnapshot>>,
    /// The kind of the last mutation, for coalescing runs. Reset by caret
    /// moves so the next edit starts a fresh undo entry.
    last_edit_op: std::cell::Cell<Option<TextEditOp>>,
    /// Render the content as bullets and disable copy/cut.
    password: RefCell<bool>,
    /// Per-character input filter: when set, a typed or pasted insert
    /// containing any disallowed character is rejected wholesale.
    /// Programmatic set_text() bypasses it.
    input_filter: RefCell<Option<InputFilter>>,
}

impl HasMainFields for Edit {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for Edit {}

#[allow(dead_code)]
impl Edit {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> Edit {
        let mut fields = FieldsTexted {
            main: FieldsMain::with_rect(rect, Dimension::Max, Dimension::Min),
            text: text.to_owned(),
            text_size,
            line_height: 0f32,
            single_line: true,
            cached_text: None,
            font: FontSelector::new()
        };
        fields.main.padding = Borders::with_padding(4);
        Edit {
            state: RefCell::new(fields),
            scroll_x: RefCell::new(0),
            caret_pos: RefCell::new(0),
            caret_rect: RefCell::new(crate::types::rect((0, 0), (0, 0))),
            caret_time: RefCell::new(Instant::now()),
            caret_visible: RefCell::new(false),
            selection_anchor: RefCell::new(None),
            last_click_time: RefCell::new(Instant::now()),
            click_count: RefCell::new(0),
            placeholder: RefCell::new(String::new()),
            read_only: RefCell::new(false),
            max_length: RefCell::new(None),
            held_key: RefCell::new(None),
            held_shift: RefCell::new(false),
            held_ctrl: RefCell::new(false),
            key_repeat_time: RefCell::new(Instant::now()),
            key_repeat_started: RefCell::new(false),
            icon_left: RefCell::new(None),
            icon_right: RefCell::new(None),
            icon_tint: RefCell::new(None),
            pressed_icon: RefCell::new(None),
            error: RefCell::new(false),
            error_color: RefCell::new(None),
            dragging: RefCell::new(false),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            last_edit_op: std::cell::Cell::new(None),
            password: RefCell::new(false),
            input_filter: RefCell::new(None),
        }
    }

    fn current_snapshot(&self) -> TextSnapshot {
        TextSnapshot {
            text: self.state.borrow().text.clone(),
            caret: *self.caret_pos.borrow(),
            anchor: *self.selection_anchor.borrow(),
        }
    }

    /// Record the state before a mutating operation. Consecutive operations
    /// of the same kind (a typing run, a backspace run) coalesce into the
    /// entry already on the stack.
    fn remember_for_undo(&self, op: TextEditOp) {
        if self.last_edit_op.get() == Some(op) && op != TextEditOp::Other {
            return;
        }
        let snapshot = self.current_snapshot();
        {
            let mut undo = self.undo_stack.borrow_mut();
            if undo.last() != Some(&snapshot) {
                undo.push(snapshot);
                if undo.len() > UNDO_LIMIT {
                    undo.remove(0);
                }
            }
        }
        self.redo_stack.borrow_mut().clear();
        self.last_edit_op.set(Some(op));
    }

    /// The caret moved without an edit — the next edit starts a new undo entry.
    fn break_undo_coalescing(&self) {
        self.last_edit_op.set(None);
    }

    fn restore_snapshot(&self, snapshot: TextSnapshot, ui: &mut UI) {
        self.state.borrow_mut().text = snapshot.text;
        *self.caret_pos.borrow_mut() = snapshot.caret;
        *self.selection_anchor.borrow_mut() = snapshot.anchor;
        self.on_text_changed(ui);
    }

    pub fn undo(&self, ui: &mut UI) -> bool {
        if *self.read_only.borrow() {
            return false;
        }
        let snapshot = self.undo_stack.borrow_mut().pop();
        if let Some(snapshot) = snapshot {
            self.redo_stack.borrow_mut().push(self.current_snapshot());
            self.last_edit_op.set(None);
            self.restore_snapshot(snapshot, ui);
            true
        } else {
            false
        }
    }

    pub fn redo(&self, ui: &mut UI) -> bool {
        if *self.read_only.borrow() {
            return false;
        }
        let snapshot = self.redo_stack.borrow_mut().pop();
        if let Some(snapshot) = snapshot {
            self.undo_stack.borrow_mut().push(self.current_snapshot());
            self.last_edit_op.set(None);
            self.restore_snapshot(snapshot, ui);
            true
        } else {
            false
        }
    }

    /// Mask the content with bullets and disable copy/cut, for password entry.
    pub fn set_password(&self, password: bool) {
        *self.password.borrow_mut() = password;
        self.state.borrow_mut().cached_text = None;
        self.caret_rect.borrow_mut().clear();
        let scale = self.state.borrow().main.scale;
        self.layout_text(self.get_rect_width(), scale);
    }

    pub fn is_password(&self) -> bool {
        *self.password.borrow()
    }

    pub fn set_error(&self, error: bool) {
        *self.error.borrow_mut() = error;
    }

    pub fn is_error(&self) -> bool {
        *self.error.borrow()
    }

    fn load_icons(&self) {
        if let Some(s) = self.icon_left.borrow_mut().as_mut() {
            s.ensure_loaded();
        }
        if let Some(s) = self.icon_right.borrow_mut().as_mut() {
            s.ensure_loaded();
        }
    }

    /// Width in pixels reserved by an icon side. Returns 0 when no icon is set.
    /// `inner_height` is the field's content height (after padding).
    fn icon_side_width(&self, has_icon: bool, inner_height: i32, scale: f64) -> i32 {
        if !has_icon || inner_height <= 0 {
            return 0;
        }
        inner_height + (ICON_GAP_DIP as f64 * scale).round() as i32
    }

    /// Returns (left_inset, right_inset) in pixels for icon reservations.
    /// Used by paint, click hit-testing, scroll bounds, caret rect.
    fn icon_insets(&self, inner_height: i32, scale: f64) -> (i32, i32) {
        let has_left = self.icon_left.borrow().is_some();
        let has_right = self.icon_right.borrow().is_some();
        (
            self.icon_side_width(has_left, inner_height, scale),
            self.icon_side_width(has_right, inner_height, scale),
        )
    }

    /// Returns (left_icon_rect, right_icon_rect) in absolute view coords (no
    /// origin offset; same coord system as `state.main.rect`). `None` for a
    /// side without an icon.
    fn icon_hit_rects(&self) -> (Option<Rect<i32>>, Option<Rect<i32>>) {
        let scale = self.state.borrow().main.scale;
        let padding = self.get_padding(scale);
        let my_rect = self.state.borrow().main.rect;
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        if inner_h <= 0 {
            return (None, None);
        }
        let icon_size = inner_h;
        let inner_top = my_rect.min.y + padding.top;
        let has_left = self.icon_left.borrow().is_some();
        let has_right = self.icon_right.borrow().is_some();
        let left = if has_left {
            let x = my_rect.min.x + padding.left;
            Some(crate::types::rect((x, inner_top), (x + icon_size, inner_top + icon_size)))
        } else {
            None
        };
        let right = if has_right {
            let x = my_rect.max.x - padding.right - icon_size;
            Some(crate::types::rect((x, inner_top), (x + icon_size, inner_top + icon_size)))
        } else {
            None
        };
        (left, right)
    }

    fn fire_icon_event(&self, ui: &mut UI, event: EventType) {
        self.base_fire_event(ui, event, &EventData::None);
    }

    fn draw_icon(&self, theme: &mut dyn Theme, icon_rect: Rect<i32>, is_left: bool, tint: u32) {
        let cell = if is_left { &self.icon_left } else { &self.icon_right };
        if let Some(icon) = cell.borrow_mut().as_mut() {
            icon.draw(theme, icon_rect, tint);
        }
    }

    pub fn set_text(&self, text: &str) {
        // Programmatic content replacement starts a fresh undo history.
        self.undo_stack.borrow_mut().clear();
        self.redo_stack.borrow_mut().clear();
        self.last_edit_op.set(None);
        {
            let mut state = self.state.borrow_mut();
            state.text.clear();
            state.text.push_str(text);
            state.cached_text = None;
            let chars_count = state.text.chars().count();
            if *self.caret_pos.borrow() > chars_count {
                *self.caret_pos.borrow_mut() = chars_count;
                self.caret_rect.borrow_mut().clear();
            }
        }
        *self.selection_anchor.borrow_mut() = None;
        let scale = self.state.borrow().main.scale;
        self.layout_text(self.get_rect_width(), scale);
    }

    pub fn get_text(&self) -> String {
        self.state.borrow().text.clone()
    }

    pub fn set_placeholder(&self, text: &str) {
        *self.placeholder.borrow_mut() = text.to_owned();
    }

    pub fn set_read_only(&self, read_only: bool) {
        *self.read_only.borrow_mut() = read_only;
    }

    pub fn set_max_length(&self, max_length: Option<usize>) {
        *self.max_length.borrow_mut() = max_length;
    }

    /// Restrict typed and pasted input. The predicate judges each character;
    /// an insert containing any disallowed character is rejected wholesale
    /// (a paste with one stray character inserts nothing). Programmatic
    /// `set_text()` is not filtered. Pass `None` to remove the filter.
    pub fn set_input_filter(&self, filter: Option<InputFilter>) {
        *self.input_filter.borrow_mut() = filter;
    }

    fn passes_filter(&self, s: &str) -> bool {
        match self.input_filter.borrow().as_ref() {
            Some(filter) => s.chars().all(filter),
            None => true,
        }
    }

    pub fn set_single_line(&self, single_line: bool) {
        let mut state = self.state.borrow_mut();
        state.single_line = single_line;
        state.cached_text = None;
    }

    pub fn select_all(&self) {
        let len = self.state.borrow().text.chars().count();
        *self.selection_anchor.borrow_mut() = Some(0);
        *self.caret_pos.borrow_mut() = len;
        self.caret_rect.borrow_mut().clear();
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let anchor = *self.selection_anchor.borrow();
        let caret = *self.caret_pos.borrow();
        if let Some(anchor) = anchor {
            if anchor != caret {
                let start = min(anchor, caret);
                let end = max(anchor, caret);
                let text = self.state.borrow().text.chars().skip(start).take(end - start).collect::<String>();
                return Some(text);
            }
        }
        None
    }

    fn has_selection(&self) -> bool {
        let anchor = *self.selection_anchor.borrow();
        if let Some(anchor) = anchor {
            anchor != *self.caret_pos.borrow()
        } else {
            false
        }
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = *self.selection_anchor.borrow();
        if let Some(anchor) = anchor {
            let caret = *self.caret_pos.borrow();
            if anchor != caret {
                return Some((min(anchor, caret), max(anchor, caret)));
            }
        }
        None
    }

    fn clear_selection(&self) {
        *self.selection_anchor.borrow_mut() = None;
    }

    /// Delete selected text, collapse caret to selection start. Returns true if selection existed.
    fn delete_selection(&self) -> bool {
        let range = self.selection_range();
        if let Some((start, end)) = range {
            let new_text = delete_range(&self.state.borrow().text, start, end);
            self.state.borrow_mut().text = new_text;
            *self.caret_pos.borrow_mut() = start;
            self.clear_selection();
            true
        } else {
            false
        }
    }

    /// Begin or extend selection depending on shift state
    fn begin_or_extend_selection(&self, shift: bool) {
        if shift {
            let has_anchor = self.selection_anchor.borrow().is_some();
            if !has_anchor {
                let pos = *self.caret_pos.borrow();
                *self.selection_anchor.borrow_mut() = Some(pos);
            }
        } else {
            self.clear_selection();
        }
    }

    /// After moving caret with shift, if anchor == caret, clear selection
    fn collapse_if_empty(&self) {
        let anchor = *self.selection_anchor.borrow();
        if let Some(anchor) = anchor {
            let caret = *self.caret_pos.borrow();
            if anchor == caret {
                self.clear_selection();
            }
        }
    }

    fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        self.state.borrow().main.font_manager.get_typeface(parent_typeface)
    }

    fn set_font(&self, font_name: &str) {
        self.state.borrow_mut().main.font_manager.set_font(font_name);
    }

    fn set_font_style(&self, style: &str) {
        self.state.borrow_mut().main.font_manager.set_font_style(style);
    }

    fn set_font_size(&self, size: f32) {
        let mut state = self.state.borrow_mut();
        state.main.font_manager.set_font_size(size);
        state.cached_text = None;
        state.line_height = 0f32;
    }

    #[allow(unused_variables)]
    fn layout_text(&self, width: i32, scale: f64) {
        if self.state.borrow().text.is_empty() {
            self.state.borrow_mut().cached_text = None;
            return;
        }
        let typeface = self.state.borrow().main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let options = TextOptions::new();
                // text_size is dips, like an explicit font_size — both scale.
                let base_size = typeface.font_size
                    .unwrap_or(self.state.borrow().text_size) * scale as f32;
                // Password fields render bullets; one per char keeps all the
                // glyph-based geometry (caret, selection, hit tests) valid.
                let display = if *self.password.borrow() {
                    "\u{2022}".repeat(self.state.borrow().text.chars().count())
                } else {
                    self.state.borrow().text.clone()
                };
                let text = font.layout_text(&display, base_size, options);
                self.state.borrow_mut().cached_text = Some(text);
            }
        }
    }

    /// Layout placeholder text for rendering
    fn layout_placeholder_text(&self) -> Option<TextBlock> {
        let placeholder = self.placeholder.borrow();
        if placeholder.is_empty() {
            return None;
        }
        let typeface = self.state.borrow().main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let options = TextOptions::new();
                let scale = self.state.borrow().main.scale;
                let base_size = typeface.font_size
                    .unwrap_or(self.state.borrow().text_size) * scale as f32;
                return Some(font.layout_text(&placeholder, base_size, options));
            }
        }
        None
    }

    /// Get the x-coordinate of a char position relative to the text origin (padding.left of the view)
    fn x_of_char_pos(&self, pos: usize) -> i32 {
        if pos == 0 {
            return 0;
        }
        let state = self.state.borrow();
        if let Some(text) = &state.cached_text {
            if let Some(line) = text.iter_lines().next() {
                for (count, glyph) in line.iter_glyphs().enumerate() {
                    if count == pos - 1 {
                        return glyph.position_x().ceil() as i32 + glyph.advance_width().ceil() as i32;
                    }
                }
                // If pos > glyph count, return end of text
                return text.width().ceil() as i32;
            }
        }
        0
    }

    /// Convert a pixel x-coordinate (relative to the view's content area left edge, accounting for scroll) to a char position
    fn char_pos_from_x(&self, x: i32) -> usize {
        let state = self.state.borrow();
        if let Some(text) = &state.cached_text {
            if let Some(line) = text.iter_lines().next() {
                for (count, glyph) in line.iter_glyphs().enumerate() {
                    let glyph_left = glyph.position_x().round() as i32;
                    let glyph_right = glyph_left + glyph.advance_width().round() as i32;
                    let mid = (glyph_left + glyph_right) / 2;
                    if x < mid {
                        return count;
                    }
                }
                // Past end of text: return total glyph count
                return line.iter_glyphs().count();
            }
        }
        0
    }

    fn update_caret_rect(&self, scale: f64) {
        let padding = self.get_padding(scale);
        let mut rect = *self.caret_rect.borrow();
        let caret_pos = *self.caret_pos.borrow();
        let my_rect = self.state.borrow().main.rect;
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        let (left_inset, _) = self.icon_insets(inner_h, scale);

        rect.min.y = my_rect.min.y + padding.top + 2;
        rect.max.y = my_rect.max.y - padding.bottom - 2;

        let x_offset = self.x_of_char_pos(caret_pos);
        rect.min.x = my_rect.min.x + padding.left + left_inset + x_offset;
        rect.max.x = rect.min.x + (crate::drawing::current_dimension("caret.width") as f64 * scale) as i32;

        *self.caret_rect.borrow_mut() = rect;
        *self.caret_time.borrow_mut() = Instant::now();
        *self.caret_visible.borrow_mut() = true;
    }

    fn get_caret_rect(&self, scale: f64) -> Rect<i32> {
        let rect = *self.caret_rect.borrow();
        if rect.width() != 0 && rect.height() != 0 {
            return rect;
        }
        self.update_caret_rect(scale);
        *self.caret_rect.borrow()
    }

    fn update_scroll(&self) {
        let scale = self.state.borrow().main.scale;
        let my_rect = self.state.borrow().main.rect;
        let caret_rect = self.get_caret_rect(scale);
        let padding = self.get_padding(scale);
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        let (left_inset, right_inset) = self.icon_insets(inner_h, scale);
        let cur_scroll_x = *self.scroll_x.borrow();
        let view_left = my_rect.min.x + padding.left + left_inset;
        let view_right = my_rect.max.x - padding.right - right_inset;

        if caret_rect.max.x + cur_scroll_x > view_right {
            // Caret is past the right edge — scroll so caret is at the right edge
            *self.scroll_x.borrow_mut() = view_right - caret_rect.max.x;
        } else if caret_rect.min.x + cur_scroll_x < view_left {
            // Caret is past the left edge — scroll so caret is at the left edge
            *self.scroll_x.borrow_mut() = view_left - caret_rect.min.x;
        }
    }

    fn get_line_height(&self) -> f32 {
        if self.state.borrow().line_height != 0f32 {
            return self.state.borrow().line_height;
        }

        let typeface = self.state.borrow().main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let options = TextOptions::new();
                let scale = self.state.borrow().main.scale;
                let base_size = typeface.font_size
                    .unwrap_or(self.state.borrow().text_size) * scale as f32;
                let text = font.layout_text("W", base_size, options);
                self.state.borrow_mut().line_height = text.height();
            }
        }
        self.state.borrow_mut().line_height
    }

    /// Find the start of the word at or before `pos`
    fn word_start(&self, pos: usize) -> usize {
        let text = self.state.borrow().text.clone();
        let chars: Vec<char> = text.chars().collect();
        if pos == 0 {
            return 0;
        }
        let mut p = pos;
        // Skip whitespace going left
        while p > 0 && !chars[p - 1].is_alphanumeric() {
            p -= 1;
        }
        // Skip word chars going left
        while p > 0 && chars[p - 1].is_alphanumeric() {
            p -= 1;
        }
        p
    }

    /// Find the end of the word at or after `pos`
    fn word_end(&self, pos: usize) -> usize {
        let text = self.state.borrow().text.clone();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if pos >= len {
            return len;
        }
        let mut p = pos;
        // Skip word chars going right
        while p < len && chars[p].is_alphanumeric() {
            p += 1;
        }
        // Skip whitespace going right
        while p < len && !chars[p].is_alphanumeric() {
            p += 1;
        }
        p
    }

    /// Called after any text mutation to invalidate cache, relayout, and fire TextChanged
    /// Handle navigation key action. Used by both on_key_down and key repeat in update().
    fn handle_nav_key(&self, ui: &mut UI, code: VirtualKeyCode, shift: bool, ctrl: bool) -> bool {
        self.break_undo_coalescing();
        match code {
            VirtualKeyCode::Left => {
                if !shift && self.has_selection() && !ctrl {
                    if let Some((start, _)) = self.selection_range() {
                        *self.caret_pos.borrow_mut() = start;
                    }
                    self.clear_selection();
                } else {
                    self.begin_or_extend_selection(shift);
                    if ctrl {
                        let pos = *self.caret_pos.borrow();
                        *self.caret_pos.borrow_mut() = self.word_start(pos);
                    } else {
                        let pos = *self.caret_pos.borrow();
                        if pos > 0 {
                            *self.caret_pos.borrow_mut() = pos - 1;
                        }
                    }
                    if !shift {
                        self.clear_selection();
                    }
                    self.collapse_if_empty();
                }
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::Right => {
                if !shift && self.has_selection() && !ctrl {
                    if let Some((_, end)) = self.selection_range() {
                        *self.caret_pos.borrow_mut() = end;
                    }
                    self.clear_selection();
                } else {
                    self.begin_or_extend_selection(shift);
                    let text_len = self.state.borrow().text.chars().count();
                    if ctrl {
                        let pos = *self.caret_pos.borrow();
                        *self.caret_pos.borrow_mut() = self.word_end(pos);
                    } else {
                        let pos = *self.caret_pos.borrow();
                        if pos < text_len {
                            *self.caret_pos.borrow_mut() = pos + 1;
                        }
                    }
                    if !shift {
                        self.clear_selection();
                    }
                    self.collapse_if_empty();
                }
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::Home => {
                if !shift && self.has_selection() {
                    self.clear_selection();
                } else {
                    self.begin_or_extend_selection(shift);
                }
                *self.caret_pos.borrow_mut() = 0;
                if !shift {
                    self.clear_selection();
                }
                self.collapse_if_empty();
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::End => {
                if !shift && self.has_selection() {
                    self.clear_selection();
                } else {
                    self.begin_or_extend_selection(shift);
                }
                let new_pos = self.state.borrow().text.chars().count();
                *self.caret_pos.borrow_mut() = new_pos;
                if !shift {
                    self.clear_selection();
                }
                self.collapse_if_empty();
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::Delete => {
                if *self.read_only.borrow() {
                    return false;
                }
                self.remember_for_undo(TextEditOp::Deleting);
                if self.has_selection() {
                    self.delete_selection();
                    self.on_text_changed(ui);
                    return true;
                }
                let pos = *self.caret_pos.borrow();
                let text_len = self.state.borrow().text.chars().count();
                if pos < text_len {
                    if ctrl {
                        let end = self.word_end(pos);
                        let new_text = delete_range(&self.state.borrow().text, pos, end);
                        self.state.borrow_mut().text = new_text;
                    } else {
                        let new_text = delete_char(&self.state.borrow().text, pos);
                        self.state.borrow_mut().text = new_text;
                    }
                    self.on_text_changed(ui);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn on_text_changed(&self, ui: &mut UI) {
        self.state.borrow_mut().cached_text = None;
        self.caret_rect.borrow_mut().clear();
        let scale = self.state.borrow().main.scale;
        self.layout_text(self.get_rect_width(), scale);
        self.fire_text_changed(ui);
    }

    fn fire_text_changed(&self, ui: &mut UI) {
        self.base_fire_event(ui, EventType::TextChanged, &EventData::None);
    }

    /// Insert text at caret, replacing selection if any. Respects max_length and read_only.
    /// Returns true if text was modified.
    fn insert_text_at_caret(&self, ui: &mut UI, s: &str) -> bool {
        if *self.read_only.borrow() || s.is_empty() || !self.passes_filter(s) {
            return false;
        }
        // Single chars are typing (coalesced); larger inserts (paste) are not.
        self.remember_for_undo(if s.chars().count() == 1 { TextEditOp::Typing } else { TextEditOp::Other });
        let had_selection = self.delete_selection();
        let pos = *self.caret_pos.borrow();
        let current_len = self.state.borrow().text.chars().count();

        let insert_text = if let Some(max_len) = *self.max_length.borrow() {
            if current_len >= max_len {
                if had_selection {
                    self.on_text_changed(ui);
                }
                return had_selection;
            }
            let available = max_len - current_len;
            let chars: Vec<char> = s.chars().take(available).collect();
            chars.into_iter().collect::<String>()
        } else {
            s.to_owned()
        };

        let insert_len = insert_text.chars().count();
        let new_text = insert_str(&self.state.borrow().text, pos, &insert_text);
        self.state.borrow_mut().text = new_text;
        *self.caret_pos.borrow_mut() = pos + insert_len;
        self.on_text_changed(ui);
        true
    }

    fn copy_to_clipboard(&self) {
        // Never expose password content to the clipboard.
        if *self.password.borrow() {
            return;
        }
        if let Some(text) = self.get_selected_text() {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(text);
            }
        }
    }

    fn paste_from_clipboard(&self, ui: &mut UI) -> bool {
        if *self.read_only.borrow() {
            return false;
        }
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(text) = clipboard.get_text() {
                // Filter to single line if single_line mode
                let text = if self.state.borrow().single_line {
                    text.replace('\n', " ").replace('\r', "")
                } else {
                    text
                };
                return self.insert_text_at_caret(ui, &text);
            }
        }
        false
    }

    fn open_context_menu(&self, ui: &mut UI, x: i32, y: i32) {
        let mut menu = PopupMenu::new();
        menu.add_item("cut", "", "Cut");
        menu.add_item("copy", "", "Copy");
        menu.add_item("paste", "", "Paste");
        menu.add_item("delete", "", "Delete");
        menu.add_separator();
        menu.add_item("select_all", "", "Select All");

        let edit_id = self.get_id();
        menu.on_event(EventType::Click, Box::new(move |ui: &mut UI, view: &dyn View, _data: &EventData| {
            let menu = view.as_any().downcast_ref::<PopupMenu>().unwrap();
            let index = menu.get_hovered_index();
            if let Some(index) = index {
                // Flags extracted from scoped borrows to drive subsequent &mut UI calls
                let mut need_text_changed = false;
                let mut need_paste = false;

                match index {
                    0 => {
                        // Cut: copy + delete selection (no-op for password fields)
                        if let Some(el) = ui.get_view(&edit_id) {
                            let b = el.borrow();
                            let edit = b.as_any().downcast_ref::<Edit>().unwrap();
                            if !*edit.password.borrow() {
                                edit.copy_to_clipboard();
                                if edit.has_selection() && !*edit.read_only.borrow() {
                                    edit.remember_for_undo(TextEditOp::Other);
                                    edit.delete_selection();
                                    need_text_changed = true;
                                }
                            }
                        }
                    }
                    1 => {
                        // Copy
                        if let Some(el) = ui.get_view(&edit_id) {
                            let b = el.borrow();
                            let edit = b.as_any().downcast_ref::<Edit>().unwrap();
                            edit.copy_to_clipboard();
                        }
                    }
                    2 => {
                        // Paste
                        need_paste = true;
                    }
                    3 => {
                        // Delete selection
                        if let Some(el) = ui.get_view(&edit_id) {
                            let b = el.borrow();
                            let edit = b.as_any().downcast_ref::<Edit>().unwrap();
                            if edit.has_selection() && !*edit.read_only.borrow() {
                                edit.remember_for_undo(TextEditOp::Other);
                                edit.delete_selection();
                                need_text_changed = true;
                            }
                        }
                    }
                    5 => {
                        // Select All
                        if let Some(el) = ui.get_view(&edit_id) {
                            let b = el.borrow();
                            let edit = b.as_any().downcast_ref::<Edit>().unwrap();
                            edit.select_all();
                        }
                    }
                    _ => {}
                }

                // Deferred operations that need &mut UI (borrows above are dropped)
                if need_paste {
                    if let Some(el) = ui.get_view(&edit_id) {
                        let b = el.borrow();
                        let edit = b.as_any().downcast_ref::<Edit>().unwrap();
                        edit.paste_from_clipboard(ui);
                    }
                }
                if need_text_changed {
                    if let Some(el) = ui.get_view(&edit_id) {
                        let b = el.borrow();
                        let edit = b.as_any().downcast_ref::<Edit>().unwrap();
                        edit.on_text_changed(ui);
                    }
                }
            }
            true
        }));

        let element: Element = Rc::new(RefCell::new(menu));
        // `x`/`y` arrive in parent-local coords (Frame subtracts its origin when
        // dispatching), but `show_popup` positions in window coords. Add the
        // accumulated ancestor origin (`get_absolute_position` - own `rect.min`).
        let abs = self.get_absolute_position();
        let rect_min = self.state.borrow().main.rect.min;
        let (wx, wy) = (x + abs.x - rect_min.x, y + abs.y - rect_min.y);
        ui.show_popup(element, wx, wy, PopupDirection::BottomRight, PopupMode::Popup);
    }
}

impl View for Edit {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }

        match name {
            "text" => { self.set_text(value) }
            "font" => { self.set_font(value) }
            "font_style" => { self.set_font_style(value) }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.set_font_size(size);
                }
            }
            "placeholder" => { self.set_placeholder(value) }
            "readonly" => { self.set_read_only(value == "true") }
            "password" => { self.set_password(value == "true") }
            "single_line" => { self.state.borrow_mut().single_line = value.parse().unwrap_or(true) }
            "maxlength" => {
                if let Ok(n) = value.parse::<usize>() {
                    self.set_max_length(Some(n));
                }
            }
            "filter" => {
                if value == "numeric" {
                    self.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit())));
                }
            }
            "allowed_chars" => {
                let set: std::collections::HashSet<char> = value.chars().collect();
                self.set_input_filter(Some(Box::new(move |c| set.contains(&c))));
            }
            "icon_left" => {
                *self.icon_left.borrow_mut() = ImageSource::for_path(value);
            }
            "icon_right" => {
                *self.icon_right.borrow_mut() = ImageSource::for_path(value);
            }
            "icon_tint" => {
                if let Some(c) = crate::view_base::parse_color_value(value) {
                    *self.icon_tint.borrow_mut() = Some(c);
                }
            }
            "error" => {
                self.set_error(value == "true");
            }
            "error_color" => {
                if let Some(c) = crate::view_base::parse_color_value(value) {
                    *self.error_color.borrow_mut() = Some(c);
                }
            }
            &_ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        // Recompute the cached text when it's missing OR when the scale changed
        // (rendered font size = text_size * scale). Without the scale check a
        // view laid out once at one scale keeps stale, wrongly-sized glyphs —
        // e.g. a dialog measured at scale 1.0 before its window opens at the
        // display scale would render the Edit text half-size.
        let stale_scale = self.state.borrow().main.scale != scale;
        if self.state.borrow().cached_text.is_none() || stale_scale {
            let typeface = self.get_typeface(typeface);
            self.state.borrow_mut().main.font_manager.set(Some(typeface));
            self.base_set_scale(scale);
            self.layout_text(width, scale);
        }
        let (new_width, new_height) = self.calculate_size(width, height, scale);
        let (w, h) = self.calculate_full_size(scale);
        let (width, height) = {
            let state = self.state.borrow_mut();
            let ww = match &state.main.width {
                Dimension::Min => w,
                Dimension::Max => new_width,
                Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => (width as f32 * p / 100f32).round() as i32
            };
            let hh = match &state.main.height {
                Dimension::Min => h,
                Dimension::Max => new_height,
                Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => (height as f32 * p / 100f32).round() as i32
            };
            (ww, hh)
        };
        let rect = rect((x, y), (x + width, y + height));
        self.set_rect(rect);
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let state = self.state.borrow();
        match &state.cached_text {
            Some(text) => text.width() <= width as f32 && text.height() <= height as f32,
            None => width <= BUTTON_MIN_WIDTH && height <= BUTTON_MIN_HEIGHT
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        // Lazy load icon assets on first paint
        self.load_icons();
        self.update_scroll();
        let state = self.state.borrow();
        let scroll_x = *self.scroll_x.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);

        // Step 1: Draw background
        theme.push_clip();
        theme.clip_rect(rect);
        theme.draw_component("edit.back", rect, state.main.state);
        theme.pop_clip();

        // Step 2: Draw selection highlight + text (or placeholder)
        let padding = state.main.padding.scaled(state.main.scale);
        let mut text_rect = rect;
        text_rect.shrink_by(padding.top, padding.left, padding.right, padding.bottom);
        let inner_h = text_rect.height();
        let scale = state.main.scale;
        let (left_inset, right_inset) = self.icon_insets(inner_h, scale);
        // Reserve icon space inside the text rect
        text_rect.min.x += left_inset;
        text_rect.max.x -= right_inset;
        theme.push_clip();
        theme.clip_rect(text_rect);

        if let Some(text) = &state.cached_text {
            let y = (text_rect.height() as f32 - text.height()) / 2f32;
            let text_x = (text_rect.min.x as f32 + scroll_x as f32).round();
            let text_y = (text_rect.min.y as f32 + y).round();

            // Draw selection highlight if any
            let mut sel_rect = None;
            if let Some(anchor) = *self.selection_anchor.borrow() {
                let caret = *self.caret_pos.borrow();
                if anchor != caret {
                    let sel_start = min(anchor, caret);
                    let sel_end = max(anchor, caret);
                    let x1 = self.x_of_char_pos(sel_start);
                    let x2 = self.x_of_char_pos(sel_end);
                    let rect = crate::types::rect(
                        (text_rect.min.x + x1 + scroll_x, text_rect.min.y),
                        (text_rect.min.x + x2 + scroll_x, text_rect.max.y),
                    );
                    theme.draw_rect(rect, theme.color("selection"));
                    sel_rect = Some(rect);
                }
            }

            // Draw text
            let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
            theme.draw_text(text_x, text_y, color, text);
            // Redraw the selected part in a contrasting color over the highlight
            if let Some(sel_rect) = sel_rect {
                let sel_color = crate::themes::selection_text_color(theme.color("selection"));
                theme.draw_text_cropped(text_x, text_y, sel_rect, sel_color, text);
            }
        } else if !self.placeholder.borrow().is_empty() {
            // Draw placeholder text when empty
            drop(state); // Release borrow before layout_placeholder_text
            if let Some(placeholder_text) = self.layout_placeholder_text() {
                let state = self.state.borrow();
                let y = (text_rect.height() as f32 - placeholder_text.height()) / 2f32;
                theme.draw_text(
                    text_rect.min.x as f32,
                    (text_rect.min.y as f32 + y).round(),
                    theme.color("text_hint"),
                    &placeholder_text,
                );
                drop(state);
            }
            // Re-borrow for the rest of the method
            let _state = self.state.borrow();
        }
        theme.pop_clip();

        // Step 2.5: Draw leading/trailing icons (between text and borders).
        // Icons are square, sized to the field's inner (padded) height, vertically centred.
        let tint = self.icon_tint.borrow().unwrap_or_else(|| theme.color("icon_tint"));
        let icon_size = inner_h;
        if icon_size > 0 {
            let inner_top = rect.min.y + padding.top;
            // Padded content rect is the visual bounds for clipping icons
            let mut content_rect = rect;
            content_rect.shrink_by(padding.top, padding.left, padding.right, padding.bottom);
            theme.push_clip();
            theme.clip_rect(content_rect);

            if left_inset > 0 {
                let icon_x = rect.min.x + padding.left;
                let icon_rect = crate::types::rect(
                    (icon_x, inner_top),
                    (icon_x + icon_size, inner_top + icon_size),
                );
                self.draw_icon(theme, icon_rect, true, tint);
            }
            if right_inset > 0 {
                let icon_x = rect.max.x - padding.right - icon_size;
                let icon_rect = crate::types::rect(
                    (icon_x, inner_top),
                    (icon_x + icon_size, inner_top + icon_size),
                );
                self.draw_icon(theme, icon_rect, false, tint);
            }
            theme.pop_clip();
        }

        // Step 3: Draw borders (after text)
        let state = self.state.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        theme.draw_component("edit.body", rect, state.main.state);
        theme.pop_clip();

        // Step 3.5: Error underline. Draw a 2-dip line just above the existing
        // bottom inset highlights — it visually underlines the field without
        // disturbing the outer 1-pixel highlight at max.y - 1.
        if *self.error.borrow() {
            let line_h = ((2.0 * scale).round() as i32).max(1);
            let inset = ((2.0 * scale).round() as i32).max(2);
            let bottom_offset = (2.0 * scale).round() as i32;
            let underline = crate::types::rect(
                (rect.min.x + inset, rect.max.y - bottom_offset - line_h),
                (rect.max.x - inset, rect.max.y - bottom_offset),
            );
            let error_color = self.error_color.borrow().unwrap_or_else(|| theme.color("error"));
            theme.draw_rect(underline, error_color);
        }

        // Step 4: Draw caret (on top of everything, only when no selection or always)
        if state.main.state.focused && *self.caret_visible.borrow() {
            let mut caret_rect = self.get_caret_rect(state.main.scale);
            caret_rect.move_by(origin);
            caret_rect.move_by((scroll_x, 0));
            theme.draw_component("edit.caret", caret_rect, state.main.state);
        }
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().main.state)
    }

    fn get_rect(&self) -> Rect<i32> {
        self.base_get_rect()
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.base_set_rect(rect);
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.base_get_padding(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_padding(top, left, right, bottom);
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.base_get_margin(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_margin(top, left, right, bottom);
    }

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn get_layout_params(&self) -> super::LayoutParams {
        self.base_get_layout_params()
    }

    fn set_layout_params(&self, params: super::LayoutParams) {
        self.base_set_layout_params(params);
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        let line_height = self.get_line_height().round() as i32;
        let state = self.state.borrow();
        match &state.cached_text {
            None => {
                (BUTTON_MIN_WIDTH, line_height)
            },
            Some(text) => {
                let width = max(text.width().round() as i32, BUTTON_MIN_WIDTH);
                let height = max(text.height().round() as i32, line_height);
                (width, height)
            }
        }
    }

    fn is_focused(&self) -> bool {
        self.base_is_focused()
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn set_focused(&self, focused: bool) {
        self.base_set_focused(focused);
    }

    fn set_focusable(&self, focusable: bool) {
        self.base_set_focusable(focusable);
    }

    fn set_width(&mut self, width: Dimension) {
        self.base_set_width(width);
    }

    fn set_height(&mut self, height: Dimension) {
        self.base_set_height(height);
    }

    fn set_scale(&mut self, scale: f64) {
        self.base_set_scale(scale);
    }

    fn set_id(&mut self, id: &str) {
        self.base_set_id(id);
    }

    fn get_id(&self) -> String {
        self.base_get_id()
    }
    fn is_enabled(&self) -> bool {
        self.base_is_enabled()
    }
    fn set_enabled(&mut self, enabled: bool) {
        self.base_set_enabled(enabled);
    }

    fn get_visibility(&self) -> Visibility {
        self.base_get_visibility()
    }
    fn set_visibility(&mut self, visibility: Visibility) {
        self.base_set_visibility(visibility);
    }
    fn get_tooltip(&self) -> Option<String> {
        self.base_get_tooltip()
    }
    fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.base_set_tooltip(tooltip);
    }

    fn get_background(&self) -> Option<u32> {
        self.base_get_background()
    }
    fn set_background(&mut self, color: Option<u32>) {
        self.base_set_background(color);
    }
    fn get_border_color(&self) -> Option<u32> {
        self.base_get_border_color()
    }
    fn set_border_color(&mut self, color: Option<u32>) {
        self.base_set_border_color(color);
    }

    fn on_event(&mut self, event: EventType, func: EventCallback) {
        self.base_on_event(event, func);
    }

    fn has_listener(&self, event: EventType) -> bool {
        self.base_has_listener(event)
    }

    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool {
        self.base_fire_event(ui, event, data)
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.base_fire_event(ui, EventType::Click, &EventData::None)
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        let focused = self.state.borrow().main.state.focused;
        let mut redraw = false;

        if focused {
            // Caret blink
            let elapsed = self.caret_time.borrow().elapsed().as_millis();
            if elapsed >= 500 {
                let visible = *self.caret_visible.borrow();
                *self.caret_visible.borrow_mut() = !visible;
                *self.caret_time.borrow_mut() = Instant::now();
                redraw = true;
            }

            // Key repeat for navigation keys
            let held = *self.held_key.borrow();
            if let Some(code) = held {
                let repeat_elapsed = self.key_repeat_time.borrow().elapsed().as_millis();
                let started = *self.key_repeat_started.borrow();
                // Initial delay 400ms, then repeat every 33ms (~30 repeats/sec)
                let threshold = if started { 33 } else { 400 };
                if repeat_elapsed >= threshold {
                    *self.key_repeat_time.borrow_mut() = Instant::now();
                    *self.key_repeat_started.borrow_mut() = true;
                    let shift = *self.held_shift.borrow();
                    let ctrl = *self.held_ctrl.borrow();
                    if self.handle_nav_key(ui, code, shift, ctrl) {
                        redraw = true;
                    }
                }
            }
        }

        redraw
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Vector2<i32>) -> bool {
        let scale = self.state.borrow().main.scale;
        let padding = self.get_padding(scale);
        let my_rect = self.state.borrow().main.rect;
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        let (left_inset, right_inset) = self.icon_insets(inner_h, scale);

        // Drag-selection: extend regardless of whether the pointer is still
        // inside the view (the parent dispatches moves to every child).
        if *self.dragging.borrow() {
            let scroll_x = *self.scroll_x.borrow();
            let move_x = position.x - my_rect.min.x - padding.left - left_inset - scroll_x;
            let char_pos = self.char_pos_from_x(move_x);
            *self.caret_pos.borrow_mut() = char_pos;
            self.caret_rect.borrow_mut().clear();
            self.update_scroll();
            return true;
        }

        let hit = my_rect.hit((position.x, position.y));
        // I-beam only over the text area, not the left/right icon regions.
        if hit {
            let text_left = my_rect.min.x + padding.left + left_inset;
            let text_right = my_rect.max.x - padding.right - right_inset;
            if position.x >= text_left && position.x < text_right {
                ui.request_cursor(MouseCursorType::Text);
            }
        }
        let old_state = self.state.borrow().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow().main.state != old_state
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !self.state.borrow().main.rect.hit((position.x, position.y)) {
            return false;
        }
        self.break_undo_coalescing();

        if !matches!(button, MouseButton::Left) {
            self.state.borrow_mut().main.state.focused = true;
            if matches!(button, MouseButton::Right) && !ui.context_menu_suppressed() {
                self.open_context_menu(ui, position.x, position.y);
            }
            return true;
        }

        // Left-click on an icon? Capture press; the click fires on mouse-up if
        // the release lands on the same icon.
        let (left_rect, right_rect) = self.icon_hit_rects();
        if let Some(r) = left_rect {
            if r.hit((position.x, position.y)) {
                *self.pressed_icon.borrow_mut() = Some(true);
                return true;
            }
        }
        if let Some(r) = right_rect {
            if r.hit((position.x, position.y)) {
                *self.pressed_icon.borrow_mut() = Some(false);
                return true;
            }
        }

        self.state.borrow_mut().main.state.pressed = true;
        self.state.borrow_mut().main.state.focused = true;

        // Calculate char position from click
        let scale = self.state.borrow().main.scale;
        let padding = self.get_padding(scale);
        let my_rect = self.state.borrow().main.rect;
        let scroll_x = *self.scroll_x.borrow();
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        let (left_inset, _) = self.icon_insets(inner_h, scale);
        let click_x = position.x - my_rect.min.x - padding.left - left_inset - scroll_x;
        let char_pos = self.char_pos_from_x(click_x);

        // Multi-click detection
        let elapsed = self.last_click_time.borrow().elapsed().as_millis();
        let prev_count = *self.click_count.borrow();

        if elapsed < DOUBLE_CLICK_MS && prev_count >= 1 {
            let new_count = prev_count + 1;
            *self.click_count.borrow_mut() = new_count;
            *self.last_click_time.borrow_mut() = Instant::now();

            if new_count == 2 {
                // Double-click: select word
                let ws = self.word_start(char_pos);
                let we = self.word_end_only(char_pos);
                *self.selection_anchor.borrow_mut() = Some(ws);
                *self.caret_pos.borrow_mut() = we;
                self.caret_rect.borrow_mut().clear();
                return true;
            } else if new_count >= 3 {
                // Triple-click: select all
                self.select_all();
                *self.click_count.borrow_mut() = 0;
                return true;
            }
        } else {
            *self.click_count.borrow_mut() = 1;
            *self.last_click_time.borrow_mut() = Instant::now();
        }

        // Single click: position caret and begin a drag-selection. Shift+click
        // extends the selection from the existing anchor (or the current caret).
        let shift = *self.held_shift.borrow();
        if shift {
            let has_anchor = self.selection_anchor.borrow().is_some();
            if !has_anchor {
                let old = *self.caret_pos.borrow();
                *self.selection_anchor.borrow_mut() = Some(old);
            }
            *self.caret_pos.borrow_mut() = char_pos;
        } else {
            *self.caret_pos.borrow_mut() = char_pos;
            *self.selection_anchor.borrow_mut() = Some(char_pos);
        }
        *self.dragging.borrow_mut() = true;
        self.caret_rect.borrow_mut().clear();
        true
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        // End any drag-selection; drop the zero-length anchor a plain click left.
        if *self.dragging.borrow() {
            *self.dragging.borrow_mut() = false;
            if *self.selection_anchor.borrow() == Some(*self.caret_pos.borrow()) {
                self.clear_selection();
            }
        }
        // If a press started on an icon, treat the up over the same icon as a click.
        let pressed_icon = self.pressed_icon.borrow_mut().take();
        if let Some(was_left) = pressed_icon {
            let (left_rect, right_rect) = self.icon_hit_rects();
            let target = if was_left { left_rect } else { right_rect };
            if let Some(r) = target {
                if r.hit((position.x, position.y)) {
                    let event = if was_left { EventType::LeftIconClick } else { EventType::RightIconClick };
                    self.fire_icon_event(ui, event);
                }
            }
            return true;
        }
        if self.state.borrow().main.state.pressed {
            self.state.borrow_mut().main.state.pressed = false;
            return true;
        }
        false
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        if let Some(code) = virtual_key_code {
            let shift = state.shift();
            let ctrl = state.ctrl();

            // Track repeatable keys
            match code {
                VirtualKeyCode::Left | VirtualKeyCode::Right |
                VirtualKeyCode::Home | VirtualKeyCode::End |
                VirtualKeyCode::Delete => {
                    if *self.held_key.borrow() != Some(code) {
                        *self.held_key.borrow_mut() = Some(code);
                        *self.held_shift.borrow_mut() = shift;
                        *self.held_ctrl.borrow_mut() = ctrl;
                        *self.key_repeat_time.borrow_mut() = Instant::now();
                        *self.key_repeat_started.borrow_mut() = false;
                    }
                }
                _ => {}
            }

            match code {
                VirtualKeyCode::Left | VirtualKeyCode::Right |
                VirtualKeyCode::Home | VirtualKeyCode::End |
                VirtualKeyCode::Delete => {
                    return self.handle_nav_key(ui, code, shift, ctrl);
                }
                VirtualKeyCode::A if ctrl => {
                    self.select_all();
                    return true;
                }
                VirtualKeyCode::C if ctrl => {
                    self.copy_to_clipboard();
                    return true;
                }
                VirtualKeyCode::X if ctrl => {
                    // Password fields neither copy nor cut.
                    if *self.password.borrow() {
                        return false;
                    }
                    if *self.read_only.borrow() {
                        self.copy_to_clipboard();
                        return false;
                    }
                    self.copy_to_clipboard();
                    if self.has_selection() {
                        self.remember_for_undo(TextEditOp::Other);
                        self.delete_selection();
                        self.on_text_changed(ui);
                        return true;
                    }
                    return false;
                }
                VirtualKeyCode::V if ctrl => {
                    return self.paste_from_clipboard(ui);
                }
                VirtualKeyCode::Z if ctrl && shift => {
                    return self.redo(ui);
                }
                VirtualKeyCode::Z if ctrl => {
                    return self.undo(ui);
                }
                VirtualKeyCode::Y if ctrl => {
                    return self.redo(ui);
                }
                _ => {}
            }
        }
        false
    }

    fn on_key_up(&self, _ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if let Some(code) = virtual_key_code {
            if *self.held_key.borrow() == Some(code) {
                *self.held_key.borrow_mut() = None;
            }
        }
        false
    }

    fn on_key_char(&self, ui: &mut UI, ch: char, state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        // When Ctrl is held, ignore character input — Ctrl+key combos are handled in on_key_down
        if state.ctrl() {
            return false;
        }

        // Ignore control characters
        if ch < ' ' && ch != '\u{8}' && ch != '\u{7f}' {
            return false;
        }

        // Handle backspace
        if ch == '\u{8}' {
            if *self.read_only.borrow() {
                return false;
            }
            self.remember_for_undo(TextEditOp::Deleting);
            if self.has_selection() {
                self.delete_selection();
                self.on_text_changed(ui);
                return true;
            }
            let pos = *self.caret_pos.borrow();
            if pos > 0 {
                // Check if this is Ctrl+Backspace (word delete) — on some platforms
                // Ctrl+Backspace sends '\u{7f}', on others '\u{8}' with ctrl state.
                // We already handle Ctrl above, so this is plain backspace.
                let new_text = delete_char(&self.state.borrow().text, pos - 1);
                self.state.borrow_mut().text = new_text;
                *self.caret_pos.borrow_mut() = pos - 1;
                self.on_text_changed(ui);
                return true;
            }
            return false;
        }

        // Handle delete (some systems send '\u{7f}' for Delete key)
        if ch == '\u{7f}' {
            if *self.read_only.borrow() {
                return false;
            }
            self.remember_for_undo(TextEditOp::Deleting);
            if self.has_selection() {
                self.delete_selection();
                self.on_text_changed(ui);
                return true;
            }
            let pos = *self.caret_pos.borrow();
            let text_len = self.state.borrow().text.chars().count();
            if pos < text_len {
                let new_text = delete_char(&self.state.borrow().text, pos);
                self.state.borrow_mut().text = new_text;
                self.on_text_changed(ui);
                return true;
            }
            return false;
        }

        // Regular character input
        if ch.is_alphanumeric() || ch >= ' ' {
            if *self.read_only.borrow() {
                return false;
            }
            let s = ch.to_string();
            return self.insert_text_at_caret(ui, &s);
        }

        false
    }

    fn on_key_mod_changed(&self, _ui: &mut UI, _state: ModifiersState) -> bool {
        false
    }
}

/// Helper: find end of word only (without skipping trailing whitespace)
/// Used by double-click word selection
impl Edit {
    fn word_end_only(&self, pos: usize) -> usize {
        let text = self.state.borrow().text.clone();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if pos >= len {
            return len;
        }
        let mut p = pos;
        // If on a non-alphanumeric char, skip to next word
        if !chars[p].is_alphanumeric() {
            while p < len && !chars[p].is_alphanumeric() {
                p += 1;
            }
            return p;
        }
        // Skip word chars going right
        while p < len && chars[p].is_alphanumeric() {
            p += 1;
        }
        p
    }
}

impl Default for Edit {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        Edit::new(rect, "", crate::drawing::current_text_size("text"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_and_ui() -> (Edit, UI) {
        let ui = UI::new(800, 600, Typeface::default(), 1.0);
        let edit = Edit::new(rect((0, 0), (200, 24)), "", 16.0);
        (edit, ui)
    }

    #[test]
    fn test_typing_run_coalesces_into_one_undo() {
        let (edit, mut ui) = edit_and_ui();
        edit.insert_text_at_caret(&mut ui, "a");
        edit.insert_text_at_caret(&mut ui, "b");
        edit.insert_text_at_caret(&mut ui, "c");
        assert_eq!(edit.get_text(), "abc");
        assert!(edit.undo(&mut ui));
        assert_eq!(edit.get_text(), "");
        assert!(!edit.undo(&mut ui));
        assert!(edit.redo(&mut ui));
        assert_eq!(edit.get_text(), "abc");
    }

    #[test]
    fn test_paste_is_its_own_undo_entry() {
        let (edit, mut ui) = edit_and_ui();
        edit.insert_text_at_caret(&mut ui, "a");
        edit.insert_text_at_caret(&mut ui, "pasted");
        edit.insert_text_at_caret(&mut ui, "b");
        assert_eq!(edit.get_text(), "apastedb");
        edit.undo(&mut ui);
        assert_eq!(edit.get_text(), "apasted");
        edit.undo(&mut ui);
        assert_eq!(edit.get_text(), "a");
        edit.undo(&mut ui);
        assert_eq!(edit.get_text(), "");
    }

    #[test]
    fn test_caret_move_breaks_typing_coalescing() {
        let (edit, mut ui) = edit_and_ui();
        edit.insert_text_at_caret(&mut ui, "a");
        edit.insert_text_at_caret(&mut ui, "b");
        edit.break_undo_coalescing(); // as a click or arrow key would
        edit.insert_text_at_caret(&mut ui, "c");
        edit.undo(&mut ui);
        assert_eq!(edit.get_text(), "ab");
        edit.undo(&mut ui);
        assert_eq!(edit.get_text(), "");
    }

    #[test]
    fn test_set_text_clears_history() {
        let (edit, mut ui) = edit_and_ui();
        edit.insert_text_at_caret(&mut ui, "a");
        edit.set_text("fresh");
        assert!(!edit.undo(&mut ui));
        assert_eq!(edit.get_text(), "fresh");
    }

    #[test]
    fn test_new_edit_after_undo_clears_redo() {
        let (edit, mut ui) = edit_and_ui();
        edit.insert_text_at_caret(&mut ui, "a");
        edit.undo(&mut ui);
        edit.insert_text_at_caret(&mut ui, "x");
        assert!(!edit.redo(&mut ui));
        assert_eq!(edit.get_text(), "x");
    }

    #[test]
    fn test_undo_restores_selection_deleted_by_typing() {
        let (edit, mut ui) = edit_and_ui();
        edit.set_text("hello");
        edit.select_all();
        edit.insert_text_at_caret(&mut ui, "x"); // replaces selection
        assert_eq!(edit.get_text(), "x");
        edit.undo(&mut ui);
        assert_eq!(edit.get_text(), "hello");
        assert_eq!(edit.get_selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn test_password_blocks_clipboard_copy() {
        let (edit, mut ui) = edit_and_ui();
        edit.insert_text_at_caret(&mut ui, "secret");
        edit.set_password(true);
        assert!(edit.is_password());
        edit.select_all();
        // Early-returns before touching the system clipboard.
        edit.copy_to_clipboard();
    }

    #[test]
    fn test_numeric_filter_rejects_non_digits() {
        let (edit, mut ui) = edit_and_ui();
        edit.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit())));
        assert!(edit.insert_text_at_caret(&mut ui, "1"));
        assert!(!edit.insert_text_at_caret(&mut ui, "a"));
        assert!(edit.insert_text_at_caret(&mut ui, "2"));
        assert_eq!(edit.get_text(), "12");
    }

    #[test]
    fn test_filter_rejects_whole_paste_with_one_bad_char() {
        let (edit, mut ui) = edit_and_ui();
        edit.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit())));
        assert!(!edit.insert_text_at_caret(&mut ui, "12 34"));
        assert_eq!(edit.get_text(), "");
        assert!(edit.insert_text_at_caret(&mut ui, "1234"));
        assert_eq!(edit.get_text(), "1234");
    }

    #[test]
    fn test_rejected_insert_leaves_undo_history_alone() {
        let (edit, mut ui) = edit_and_ui();
        edit.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit())));
        edit.insert_text_at_caret(&mut ui, "1");
        edit.insert_text_at_caret(&mut ui, "a"); // rejected: no undo entry
        edit.insert_text_at_caret(&mut ui, "2"); // still coalesces with "1"
        assert!(edit.undo(&mut ui));
        assert_eq!(edit.get_text(), "");
        assert!(!edit.undo(&mut ui));
    }

    #[test]
    fn test_filter_xml_attributes() {
        let (mut edit, mut ui) = edit_and_ui();
        edit.set_any("filter", "numeric");
        assert!(!edit.insert_text_at_caret(&mut ui, "x"));
        assert!(edit.insert_text_at_caret(&mut ui, "7"));
        edit.set_any("allowed_chars", "0123456789abcdef");
        assert!(edit.insert_text_at_caret(&mut ui, "f"));
        assert!(!edit.insert_text_at_caret(&mut ui, "g"));
        assert_eq!(edit.get_text(), "7f");
    }

    #[test]
    fn test_set_text_bypasses_filter_and_clearing_lifts_it() {
        let (edit, mut ui) = edit_and_ui();
        edit.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit())));
        edit.set_text("not numeric");
        assert_eq!(edit.get_text(), "not numeric");
        edit.set_input_filter(None);
        assert!(edit.insert_text_at_caret(&mut ui, "!")); // caret still at 0
        assert_eq!(edit.get_text(), "!not numeric");
    }
}
