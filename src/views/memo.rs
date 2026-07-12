use std::cell::RefCell;
use std::cmp::{max, min};
use std::rc::Rc;
use std::time::Instant;
use crate::text::{TextAlignment, TextBlock, TextOptions};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseCursorType, MouseScrollDistance, VirtualKeyCode};

use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::common::{delete_char, delete_range, insert_str, InputFilter, TextEditOp, TextSnapshot, UNDO_LIMIT};
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

pub struct Memo {
    state: RefCell<FieldsTexted>,
    scroll_y: RefCell<i32>,
    caret_pos: RefCell<usize>,
    caret_rect: RefCell<Rect<i32>>,
    caret_time: RefCell<Instant>,
    caret_visible: RefCell<bool>,
    selection_anchor: RefCell<Option<usize>>,
    last_click_time: RefCell<Instant>,
    click_count: RefCell<u32>,
    placeholder: RefCell<String>,
    read_only: RefCell<bool>,
    max_length: RefCell<Option<usize>>,
    max_lines: RefCell<u32>,
    preferred_x: RefCell<Option<f32>>,
    held_key: RefCell<Option<VirtualKeyCode>>,
    held_shift: RefCell<bool>,
    held_ctrl: RefCell<bool>,
    key_repeat_time: RefCell<Instant>,
    key_repeat_started: RefCell<bool>,
    /// Cached line count from last layout, used for height calculation
    line_count: RefCell<u32>,
    /// Start char index of each visual line (rebuilt in layout_text)
    line_offsets: RefCell<Vec<usize>>,
    /// True while the left button is held after a press inside the text, so
    /// mouse-move extends the selection (even when the pointer leaves the view).
    dragging: RefCell<bool>,
    // Undo/redo: snapshots taken before each mutating operation.
    undo_stack: RefCell<Vec<TextSnapshot>>,
    redo_stack: RefCell<Vec<TextSnapshot>>,
    /// The kind of the last mutation, for coalescing runs. Reset by caret
    /// moves so the next edit starts a fresh undo entry.
    last_edit_op: std::cell::Cell<Option<TextEditOp>>,
    /// Per-character input filter: when set, a typed or pasted insert
    /// containing any disallowed character is rejected wholesale.
    /// Programmatic set_text() bypasses it.
    input_filter: RefCell<Option<InputFilter>>,
}

impl HasMainFields for Memo {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for Memo {}

#[allow(dead_code)]
impl Memo {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> Memo {
        let mut fields = FieldsTexted {
            main: FieldsMain::with_rect(rect, Dimension::Max, Dimension::Min),
            text: text.to_owned(),
            text_size,
            line_height: 0f32,
            single_line: false,
            cached_text: None,
            font: FontSelector::new()
        };
        fields.main.padding = Borders::with_padding(4);
        Memo {
            state: RefCell::new(fields),
            scroll_y: RefCell::new(0),
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
            max_lines: RefCell::new(5),
            preferred_x: RefCell::new(None),
            held_key: RefCell::new(None),
            held_shift: RefCell::new(false),
            held_ctrl: RefCell::new(false),
            key_repeat_time: RefCell::new(Instant::now()),
            key_repeat_started: RefCell::new(false),
            line_count: RefCell::new(1),
            line_offsets: RefCell::new(vec![0]),
            dragging: RefCell::new(false),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            last_edit_op: std::cell::Cell::new(None),
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
        *self.scroll_y.borrow_mut() = 0;
        self.caret_rect.borrow_mut().clear();
        let scale = self.state.borrow().main.scale;
        self.layout_text(self.get_rect_width(), scale);
    }

    /// Clear text and reset to initial single-line state. Call `ui.relayout()` after this.
    pub fn reset(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.text.clear();
            state.cached_text = None;
        }
        *self.caret_pos.borrow_mut() = 0;
        *self.selection_anchor.borrow_mut() = None;
        *self.scroll_y.borrow_mut() = 0;
        *self.line_count.borrow_mut() = 1;
        *self.line_offsets.borrow_mut() = vec![0];
        self.caret_rect.borrow_mut().clear();
        *self.preferred_x.borrow_mut() = None;
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

    pub fn is_read_only(&self) -> bool {
        *self.read_only.borrow()
    }

    pub fn set_max_length(&self, max_length: Option<usize>) {
        *self.max_length.borrow_mut() = max_length;
    }

    /// Restrict typed and pasted input. The predicate judges each character;
    /// an insert containing any disallowed character is rejected wholesale
    /// (a paste with one stray character inserts nothing). The predicate sees
    /// `'\n'` too, so a restrictive filter also blocks Enter unless it allows
    /// newlines. Programmatic `set_text()` is not filtered. Pass `None` to
    /// remove the filter.
    pub fn set_input_filter(&self, filter: Option<InputFilter>) {
        *self.input_filter.borrow_mut() = filter;
    }

    fn passes_filter(&self, s: &str) -> bool {
        match self.input_filter.borrow().as_ref() {
            Some(filter) => s.chars().all(filter),
            None => true,
        }
    }

