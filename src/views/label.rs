use std::cell::RefCell;
use std::rc::Rc;

use crate::text::{TextAlignment, TextBlock, TextOptions};
use crate::input::{MouseButton, MouseCursorType};
use crate::assets::get_font_family;
use crate::events::{EventCallback, EventData, EventType};
use crate::image_source::ImageSource;

use crate::themes::{Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::{PopupDirection, PopupMode, UI};
use crate::views::{Borders, Dimension, Gravity, Visibility};
use crate::views::popupmenu::PopupMenu;
use crate::styles::selector::FontSelector;
use crate::views::{FieldsMain, FieldsTexted};
use crate::view_base::{HasMainFields, ViewBasics};



const ICON_GAP_DIP: i32 = 2;
/// Highlight colour behind selected text (same blue as `Edit`/`Memo`).

pub struct Label {
    state: RefCell<FieldsTexted>,
    /// When true, render as a hyperlink: link-coloured text + underline; the
    /// view becomes focusable and dispatches `EventType::Click`.
    link: RefCell<bool>,
    /// None = the theme's "link" token; Some = user override.
    link_color: RefCell<Option<u32>>,
    /// Tracks press-down on the label so `on_mouse_button_up` only fires the
    /// click when release lands on the label too (drag-off cancels).
    pressed: RefCell<bool>,
    /// Optional rounded-rectangle background fill drawn before the text.
    background_color: RefCell<Option<u32>>,
    /// Optional override for the text colour. Wins over both the link colour
    /// (when `link=true`) and the theme default.
    text_color: RefCell<Option<u32>>,
    /// Corner radius (dip) for the background fill. 0 = square.
    corner_radius: RefCell<i32>,
    // Leading/trailing icons + tint. `None` = no icon for that slot.
    left_icon: RefCell<Option<ImageSource>>,
    right_icon: RefCell<Option<ImageSource>>,
    /// None = the theme's "icon_tint" token; Some = user override.
    icon_tint: RefCell<Option<u32>>,
    /// Track which icon (if any) absorbed the most recent mouse-down, so the
    /// click only fires on mouse-up if the release lands over the same icon.
    pressed_icon: RefCell<Option<bool>>, // Some(true)=left, Some(false)=right
    /// Width / height / scale params used the last time `layout_content` ran.
    /// `layout_content` returns the cached rect when these match (skipping the
    /// expensive font shaping); when they differ we re-layout. Resolves the
    /// "Label doesn't reflow on parent resize" bug.
    last_layout_params: std::cell::Cell<Option<(i32, i32, f64)>>,
    /// When true, the text can be selected with the mouse (I-beam cursor,
    /// click-drag highlight, right-click Copy / Select All). Read-only:
    /// no editing, no keyboard, no focus changes. Default false.
    selectable: RefCell<bool>,
    /// Anchor (fixed end) of the current selection, as a char index into
    /// `text`. `None` = no selection; equal to `caret_pos` = empty selection.
    selection_anchor: RefCell<Option<usize>>,
    /// Moving end of the selection drag, as a char index into `text`.
    caret_pos: RefCell<usize>,
    /// True while the left button is held after a press inside the text, so
    /// mouse-move extends the selection (even when the pointer leaves the view).
    dragging: RefCell<bool>,
    /// Start char index of each visual (wrapped) line, rebuilt whenever the
    /// text is laid out. Drives hit-testing and the selection highlight.
    line_offsets: RefCell<Vec<usize>>,
}

impl HasMainFields for Label {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for Label {}

#[allow(dead_code)]
impl Label {
    pub fn new(rect: Rect<i32>, text: &str, text_size: f32) -> Label {
        let mut main = FieldsMain::with_rect(rect, Dimension::Min, Dimension::Min);
        main.state.focusable = false;
        Label {
            state: RefCell::new(FieldsTexted {
                main,
                text: text.to_owned(),
                text_size,
                line_height: 0f32,
                single_line: false,
                cached_text: None,
                font: FontSelector::new()
            }),
            link: RefCell::new(false),
            link_color: RefCell::new(None),
            pressed: RefCell::new(false),
            background_color: RefCell::new(None),
            text_color: RefCell::new(None),
            corner_radius: RefCell::new(0),
            left_icon: RefCell::new(None),
            right_icon: RefCell::new(None),
            icon_tint: RefCell::new(None),
            pressed_icon: RefCell::new(None),
            last_layout_params: std::cell::Cell::new(None),
            selectable: RefCell::new(false),
            selection_anchor: RefCell::new(None),
            caret_pos: RefCell::new(0),
            dragging: RefCell::new(false),
            line_offsets: RefCell::new(Vec::new()),
        }
    }

    pub fn set_background_color(&self, color: Option<u32>) {
        *self.background_color.borrow_mut() = color;
    }

    pub fn set_text_color(&self, color: Option<u32>) {
        *self.text_color.borrow_mut() = color;
    }

    pub fn set_corner_radius(&self, radius: i32) {
        *self.corner_radius.borrow_mut() = radius;
    }

    pub fn set_left_icon(&self, path: &str) {
        *self.left_icon.borrow_mut() = ImageSource::for_path(path);
        self.state.borrow_mut().cached_text = None;
    }

    pub fn set_right_icon(&self, path: &str) {
        *self.right_icon.borrow_mut() = ImageSource::for_path(path);
        self.state.borrow_mut().cached_text = None;
    }

    pub fn set_icon_tint(&self, tint: u32) {
        *self.icon_tint.borrow_mut() = Some(tint);
    }

    /// `&self` visibility setter. Safe to call from inside an event handler
    /// firing from this same view — the trait-level `set_visibility(&mut self)`
    /// would deadlock because the dispatcher already holds `element.borrow()`.
    pub fn hide(&self) {
        self.base_set_visibility(Visibility::Gone);
    }

    pub fn show(&self) {
        self.base_set_visibility(Visibility::Visible);
    }

    fn load_icons(&self) {
        if let Some(s) = self.left_icon.borrow_mut().as_mut() {
            s.ensure_loaded();
        }
        if let Some(s) = self.right_icon.borrow_mut().as_mut() {
            s.ensure_loaded();
        }
    }

    /// Width in pixels reserved by an icon side; 0 when no icon is set.
    fn icon_side_width(has_icon: bool, inner_height: i32, scale: f64) -> i32 {
        if !has_icon || inner_height <= 0 {
            return 0;
        }
        inner_height + (ICON_GAP_DIP as f64 * scale).round() as i32
    }

    /// (left_inset, right_inset) in pixels, given inner (post-padding) height.
    fn icon_insets(&self, inner_height: i32, scale: f64) -> (i32, i32) {
        let has_left = self.left_icon.borrow().is_some();
        let has_right = self.right_icon.borrow().is_some();
        (
            Self::icon_side_width(has_left, inner_height, scale),
            Self::icon_side_width(has_right, inner_height, scale),
        )
    }

    /// Icon hit rectangles in the same coord system as `state.main.rect` (pre-origin).
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
        let has_left = self.left_icon.borrow().is_some();
        let has_right = self.right_icon.borrow().is_some();
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
        let cell = if is_left { &self.left_icon } else { &self.right_icon };
        if let Some(icon) = cell.borrow_mut().as_mut() {
            icon.draw(theme, icon_rect, tint);
        }
    }

    pub fn set_link(&self, link: bool) {
        *self.link.borrow_mut() = link;
        // Links accept hover/press input.
        self.state.borrow_mut().main.state.focusable = link;
        // Resizing depends on link state — invalidate the cache so the next
        // paint/layout recomputes with the underline reservation included.
        self.state.borrow_mut().cached_text = None;
    }

    pub fn is_link(&self) -> bool {
        *self.link.borrow()
    }

    /// Extra vertical pixels reserved below the text for the link underline.
    /// Currently zero — the underline is drawn inside the text bounding box
    /// (sits in the descender row), so no extra space is needed.
    fn link_extra_v(&self, _scale: f64) -> i32 {
        0
    }

    fn rebuild_text(&self) {
        let state = self.state.borrow();
        let typeface = state.main.font_manager.get_typeface(&Typeface::default());
        let font = match get_font_family(&typeface.font_name, typeface.font_style) {
            Some(f) => f,
            None => return,
        };
        let scale = state.main.scale;
        // Effective padding (may come from a 9-patch background), pre-scaled.
        let padding = self.get_padding(scale);
        let pad_h = padding.left + padding.right;
        // text_size is dips, like an explicit font_size — both scale.
        let base_size = typeface.font_size
            .unwrap_or(state.text_size) * scale as f32;
        // Reserve icon space using a font-height estimate so wrap-to-width
        // accounts for icons before the text is laid out.
        let has_left = self.left_icon.borrow().is_some();
        let has_right = self.right_icon.borrow().is_some();
        let est_icon = base_size.ceil() as i32;
        let gap = (ICON_GAP_DIP as f64 * scale).round() as i32;
        let icon_reserve = (if has_left { est_icon + gap } else { 0 })
            + (if has_right { est_icon + gap } else { 0 });
        // Use parent width for wrapping when label is width=Min
        let available_width = if matches!(state.main.width, Dimension::Min) {
            if let Some(parent) = state.main.parent.as_ref().and_then(|w| w.upgrade()) {
                let parent_rect = parent.borrow().get_rect();
                let label_x = state.main.rect.min.x;
                (parent_rect.max.x - label_x - pad_h - icon_reserve).max(0)
            } else {
                (state.main.rect.width() - pad_h - icon_reserve).max(0)
            }
        } else {
            (state.main.rect.width() - pad_h - icon_reserve).max(0)
        };
        let options = match state.single_line {
            true => TextOptions::new(),
            false => TextOptions::new().with_wrap_to_width(available_width as f32, TextAlignment::Left),
        };
        let text = font.layout_text(&state.text, base_size, options);
        *self.line_offsets.borrow_mut() = Self::build_line_offsets(&text, &state.text);
        // After layout, recompute icon reservation using actual text height so
        // the final rect snaps tight; insets used by paint/hit-test derive
        // from this same text height via `icon_insets`.
        let actual_icon = if has_left || has_right { text.height().ceil() as i32 } else { 0 };
        let actual_reserve = (if has_left { actual_icon + gap } else { 0 })
            + (if has_right { actual_icon + gap } else { 0 });
        // Update rect to fit new text
        let new_width = text.width().ceil() as i32 + pad_h + actual_reserve;
        let pad_v = padding.top + padding.bottom;
        let new_height = text.height().ceil() as i32 + pad_v + self.link_extra_v(scale);
        drop(state);
        let mut state = self.state.borrow_mut();
        if matches!(state.main.width, Dimension::Min) {
            state.main.rect.max.x = state.main.rect.min.x + new_width;
        }
        if matches!(state.main.height, Dimension::Min) {
            state.main.rect.max.y = state.main.rect.min.y + new_height;
        }
        state.cached_text = Some(text);
    }

    pub fn set_text(&mut self, text: &str) {
        let mut state = self.state.borrow_mut();
        state.text.clear();
        state.text.push_str(text);
        let _ = state.cached_text.take();
    }

    pub fn get_text(&self) -> String {
        self.state.borrow().text.clone()
    }

    pub fn set_single_line(&self, single_line: bool) {
        let mut state = self.state.borrow_mut();
        state.single_line = single_line;
        state.cached_text = None;
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

    pub fn set_font_size(&self, size: f32) {
        let mut state = self.state.borrow_mut();
        state.main.font_manager.set_font_size(size);
        state.cached_text = None;
    }

    pub fn set_selectable(&self, selectable: bool) {
        *self.selectable.borrow_mut() = selectable;
    }

    pub fn is_selectable(&self) -> bool {
        *self.selectable.borrow()
    }

    /// Build `line_offsets` (start char index per visual line) from a laid-out
    /// block. Mirrors `Memo`: wrapped lines advance by glyph count, hard `\n`
    /// breaks are skipped (they are not glyphs), and a trailing `\n` adds a
    /// virtual empty line.
    fn build_line_offsets(text: &TextBlock, full_text: &str) -> Vec<usize> {
        let chars: Vec<char> = full_text.chars().collect();
        let mut offsets = Vec::new();
        let mut char_offset = 0usize;
        for line in text.iter_lines() {
            offsets.push(char_offset);
            char_offset += line.iter_glyphs().count();
            if char_offset < chars.len() && chars[char_offset] == '\n' {
                char_offset += 1;
            }
        }
        if !full_text.is_empty() && full_text.ends_with('\n') {
            offsets.push(chars.len());
        }
        if offsets.is_empty() {
            offsets.push(0);
        }
        offsets
    }

    fn has_selection(&self) -> bool {
        match *self.selection_anchor.borrow() {
            Some(anchor) => anchor != *self.caret_pos.borrow(),
            None => false,
        }
    }

    /// `(start, end)` char indices of the selection, or `None` when empty.
    fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = (*self.selection_anchor.borrow())?;
        let caret = *self.caret_pos.borrow();
        if anchor == caret {
            return None;
        }
        Some((anchor.min(caret), anchor.max(caret)))
    }

    fn clear_selection(&self) {
        *self.selection_anchor.borrow_mut() = None;
    }

    pub fn select_all(&self) {
        let len = self.state.borrow().text.chars().count();
        *self.selection_anchor.borrow_mut() = Some(0);
        *self.caret_pos.borrow_mut() = len;
    }

    fn get_selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let text = &self.state.borrow().text;
        Some(text.chars().skip(start).take(end - start).collect())
    }

    fn copy_to_clipboard(&self) {
        if let Some(text) = self.get_selected_text() {
            crate::clipboard::set_text(&text);
        }
    }

    /// Vertical pixel of the laid-out text top, in `main.rect` coordinates
    /// (matches the `text_y` used by `paint`, minus `origin`).
    fn text_top(&self, text_height: f32) -> i32 {
        let my_rect = self.state.borrow().main.rect;
        let y = (my_rect.height() as f32 - text_height) / 2f32;
        (my_rect.min.y as f32 + y).round() as i32
    }

    /// Per visual-line height (uniform — `Label` uses a single font/size).
    fn per_line_height(&self, text_height: f32) -> f32 {
        let lines = self.line_offsets.borrow().len().max(1);
        text_height / lines as f32
    }

    /// Map a mouse point (in `main.rect` coordinates) to a char index.
    fn pos_from_point(&self, x: i32, y: i32) -> usize {
        let state = self.state.borrow();
        let text = match &state.cached_text {
            Some(t) => t,
            None => return 0,
        };
        let scale = state.main.scale;
        let padding = self.get_padding(scale);
        let my_rect = state.main.rect;
        let inner_h = my_rect.height() - padding.top - padding.bottom;
        let (left_inset, _) = self.icon_insets(inner_h, scale);
        let text_x = my_rect.min.x + padding.left + left_inset;
        let text_top = self.text_top(text.height());
        let per_line = self.per_line_height(text.height());

        let offsets = self.line_offsets.borrow();
        let line_count = offsets.len();
        let target_line = if per_line > 0.0 {
            (((y - text_top) as f32 / per_line).floor().max(0.0) as usize).min(line_count - 1)
        } else {
            0
        };
        let line_start = offsets[target_line];

        let visual_line_count = text.iter_lines().count();
        if target_line >= visual_line_count {
            return line_start; // virtual empty line after a trailing '\n'
        }
        if let Some(line) = text.iter_lines().nth(target_line) {
            let rel_x = (x - text_x) as f32;
            for (i, glyph) in line.iter_glyphs().enumerate() {
                let mid = glyph.position_x() + glyph.advance_width() / 2.0;
                if rel_x < mid {
                    return line_start + i;
                }
            }
            return line_start + line.iter_glyphs().count();
        }
        line_start
    }

    /// Map a char index to `(visual_line, x_pixels_from_line_left)`.
    fn pos_to_line_and_x(&self, pos: usize) -> (usize, f32) {
        let offsets = self.line_offsets.borrow();
        let mut line_idx = 0;
        for i in (0..offsets.len()).rev() {
            if offsets[i] <= pos {
                line_idx = i;
                break;
            }
        }
        let pos_in_line = pos - offsets[line_idx];

        let state = self.state.borrow();
        if let Some(text) = &state.cached_text {
            if line_idx >= text.iter_lines().count() {
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
                let x = line.iter_glyphs().last()
                    .map(|g| g.position_x() + g.advance_width())
                    .unwrap_or(0.0);
                return (line_idx, x);
            }
        }
        (line_idx, 0.0)
    }

    fn open_context_menu(&self, ui: &mut UI, x: i32, y: i32) {
        let mut menu = PopupMenu::new();
        menu.add_item("copy", "", "Copy");
        menu.add_item("select_all", "", "Select All");

        let label_id = self.get_id();
        menu.on_event(EventType::Click, Box::new(move |ui: &mut UI, view: &dyn View, _data: &EventData| {
            let menu = view.as_any().downcast_ref::<PopupMenu>().unwrap();
            if let Some(index) = menu.get_hovered_index()
                && let Some(el) = ui.get_view(&label_id)
            {
                let b = el.borrow();
                if let Some(label) = b.as_any().downcast_ref::<Label>() {
                    match index {
                        0 => label.copy_to_clipboard(),
                        1 => label.select_all(),
                        _ => {}
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

impl View for Label {
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
            "single_line" => { self.state.borrow_mut().single_line = value.parse().unwrap_or(false) }
            "link" => { self.set_link(value == "true") }
            "link_color" => {
                if let Some(c) = crate::view_base::parse_color_value(value) {
                    *self.link_color.borrow_mut() = Some(c);
                }
            }
            "background_color" => {
                self.set_background_color(crate::view_base::parse_color_value(value));
            }
            "text_color" => {
                self.set_text_color(crate::view_base::parse_color_value(value));
            }
            "corner_radius" => {
                if let Ok(r) = value.parse::<i32>() {
                    self.set_corner_radius(r);
                }
            }
            "left_icon" => { self.set_left_icon(value); }
            "right_icon" => { self.set_right_icon(value); }
            "icon_tint" => {
                if let Some(c) = crate::view_base::parse_color_value(value) {
                    self.set_icon_tint(c);
                }
            }
            "selectable" => { self.set_selectable(value == "true") }
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
        // Skip the (expensive) font shaping if nothing relevant has changed.
        // We re-layout when (width, height, scale) differ from the previous
        // call OR when the text was invalidated (cached_text is None).
        let params = (width, height, scale);
        if self.state.borrow().cached_text.is_some()
            && self.last_layout_params.get() == Some(params)
        {
            return self.get_rect();
        }
        self.last_layout_params.set(Some(params));
        self.state.borrow_mut().cached_text = None;

        self.base_set_scale(scale);
        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let (new_width, new_height) = self.calculate_size(width - horizontal, height - vertical, scale);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface.clone()));
        let base_size = typeface.font_size
            .unwrap_or(self.state.borrow().text_size) * scale as f32;
        let has_left = self.left_icon.borrow().is_some();
        let has_right = self.right_icon.borrow().is_some();
        let est_icon = base_size.ceil() as i32;
        let gap = (ICON_GAP_DIP as f64 * scale).round() as i32;
        let icon_reserve = (if has_left { est_icon + gap } else { 0 })
            + (if has_right { est_icon + gap } else { 0 });
        if let Some(font) = get_font_family(&typeface.font_name, typeface.font_style) {
            let single_line = self.state.borrow().single_line;
            let wrap_w = (new_width - icon_reserve).max(0);
            let options = match single_line {
                true => TextOptions::new(),
                false => TextOptions::new().with_wrap_to_width(wrap_w as f32, TextAlignment::Left),
            };
            let text = font.layout_text(&self.state.borrow().text, base_size, options);
            *self.line_offsets.borrow_mut() = Self::build_line_offsets(&text, &self.state.borrow().text);
            self.state.borrow_mut().cached_text = Some(text);
        }
        let (content_width, content_height) = self.calculate_full_size(scale);
        let (b_width, b_height) = self.get_bounds();
        let final_width = match b_width {
            Dimension::Min => content_width,
            _ => new_width + horizontal,
        };
        let final_height = match b_height {
            Dimension::Min => content_height,
            _ => new_height + vertical,
        };
        let rect = rect((x, y), (x + final_width, y + final_height));
        self.set_rect(rect.clone());
        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let state = self.state.borrow();
        match &state.cached_text {
            Some(text) => text.width() <= width as f32 && text.height() <= height as f32,
            None => true
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        // Rebuild cached text if it was invalidated (e.g. by set_text)
        if self.state.borrow().cached_text.is_none() {
            self.rebuild_text();
        }
        // Lazy-load icon assets on first paint.
        self.load_icons();
        let state = self.state.borrow();
        let mut rect = state.main.rect;
        rect.move_by(origin);
        let scale = state.main.scale;
        theme.push_clip();
        theme.clip_rect(rect);

        // Background fill (9-patch, else rounded if corner_radius > 0).
        if !self.base_draw_ninepatch(theme, rect)
            && let Some(bg) = *self.background_color.borrow()
        {
            let radius = ((*self.corner_radius.borrow() as f64) * scale).round() as i32;
            theme.draw_rounded_rect(rect, bg, radius);
        }

        // Icon insets — same coordinate convention as Edit.
        let padding = self.get_padding(scale);
        let inner_h = rect.height() - padding.top - padding.bottom;
        let (left_inset, right_inset) = self.icon_insets(inner_h, scale);

        if let Some(text) = &state.cached_text {
            let is_link = *self.link.borrow();
            let has_bg = self.background_color.borrow().is_some();
            // Colour precedence: explicit text_color > link_color (if link) > theme default.
            let color = if let Some(c) = *self.text_color.borrow() {
                c
            } else if is_link {
                self.link_color.borrow().unwrap_or_else(|| theme.color("link"))
            } else {
                theme.get_text_color(state.main.state, state.main.foreground.as_ref())
            };
            let y = (self.get_rect_height() as f32 - text.height()) / 2f32;
            let text_x = (rect.min.x + padding.left + left_inset) as f32;
            let text_y = (rect.min.y as f32 + y).round();
            // Selection highlight (drawn under the text).
            let mut sel_rects = Vec::new();
            if *self.selectable.borrow()
                && let Some((sel_start, sel_end)) = self.selection_range()
            {
                let (start_line, start_x) = self.pos_to_line_and_x(sel_start);
                let (end_line, end_x) = self.pos_to_line_and_x(sel_end);
                let per_line = self.per_line_height(text.height());
                let text_top = text_y as i32;
                let text_left = text_x as i32;
                let line_right = rect.max.x - padding.right - right_inset;
                for line in start_line..=end_line {
                    let y_top = text_top + (line as f32 * per_line).round() as i32;
                    let y_bottom = text_top + ((line + 1) as f32 * per_line).round() as i32;
                    let x_left = if line == start_line { text_left + start_x.round() as i32 } else { text_left };
                    let x_right = if line == end_line { text_left + end_x.round() as i32 } else { line_right };
                    let sel_rect = crate::types::rect((x_left, y_top), (x_right, y_bottom));
                    theme.draw_rect(sel_rect, theme.color("selection"));
                    sel_rects.push(sel_rect);
                }
            }
            theme.draw_text(text_x, text_y, color, text);
            // Redraw the selected part in a contrasting color over the highlight
            if !sel_rects.is_empty() {
                let sel_color = crate::themes::selection_text_color(theme.color("selection"));
                for sel_rect in sel_rects {
                    theme.draw_text_cropped(text_x, text_y, sel_rect, sel_color, text);
                }
            }
            // Underline: only when link mode is on AND there's no background
            // (a filled chip with an underlined word looks busy).
            if is_link && !has_bg {
                let line_h = ((1.0 * scale).round() as i32).max(1);
                let underline_bottom = (text_y + text.height()).round() as i32;
                let text_w = text.width().ceil() as i32;
                let underline = crate::types::rect(
                    (text_x.round() as i32, underline_bottom - line_h),
                    (text_x.round() as i32 + text_w, underline_bottom),
                );
                theme.draw_rect(underline, color);
            }
        }

        // Icons (drawn after text so their square hit area sits over the
        // padded reservation rather than under any background-colour fill).
        let tint = self.icon_tint.borrow().unwrap_or_else(|| theme.color("icon_tint"));
        if inner_h > 0 {
            let icon_size = inner_h;
            let inner_top = rect.min.y + padding.top;
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
        }

        theme.pop_clip();
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
        let scale = self.state.borrow().main.scale;
        let extra_v = self.link_extra_v(scale);
        let has_left = self.left_icon.borrow().is_some();
        let has_right = self.right_icon.borrow().is_some();
        let gap = (ICON_GAP_DIP as f64 * scale).round() as i32;
        let state = self.state.borrow();
        match &state.cached_text {
            None => (0, extra_v),
            Some(text) => {
                let icon = if has_left || has_right { text.height().ceil() as i32 } else { 0 };
                let icon_reserve = (if has_left { icon + gap } else { 0 })
                    + (if has_right { icon + gap } else { 0 });
                let width = text.width().round() as i32 + icon_reserve;
                let height = text.height().round() as i32 + extra_v;
                (width, height)
            }
        }
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
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

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        self.fire_click(ui)
    }

    fn accessibility_node(&self) -> accesskit::Node {
        if self.is_link() {
            let mut node = accesskit::Node::new(accesskit::Role::Link);
            node.set_label(self.get_text());
            node.add_action(accesskit::Action::Click);
            return node;
        }
        let mut node = accesskit::Node::new(accesskit::Role::Label);
        // AccessKit convention: a static-text node's content is its VALUE
        // (`label_comes_from_value`); platform adapters derive the accessible
        // name from it, and `labelled_by` associations read it too.
        node.set_value(self.get_text());
        node
    }

    fn on_mouse_move(&self, ui: &mut UI, position: Point<i32>) -> bool {
        // Selection drag — continues even when the pointer leaves the view
        // (the Frame dispatches moves to every child).
        if *self.dragging.borrow() {
            *self.caret_pos.borrow_mut() = self.pos_from_point(position.x, position.y);
            return true;
        }
        if *self.link.borrow() {
            let hit = self.state.borrow().main.rect.hit((position.x, position.y));
            if hit { ui.request_cursor(MouseCursorType::Pointer); }
            let old_state = self.state.borrow().main.state;
            self.state.borrow_mut().main.state.hovered = hit;
            return self.state.borrow().main.state != old_state;
        }
        if *self.selectable.borrow()
            && self.state.borrow().main.rect.hit((position.x, position.y))
        {
            ui.request_cursor(MouseCursorType::Text);
        }
        false
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        // Right-click opens the Copy / Select All menu on selectable labels.
        if matches!(button, MouseButton::Right) {
            if *self.selectable.borrow()
                && self.state.borrow().main.rect.hit((position.x, position.y))
                && !ui.context_menu_suppressed()
            {
                self.open_context_menu(ui, position.x, position.y);
                return true;
            }
            return false;
        }
        if !matches!(button, MouseButton::Left) { return false; }
        // Icon clicks work even when the label isn't a link — capture the press
        // and route to LeftIconClick / RightIconClick on mouse-up.
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
        // A link label is a single clickable unit — link wins over selection.
        if *self.link.borrow() {
            if self.state.borrow().main.rect.hit((position.x, position.y)) {
                *self.pressed.borrow_mut() = true;
                self.state.borrow_mut().main.state.pressed = true;
                return true;
            }
            return false;
        }
        // Start a selection drag (a fresh click clears any previous selection,
        // here and in any other view that held one).
        if *self.selectable.borrow()
            && self.state.borrow().main.rect.hit((position.x, position.y))
        {
            ui.deselect_text();
            let pos = self.pos_from_point(position.x, position.y);
            *self.selection_anchor.borrow_mut() = Some(pos);
            *self.caret_pos.borrow_mut() = pos;
            *self.dragging.borrow_mut() = true;
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if !matches!(button, MouseButton::Left) { return false; }
        // Finish a selection drag; collapse a zero-length (plain-click) selection.
        if *self.dragging.borrow() {
            *self.dragging.borrow_mut() = false;
            if *self.selection_anchor.borrow() == Some(*self.caret_pos.borrow()) {
                self.clear_selection();
            }
            return true;
        }
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
        if !*self.link.borrow() { return false; }
        let was_pressed = *self.pressed.borrow();
        *self.pressed.borrow_mut() = false;
        self.state.borrow_mut().main.state.pressed = false;
        if was_pressed && self.state.borrow().main.rect.hit((position.x, position.y)) {
            self.fire_click(ui);
            return true;
        }
        false
    }

    fn deselect_text(&self) {
        self.clear_selection();
    }

}

impl Default for Label {
    fn default() -> Self {
        let rect = rect((0, 0), (60, 24));
        Label::new(rect, "", crate::drawing::current_text_size("label"))
    }
}

impl Label {
    fn fire_click(&self, ui: &mut UI) -> bool {
        self.base_fire_event(ui, EventType::Click, &EventData::None)
    }
}