    pub fn set_max_lines(&self, max_lines: u32) {
        *self.max_lines.borrow_mut() = max_lines.max(1);
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

    fn layout_text(&self, width: i32, scale: f64) {
        let state = self.state.borrow();
        let typeface = state.main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let padding = state.main.padding.scaled(state.main.scale);
                let available_width = (width - padding.left - padding.right).max(1) as f32;
                let options = TextOptions::new().with_wrap_to_width(available_width, TextAlignment::Left);
                let text_str = if state.text.is_empty() { " " } else { &state.text };
                // text_size is dips, like an explicit font_size — both scale.
                let base_size = typeface.font_size
                    .unwrap_or(state.text_size) * scale as f32;
                let text = font.layout_text(text_str, base_size, options);

                // Build line_offsets: start char index of each visual line
                let chars: Vec<char> = state.text.chars().collect();
                let mut offsets = Vec::new();
                let mut char_offset = 0usize;
                for line in text.iter_lines() {
                    offsets.push(char_offset);
                    let glyph_count = line.iter_glyphs().count();
                    char_offset += glyph_count;
                    // Skip newline char between lines (not a glyph)
                    if char_offset < chars.len() && chars[char_offset] == '\n' {
                        char_offset += 1;
                    }
                }
                // If text ends with '\n', there's a virtual empty line after it
                if !state.text.is_empty() && state.text.ends_with('\n') {
                    offsets.push(chars.len());
                }
                if offsets.is_empty() {
                    offsets.push(0);
                }

                let line_count = offsets.len() as u32;
                *self.line_count.borrow_mut() = line_count;
                *self.line_offsets.borrow_mut() = offsets;
                drop(state);
                let mut state = self.state.borrow_mut();
                // The cached line height is scale-dependent; drop it so the
                // next `get_line_height` recomputes at the scale this layout
                // ran at (a window can move to a differently-scaled monitor).
                state.line_height = 0f32;
                if state.text.is_empty() {
                    state.cached_text = None;
                } else {
                    state.cached_text = Some(text);
                }
                return;
            }
        }
        drop(state);
        let mut state = self.state.borrow_mut();
        state.line_height = 0f32;
        state.cached_text = None;
    }

    fn layout_placeholder_text(&self) -> Option<TextBlock> {
        let placeholder = self.placeholder.borrow();
        if placeholder.is_empty() {
            return None;
        }
        let state = self.state.borrow();
        let typeface = state.main.font_manager.get();
        if let Some(typeface) = typeface {
            if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
                let padding = state.main.padding.scaled(state.main.scale);
                let available_width = (state.main.rect.width() - padding.left - padding.right).max(1) as f32;
                let options = TextOptions::new().with_wrap_to_width(available_width, TextAlignment::Left);
                let base_size = typeface.font_size
                    .unwrap_or(state.text_size) * state.main.scale as f32;
                return Some(font.layout_text(&placeholder, base_size, options));
            }
        }
        None
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
        self.state.borrow().line_height
    }

    /// Map a flat char index to (visual_line_index, x_pixel_within_line).
    /// Uses line_offsets built during layout_text.
    fn pos_to_line_and_x(&self, pos: usize) -> (usize, f32) {
        let offsets = self.line_offsets.borrow();

        // Find which line: last line whose start offset <= pos
        let mut line_idx = 0;
        for i in (0..offsets.len()).rev() {
            if offsets[i] <= pos {
                line_idx = i;
                break;
            }
        }

        let line_start = offsets[line_idx];
        let pos_in_line = pos - line_start;

        let state = self.state.borrow();
        if let Some(text) = &state.cached_text {
            let visual_line_count = text.iter_lines().count();
            if line_idx >= visual_line_count {
                // Virtual empty line after trailing \n
                return (line_idx, 0.0);
            }

            if let Some(line) = text.iter_lines().nth(line_idx) {
                if pos_in_line == 0 {
                    return (line_idx, 0.0);
                }
                for (i, glyph) in line.iter_glyphs().enumerate() {
                    if i == pos_in_line - 1 {
                        return (line_idx, glyph.position_x() + glyph.advance_width());
                    }
                }
                // pos_in_line > glyph count: return end of line
                let x = line.iter_glyphs().last()
                    .map(|g| g.position_x() + g.advance_width())
                    .unwrap_or(0.0);
                return (line_idx, x);
            }
        }

        (line_idx, 0.0)
    }

    /// Convert a pixel point (relative to text area top-left) to a char position
    fn pos_from_point(&self, x: f32, y: f32) -> usize {
        let line_height = self.get_line_height();
        if line_height <= 0.0 {
            return 0;
        }

        let offsets = self.line_offsets.borrow();
        let total_lines = offsets.len();
        let target_line = ((y / line_height).floor() as usize).min(total_lines.saturating_sub(1));
        let line_start = offsets[target_line];

        let state = self.state.borrow();
        let chars: Vec<char> = state.text.chars().collect();

        if let Some(text) = &state.cached_text {
            let visual_line_count = text.iter_lines().count();
            if target_line >= visual_line_count {
                // Virtual empty line after trailing \n
                return line_start;
            }

            if let Some(line) = text.iter_lines().nth(target_line) {
                for (i, glyph) in line.iter_glyphs().enumerate() {
                    let glyph_left = glyph.position_x();
                    let glyph_right = glyph_left + glyph.advance_width();
                    let mid = (glyph_left + glyph_right) / 2.0;
                    if x < mid {
                        return line_start + i;
                    }
                }
                // Past end of glyphs on this line
                let glyph_count = line.iter_glyphs().count();
                return line_start + glyph_count;
            }
        }

        // No cached text: return line start
        line_start.min(chars.len())
    }

    fn update_caret_rect(&self, scale: f64) {
        let padding = self.get_padding(scale);
        let caret_pos = *self.caret_pos.borrow();
        let my_rect = self.state.borrow().main.rect;
        let line_height = self.get_line_height();

        let (line_idx, x_in_line) = self.pos_to_line_and_x(caret_pos);
        let caret_y = line_idx as f32 * line_height;

        let rect = Rect {
            min: Point {
                x: my_rect.min.x + padding.left + x_in_line.round() as i32,
                y: my_rect.min.y + padding.top + caret_y.round() as i32,
            },
            max: Point {
                x: my_rect.min.x + padding.left + x_in_line.round() as i32 + (crate::drawing::current_dimension("caret.width") as f64 * scale) as i32,
                y: my_rect.min.y + padding.top + (caret_y + line_height).round() as i32,
            },
        };

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
        let padding = self.get_padding(scale);
        let view_top = my_rect.min.y + padding.top;
        let view_bottom = my_rect.max.y - padding.bottom;
        let visible_height = (view_bottom - view_top).max(0);

        // Total text height including virtual trailing-newline lines
        let line_height = self.get_line_height();
        let total_lines = *self.line_count.borrow();
        let total_text_height = (total_lines as f32 * line_height).ceil() as i32;

        // If all content fits, no scrolling needed
        if total_text_height <= visible_height {
            *self.scroll_y.borrow_mut() = 0;
            return;
        }

        let caret_rect = self.get_caret_rect(scale);

        if caret_rect.max.y + *self.scroll_y.borrow() > view_bottom {
            *self.scroll_y.borrow_mut() = view_bottom - caret_rect.max.y;
        } else if caret_rect.min.y + *self.scroll_y.borrow() < view_top {
            *self.scroll_y.borrow_mut() = view_top - caret_rect.min.y;
        }

        // Clamp scroll to valid range
        let max_scroll = (total_text_height - visible_height).max(0);
        let scroll = *self.scroll_y.borrow();
        *self.scroll_y.borrow_mut() = scroll.min(0).max(-max_scroll);
    }

    /// Get the visible height based on line count and max_lines
    fn get_visible_height(&self) -> i32 {
        let line_height = self.get_line_height();
        let line_count = *self.line_count.borrow();
        let max_lines = *self.max_lines.borrow();
        let visible_lines = line_count.min(max_lines);
        (visible_lines as f32 * line_height).ceil() as i32
    }

    fn word_start(&self, pos: usize) -> usize {
        let text = self.state.borrow().text.clone();
        let chars: Vec<char> = text.chars().collect();
        if pos == 0 {
            return 0;
        }
        let mut p = pos;
        while p > 0 && !chars[p - 1].is_alphanumeric() {
            p -= 1;
        }
        while p > 0 && chars[p - 1].is_alphanumeric() {
            p -= 1;
        }
        p
    }

    fn word_end(&self, pos: usize) -> usize {
        let text = self.state.borrow().text.clone();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if pos >= len {
            return len;
        }
        let mut p = pos;
        while p < len && chars[p].is_alphanumeric() {
            p += 1;
        }
        while p < len && !chars[p].is_alphanumeric() {
            p += 1;
        }
        p
    }

    fn word_end_only(&self, pos: usize) -> usize {
        let text = self.state.borrow().text.clone();
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if pos >= len {
            return len;
        }
        let mut p = pos;
        if !chars[p].is_alphanumeric() {
            while p < len && !chars[p].is_alphanumeric() {
                p += 1;
            }
            return p;
        }
        while p < len && chars[p].is_alphanumeric() {
            p += 1;
        }
        p
    }

    /// Get the start char index of a given visual line
    fn line_start_pos(&self, target_line: usize) -> usize {
        let offsets = self.line_offsets.borrow();
        if target_line < offsets.len() {
            offsets[target_line]
        } else {
            self.state.borrow().text.chars().count()
        }
    }

    /// Get the end char index of a given visual line (exclusive of newline)
    fn line_end_pos(&self, target_line: usize) -> usize {
        let offsets = self.line_offsets.borrow();
        if target_line + 1 < offsets.len() {
            let next_start = offsets[target_line + 1];
            let state = self.state.borrow();
            let chars: Vec<char> = state.text.chars().collect();
            // If there's a \n before next line, exclude it
            if next_start > 0 && next_start <= chars.len() && chars[next_start - 1] == '\n' {
                next_start - 1
            } else {
                next_start
            }
        } else {
            self.state.borrow().text.chars().count()
        }
    }

    /// Get total number of visual lines (including virtual trailing-newline lines)
    fn total_lines(&self) -> usize {
        self.line_offsets.borrow().len().max(1)
    }

    fn handle_nav_key(&self, ui: &mut UI, code: VirtualKeyCode, shift: bool, ctrl: bool) -> bool {
        self.break_undo_coalescing();
        match code {
            VirtualKeyCode::Left => {
                *self.preferred_x.borrow_mut() = None;
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
                *self.preferred_x.borrow_mut() = None;
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
            VirtualKeyCode::Up => {
                self.begin_or_extend_selection(shift);
                let pos = *self.caret_pos.borrow();
                let (line_idx, x) = self.pos_to_line_and_x(pos);

                let px = self.preferred_x.borrow().unwrap_or(x);
                *self.preferred_x.borrow_mut() = Some(px);

                if line_idx > 0 {
                    let line_height = self.get_line_height();
                    let target_y = (line_idx as f32 - 0.5) * line_height;
                    let new_pos = self.pos_from_point(px, target_y);
                    *self.caret_pos.borrow_mut() = new_pos;
                } else {
                    *self.caret_pos.borrow_mut() = 0;
                }
                if !shift {
                    self.clear_selection();
                }
                self.collapse_if_empty();
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::Down => {
                self.begin_or_extend_selection(shift);
                let pos = *self.caret_pos.borrow();
                let (line_idx, x) = self.pos_to_line_and_x(pos);
                let total = self.total_lines();

                let px = self.preferred_x.borrow().unwrap_or(x);
                *self.preferred_x.borrow_mut() = Some(px);

                if line_idx + 1 < total {
                    let line_height = self.get_line_height();
                    let target_y = (line_idx as f32 + 1.5) * line_height;
                    let new_pos = self.pos_from_point(px, target_y);
                    *self.caret_pos.borrow_mut() = new_pos;
                } else {
                    let text_len = self.state.borrow().text.chars().count();
                    *self.caret_pos.borrow_mut() = text_len;
                }
                if !shift {
                    self.clear_selection();
                }
                self.collapse_if_empty();
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::Home => {
                *self.preferred_x.borrow_mut() = None;
                if !shift && self.has_selection() {
                    self.clear_selection();
                } else {
                    self.begin_or_extend_selection(shift);
                }
                if ctrl {
                    *self.caret_pos.borrow_mut() = 0;
                } else {
                    let pos = *self.caret_pos.borrow();
                    let (line_idx, _) = self.pos_to_line_and_x(pos);
                    *self.caret_pos.borrow_mut() = self.line_start_pos(line_idx);
                }
                if !shift {
                    self.clear_selection();
                }
                self.collapse_if_empty();
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::End => {
                *self.preferred_x.borrow_mut() = None;
                if !shift && self.has_selection() {
                    self.clear_selection();
                } else {
                    self.begin_or_extend_selection(shift);
                }
                if ctrl {
                    let new_pos = self.state.borrow().text.chars().count();
                    *self.caret_pos.borrow_mut() = new_pos;
                } else {
                    let pos = *self.caret_pos.borrow();
                    let (line_idx, _) = self.pos_to_line_and_x(pos);
                    *self.caret_pos.borrow_mut() = self.line_end_pos(line_idx);
                }
                if !shift {
                    self.clear_selection();
                }
                self.collapse_if_empty();
                self.caret_rect.borrow_mut().clear();
                true
            }
            VirtualKeyCode::Delete => {
                *self.preferred_x.borrow_mut() = None;
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
        // Request relayout since height may have changed
        ui.relayout();
    }

    fn fire_text_changed(&self, ui: &mut UI) {
        self.base_fire_event(ui, EventType::TextChanged, &EventData::None);
    }

    fn insert_text_at_caret(&self, ui: &mut UI, s: &str) -> bool {
        if *self.read_only.borrow() || s.is_empty() || !self.passes_filter(s) {
            return false;
        }
        // Single chars are typing (coalesced); newlines and pastes are not.
        self.remember_for_undo(if s.chars().count() == 1 && s != "
" { TextEditOp::Typing } else { TextEditOp::Other });
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
        *self.preferred_x.borrow_mut() = None;
        self.on_text_changed(ui);
        true
    }

    fn copy_to_clipboard(&self) {
        if let Some(text) = self.get_selected_text() {
            crate::clipboard::set_text(&text);
        }
    }

    fn paste_from_clipboard(&self, ui: &mut UI) -> bool {
        if *self.read_only.borrow() {
            return false;
        }
        if let Some(text) = crate::clipboard::get_text() {
            return self.insert_text_at_caret(ui, &text);
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

        let memo_id = self.get_id();
        menu.on_event(EventType::Click, Box::new(move |ui: &mut UI, view: &dyn View, _data: &EventData| {
            let menu = view.as_any().downcast_ref::<PopupMenu>().unwrap();
            let index = menu.get_hovered_index();
            if let Some(index) = index {
                let mut need_text_changed = false;
                let mut need_paste = false;

                match index {
                    0 => {
                        if let Some(el) = ui.get_view(&memo_id) {
                            let b = el.borrow();
                            let memo = b.as_any().downcast_ref::<Memo>().unwrap();
                            memo.copy_to_clipboard();
                            if memo.has_selection() && !*memo.read_only.borrow() {
                                memo.remember_for_undo(TextEditOp::Other);
                                memo.delete_selection();
                                need_text_changed = true;
                            }
                        }
                    }
                    1 => {
                        if let Some(el) = ui.get_view(&memo_id) {
                            let b = el.borrow();
                            let memo = b.as_any().downcast_ref::<Memo>().unwrap();
                            memo.copy_to_clipboard();
                        }
                    }
                    2 => {
                        need_paste = true;
                    }
                    3 => {
                        if let Some(el) = ui.get_view(&memo_id) {
                            let b = el.borrow();
                            let memo = b.as_any().downcast_ref::<Memo>().unwrap();
                            if memo.has_selection() && !*memo.read_only.borrow() {
                                memo.remember_for_undo(TextEditOp::Other);
                                memo.delete_selection();
                                need_text_changed = true;
                            }
                        }
                    }
                    5 => {
                        if let Some(el) = ui.get_view(&memo_id) {
                            let b = el.borrow();
                            let memo = b.as_any().downcast_ref::<Memo>().unwrap();
                            memo.select_all();
                        }
                    }
                    _ => {}
                }

                if need_paste {
                    if let Some(el) = ui.get_view(&memo_id) {
                        let b = el.borrow();
                        let memo = b.as_any().downcast_ref::<Memo>().unwrap();
                        memo.paste_from_clipboard(ui);
                    }
                }
                if need_text_changed {
                    if let Some(el) = ui.get_view(&memo_id) {
                        let b = el.borrow();
                        let memo = b.as_any().downcast_ref::<Memo>().unwrap();
                        memo.on_text_changed(ui);
                    }
                }
            }
            true
        }));

        let element: Element = Rc::new(RefCell::new(menu));
        ui.show_popup(element, x, y, PopupDirection::BottomRight, PopupMode::Popup);
    }

    /// Paint selection highlight across multiple lines.
    /// Returns the rectangles that were filled, for the contrast text overlay.
    fn paint_selection(&self, theme: &mut dyn Theme, text_rect: Rect<i32>, scroll_y: i32) -> Vec<Rect<i32>> {
        let mut rects = Vec::new();
        if let Some(anchor) = *self.selection_anchor.borrow() {
            let caret = *self.caret_pos.borrow();
            if anchor == caret {
                return rects;
            }
            let sel_start = min(anchor, caret);
            let sel_end = max(anchor, caret);

            let (start_line, start_x) = self.pos_to_line_and_x(sel_start);
            let (end_line, end_x) = self.pos_to_line_and_x(sel_end);
            let line_height = self.get_line_height();

            for line in start_line..=end_line {
                let y_top = text_rect.min.y + (line as f32 * line_height).round() as i32 + scroll_y;
                let y_bottom = text_rect.min.y + ((line + 1) as f32 * line_height).round() as i32 + scroll_y;

                let x_left = if line == start_line {
                    text_rect.min.x + start_x.round() as i32
                } else {
                    text_rect.min.x
                };

                let x_right = if line == end_line {
                    text_rect.min.x + end_x.round() as i32
                } else {
                    text_rect.max.x
                };

                if y_bottom > text_rect.min.y && y_top < text_rect.max.y {
                    let sel_rect = rect(
                        (x_left, y_top.max(text_rect.min.y)),
                        (x_right, y_bottom.min(text_rect.max.y)),
                    );
                    theme.draw_rect(sel_rect, theme.color("selection"));
                    rects.push(sel_rect);
                }
            }
        }
        rects
    }
}

impl View for Memo {
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
            "maxlength" => {
                if let Ok(n) = value.parse::<usize>() {
                    self.set_max_length(Some(n));
                }
            }
            "max_lines" => {
                if let Ok(n) = value.parse::<u32>() {
                    self.set_max_lines(n);
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
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface));
        self.base_set_scale(scale);

        let (new_width, new_height) = self.calculate_size(width, height, scale);
        let padding = self.get_padding(scale);

        // Determine actual width first (needed for wrapping)
        let actual_width = {
            let state = self.state.borrow();
            match &state.main.width {
                Dimension::Min => new_width,
                Dimension::Max => new_width,
                Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => (width as f32 * p / 100f32).round() as i32,
            }
        };

        // Layout text with wrapping at actual width
        self.layout_text(actual_width, scale);

        // Height: min means grow to fit content up to max_lines
        let content_height = self.get_visible_height();
        let actual_height = {
            let state = self.state.borrow();
            match &state.main.height {
                Dimension::Min => content_height + padding.top + padding.bottom,
                Dimension::Max => new_height,
                Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => (height as f32 * p / 100f32).round() as i32,
            }
        };

        let rect = rect((x, y), (x + actual_width, y + actual_height));
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
        self.update_scroll();
        let state = self.state.borrow();
        let scroll_y = *self.scroll_y.borrow();
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
        theme.push_clip();
        theme.clip_rect(text_rect);

        let has_text = state.cached_text.is_some();
        drop(state);

        if has_text {
            let text_x = text_rect.min.x as f32;
            let text_y = text_rect.min.y as f32 + scroll_y as f32;

            // Draw selection
            let sel_rects = self.paint_selection(theme, text_rect, scroll_y);

            // Draw text
            let state = self.state.borrow();
            if let Some(text) = &state.cached_text {
                let color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
                theme.draw_text(text_x, text_y, color, text);
                // Redraw the selected part in a contrasting color over the highlight
                if !sel_rects.is_empty() {
                    let sel_color = crate::themes::selection_text_color(theme.color("selection"));
                    for sel_rect in sel_rects {
                        theme.draw_text_cropped(text_x, text_y, sel_rect, sel_color, text);
                    }
                }
            }
        } else if !self.placeholder.borrow().is_empty() {
            if let Some(placeholder_text) = self.layout_placeholder_text() {
                theme.draw_text(
                    text_rect.min.x as f32,
                    text_rect.min.y as f32,
                    theme.color("text_hint"),
                    &placeholder_text,
                );
            }
        }
        theme.pop_clip();

        // Step 3: Draw borders
        let state = self.state.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);
        theme.push_clip();
        theme.clip_rect(rect);
        theme.draw_component("edit.body", rect, state.main.state);
        theme.pop_clip();

        // Step 4: Draw caret
        if state.main.state.focused && *self.caret_visible.borrow() {
            let mut caret_rect = self.get_caret_rect(state.main.scale);
            caret_rect.move_by(origin);
            caret_rect.move_by((0, scroll_y));
            // Only draw if caret is within visible area
            let view_top = rect.min.y + padding.top;
            let view_bottom = rect.max.y - padding.bottom;
            if caret_rect.max.y > view_top && caret_rect.min.y < view_bottom {
                theme.draw_component("edit.caret", caret_rect, state.main.state);
            }
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
        let visible_height = self.get_visible_height();
        let state = self.state.borrow();
        match &state.cached_text {
            None => {
                (BUTTON_MIN_WIDTH, max(line_height, visible_height))
            },
            Some(text) => {
                let width = max(text.width().round() as i32, BUTTON_MIN_WIDTH);
                (width, max(visible_height, line_height))
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

    fn get_tooltip(&self) -> Option<String> {
        self.base_get_tooltip()
    }

    fn get_content_description(&self) -> Option<String> {
        self.base_get_content_description()
    }

    fn set_content_description(&mut self, description: Option<String>) {
        self.base_set_content_description(description);
    }

    fn get_labelled_by(&self) -> Option<String> {
        self.base_get_labelled_by()
    }

    fn set_labelled_by(&mut self, view_id: Option<String>) {
        self.base_set_labelled_by(view_id);
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

    fn on_event(&mut self, event: EventType, func: EventCallback) {
        self.base_on_event(event, func);
    }

    fn has_listener(&self, event: EventType) -> bool {
        self.base_has_listener(event)
    }

    fn fire_event(&self, ui: &mut UI, event: EventType, data: &EventData) -> bool {
        self.base_fire_event(ui, event, data)
    }

    fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(accesskit::Role::MultilineTextInput);
        node.set_value(self.get_text());
        if self.is_read_only() {
            node.set_read_only();
        }
        // Caret/selection, expressed against the per-line text runs below.
        let id = self.get_id();
        let position = |pos: usize| {
            let (line, _) = self.pos_to_line_and_x(pos);
            let start = self.line_offsets.borrow().get(line).copied().unwrap_or(0);
            accesskit::TextPosition {
                node: crate::accessibility::item_node_id(&id, line),
                character_index: pos.saturating_sub(start),
            }
        };
        let caret = *self.caret_pos.borrow();
        let anchor = self.selection_anchor.borrow().unwrap_or(caret);
        node.set_text_selection(accesskit::TextSelection {
            anchor: position(anchor),
            focus: position(caret),
        });
        node
    }

    /// One `TextRun` per visual (wrapped) line. A hard line break belongs to
    /// the end of its line's run, counted as one character — exactly how
    /// `line_offsets` already assigns char ranges to lines.
    fn accessibility_children(&self) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        let id = self.get_id();
        let chars: Vec<char> = self.get_text().chars().collect();
        let offsets = self.line_offsets.borrow().clone();
        let line_height = self.get_line_height();
        let scroll_y = *self.scroll_y.borrow();
        let state = self.state.borrow();
        let scale = state.main.scale;
        let rect = state.main.rect;
        let padding = self.get_padding(scale);

        let line_count = offsets.len().max(1);
        let mut result = Vec::with_capacity(line_count);
        for i in 0..line_count {
            let start = offsets.get(i).copied().unwrap_or(0);
            let end = offsets.get(i + 1).copied().unwrap_or(chars.len());
            let line_text: String = chars.get(start..end).unwrap_or(&[]).iter().collect();

            let mut node = accesskit::Node::new(accesskit::Role::TextRun);
            node.set_character_lengths(crate::accessibility::character_lengths(&line_text));
            node.set_word_starts(crate::accessibility::word_starts(&line_text));

            // Per-character geometry from the laid-out block; a hard break is
            // not a glyph, so it gets a zero-width position after the last
            // glyph (where an end-of-paragraph marker would render).
            let ends_with_break = line_text.ends_with('\n');
            let char_count = end.saturating_sub(start);
            if let Some(block) = &state.cached_text
                && let Some(line) = block.iter_lines().nth(i)
            {
                let mut positions: Vec<f32> = line.iter_glyphs().map(|g| g.position_x()).collect();
                let mut widths: Vec<f32> = line.iter_glyphs().map(|g| g.advance_width()).collect();
                if ends_with_break {
                    let line_end = positions.last().copied().unwrap_or(0.0) + widths.last().copied().unwrap_or(0.0);
                    positions.push(line_end);
                    widths.push(0.0);
                }
                if positions.len() == char_count {
                    node.set_character_positions(positions);
                    node.set_character_widths(widths);
                }
            }
            node.set_value(line_text);

            // View-local; the tree builder translates to window space.
            let y0 = padding.top as f32 + i as f32 * line_height + scroll_y as f32;
            node.set_bounds(accesskit::Rect {
                x0: padding.left as f64,
                y0: f64::from(y0),
                x1: (rect.width() - padding.right) as f64,
                y1: f64::from(y0 + line_height),
            });
            result.push((crate::accessibility::item_node_id(&id, i), node));
        }
        result
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.base_fire_event(ui, EventType::Click, &EventData::None)
    }

    fn update(&mut self, ui: &mut UI) -> bool {
        let focused = self.state.borrow().main.state.focused;
        let mut redraw = false;

        if focused {
            let elapsed = self.caret_time.borrow().elapsed().as_millis();
            if elapsed >= 500 {
                let visible = *self.caret_visible.borrow();
                *self.caret_visible.borrow_mut() = !visible;
                *self.caret_time.borrow_mut() = Instant::now();
                redraw = true;
            }

            let held = *self.held_key.borrow();
            if let Some(code) = held {
                let repeat_elapsed = self.key_repeat_time.borrow().elapsed().as_millis();
                let started = *self.key_repeat_started.borrow();
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

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        // Drag-selection: extend regardless of whether the pointer is still
        // inside the view (the parent dispatches moves to every child).
        if *self.dragging.borrow() {
            let scale = self.state.borrow().main.scale;
            let padding = self.get_padding(scale);
            let my_rect = self.state.borrow().main.rect;
            let scroll_y = *self.scroll_y.borrow();
            let move_x = (position.x - my_rect.min.x - padding.left) as f32;
            let move_y = (position.y - my_rect.min.y - padding.top - scroll_y) as f32;
            let char_pos = self.pos_from_point(move_x, move_y);
            *self.caret_pos.borrow_mut() = char_pos;
            self.caret_rect.borrow_mut().clear();
            self.update_scroll();
            return true;
        }

        let hit = self.state.borrow().main.rect.hit((position.x, position.y));
        if hit {
            ui.request_cursor(MouseCursorType::Text);
        }
        let old_state = self.state.borrow().main.state;
        self.state.borrow_mut().main.state.hovered = hit;
        self.state.borrow().main.state != old_state
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        self.break_undo_coalescing();
        if !self.state.borrow().main.rect.hit((position.x, position.y)) {
            return false;
        }

        if !matches!(button, MouseButton::Left) {
            self.state.borrow_mut().main.state.focused = true;
            if matches!(button, MouseButton::Right) && !ui.context_menu_suppressed() {
                self.open_context_menu(ui, position.x, position.y);
            }
            return true;
        }

        self.state.borrow_mut().main.state.pressed = true;
        self.state.borrow_mut().main.state.focused = true;

        let scale = self.state.borrow().main.scale;
        let padding = self.get_padding(scale);
        let my_rect = self.state.borrow().main.rect;
        let scroll_y = *self.scroll_y.borrow();
        let click_x = (position.x - my_rect.min.x - padding.left) as f32;
        let click_y = (position.y - my_rect.min.y - padding.top - scroll_y) as f32;
        let char_pos = self.pos_from_point(click_x, click_y);

        // Multi-click detection
        let elapsed = self.last_click_time.borrow().elapsed().as_millis();
        let prev_count = *self.click_count.borrow();

        if elapsed < DOUBLE_CLICK_MS && prev_count >= 1 {
            let new_count = prev_count + 1;
            *self.click_count.borrow_mut() = new_count;
            *self.last_click_time.borrow_mut() = Instant::now();

            if new_count == 2 {
                let ws = self.word_start(char_pos);
                let we = self.word_end_only(char_pos);
                *self.selection_anchor.borrow_mut() = Some(ws);
                *self.caret_pos.borrow_mut() = we;
                self.caret_rect.borrow_mut().clear();
                return true;
            } else if new_count >= 3 {
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
        *self.preferred_x.borrow_mut() = None;
        *self.dragging.borrow_mut() = true;
        self.caret_rect.borrow_mut().clear();
        true
    }

    fn on_mouse_button_up(&self, _ui: &mut UI, _position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        // End any drag-selection; collapse a zero-length anchor from a plain click.
        if *self.dragging.borrow() {
            *self.dragging.borrow_mut() = false;
            self.collapse_if_empty();
        }
        if matches!(button, MouseButton::Left) && self.state.borrow().main.state.pressed {
            self.state.borrow_mut().main.state.pressed = false;
            return true;
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
        if !self.state.borrow().main.rect.hit((position.x, position.y)) {
            return false;
        }
        let line_height = self.get_line_height();
        let delta = match distance {
            MouseScrollDistance::Lines { x: _, y, z: _ } => (y * line_height as f64) as i32,
            MouseScrollDistance::Pixels { x: _, y, z: _ } => y as i32,
            MouseScrollDistance::Pages { x: _, y, z: _ } => {
                let visible_height = self.get_visible_height();
                (y * visible_height as f64) as i32
            }
        };
        if delta == 0 {
            return false;
        }

        let total_height = {
            let state = self.state.borrow();
            state.cached_text.as_ref().map(|t| t.height().ceil() as i32).unwrap_or(0)
        };
        let visible_height = self.get_visible_height();
        let max_scroll = (total_height - visible_height).max(0);

        let mut scroll = *self.scroll_y.borrow();
        scroll += delta;
        scroll = scroll.min(0).max(-max_scroll);
        *self.scroll_y.borrow_mut() = scroll;
        true
    }

    fn on_key_down(&self, ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, state: ModifiersState) -> bool {
        if !self.base_is_enabled() { return false; }
        if let Some(code) = virtual_key_code {
            let shift = state.shift();
            let ctrl = state.ctrl();

            // Track repeatable keys
            match code {
                VirtualKeyCode::Left | VirtualKeyCode::Right |
                VirtualKeyCode::Up | VirtualKeyCode::Down |
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
                VirtualKeyCode::Up | VirtualKeyCode::Down |
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
                VirtualKeyCode::Return if shift => {
                    // Shift+Enter: insert newline
                    return self.insert_text_at_caret(ui, "\n");
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
        if state.ctrl() {
            return false;
        }

        // Ignore control characters, but allow through \n when shift is held (handled by on_key_down)
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
                let new_text = delete_char(&self.state.borrow().text, pos - 1);
                self.state.borrow_mut().text = new_text;
                *self.caret_pos.borrow_mut() = pos - 1;
                *self.preferred_x.borrow_mut() = None;
                self.on_text_changed(ui);
                return true;
            }
            return false;
        }

        // Handle delete
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

impl Default for Memo {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        Memo::new(rect, "", crate::drawing::current_text_size("text"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memo_and_ui() -> (Memo, UI) {
        let ui = UI::new(800, 600, Typeface::default(), 1.0);
        let memo = Memo::new(rect((0, 0), (200, 100)), "", 16.0);
        (memo, ui)
    }

    #[test]
    fn test_memo_typing_run_coalesces() {
        let (memo, mut ui) = memo_and_ui();
        memo.insert_text_at_caret(&mut ui, "a");
        memo.insert_text_at_caret(&mut ui, "b");
        assert!(memo.undo(&mut ui));
        assert_eq!(memo.get_text(), "");
        assert!(memo.redo(&mut ui));
        assert_eq!(memo.get_text(), "ab");
    }

    #[test]
    fn test_memo_newline_is_own_undo_entry() {
        let (memo, mut ui) = memo_and_ui();
        memo.insert_text_at_caret(&mut ui, "a");
        memo.insert_text_at_caret(&mut ui, "\n");
        memo.insert_text_at_caret(&mut ui, "b");
        assert_eq!(memo.get_text(), "a\nb");
        memo.undo(&mut ui);
        assert_eq!(memo.get_text(), "a\n");
        memo.undo(&mut ui);
        assert_eq!(memo.get_text(), "a");
    }

    #[test]
    fn test_memo_set_text_clears_history() {
        let (memo, mut ui) = memo_and_ui();
        memo.insert_text_at_caret(&mut ui, "a");
        memo.set_text("fresh");
        assert!(!memo.undo(&mut ui));
        assert_eq!(memo.get_text(), "fresh");
    }

    #[test]
    fn test_memo_filter_rejects_insert_including_newline() {
        let (memo, mut ui) = memo_and_ui();
        memo.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit())));
        assert!(memo.insert_text_at_caret(&mut ui, "1"));
        assert!(!memo.insert_text_at_caret(&mut ui, "x"));
        // The predicate sees '\n' too — Enter is blocked by a digits-only filter.
        assert!(!memo.insert_text_at_caret(&mut ui, "\n"));
        assert_eq!(memo.get_text(), "1");
    }

    #[test]
    fn test_memo_filter_allowing_newline_passes_enter() {
        let (memo, mut ui) = memo_and_ui();
        memo.set_input_filter(Some(Box::new(|c: char| c.is_ascii_digit() || c == '\n')));
        assert!(memo.insert_text_at_caret(&mut ui, "1"));
        assert!(memo.insert_text_at_caret(&mut ui, "\n"));
        assert!(memo.insert_text_at_caret(&mut ui, "2"));
        assert_eq!(memo.get_text(), "1\n2");
    }
}
