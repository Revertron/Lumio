use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use speedy2d::dimen::Vector2;
use speedy2d::window::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};
use super::super::events::EventType;
use super::super::themes::{Theme, Typeface, ViewState};
use super::super::traits::{Element, View, WeakElement};
use super::super::types::{Point, Rect, rect};
use super::super::ui::UI;
use super::super::views::{Borders, Dimension, FieldsMain};
use super::super::view_base::{HasMainFields, ViewBasics};

// ============================================================================
// ViewHolder - Wraps a view for recycling
// ============================================================================

#[derive(Clone)]
pub struct ViewHolder {
    /// The actual view element
    pub item_view: Element,

    /// Cached position (for recycling)
    position: Rc<RefCell<usize>>,

    /// View type (for matching during recycling)
    pub view_type: i32,

    /// User data cache (optional, for avoiding repeated queries)
    user_data: Rc<RefCell<Option<Box<dyn Any>>>>
}

impl ViewHolder {
    pub fn new(item_view: Element, view_type: i32) -> Self {
        Self {
            item_view,
            position: Rc::new(RefCell::new(0)),
            view_type,
            user_data: Rc::new(RefCell::new(None))
        }
    }

    pub fn get_position(&self) -> usize {
        *self.position.borrow()
    }

    pub fn set_position(&self, pos: usize) {
        *self.position.borrow_mut() = pos;
    }

    pub fn set_user_data<T: Any>(&self, data: T) {
        *self.user_data.borrow_mut() = Some(Box::new(data));
    }

    pub fn get_user_data<T: Any>(&self) -> Option<T> where T: Clone {
        self.user_data.borrow()
            .as_ref()
            .and_then(|b| b.downcast_ref::<T>())
            .cloned()
    }
}

// ============================================================================
// RecyclerAdapter - Bridges data and views
// ============================================================================

pub trait RecyclerAdapter {
    /// Total number of items
    fn get_item_count(&self) -> usize;

    /// Get view type for position (for heterogeneous lists)
    fn get_item_view_type(&self, _position: usize) -> i32 {
        0
    }

    /// Create a new ViewHolder (called when no recycled view available)
    fn create_view_holder(&mut self, view_type: i32) -> ViewHolder;

    /// Bind data to ViewHolder (called for each visible item)
    fn bind_view_holder(&self, holder: &ViewHolder, position: usize);

    /// Optional: Get stable ID for item (for animations/diffing)
    fn get_item_id(&self, position: usize) -> i64 {
        position as i64
    }

    /// Optional: Called when ViewHolder is recycled (cleanup)
    fn on_view_recycled(&self, _holder: &ViewHolder) {}
}

// ============================================================================
// LayoutManager - Controls positioning
// ============================================================================

/// Layout information for an item
#[derive(Debug, Clone)]
pub struct LayoutInfo {
    pub position: usize,
    pub rect: Rect<i32>,
    pub view_type: i32
}

/// Abstract layout manager
pub trait LayoutManager {
    /// Calculate layout for visible range
    fn layout_items(
        &mut self,
        item_count: usize,
        viewport: Rect<i32>,
        scroll_offset: i32,
        adapter: &dyn RecyclerAdapter
    ) -> Vec<LayoutInfo>;

    /// Get total content height/width
    fn get_content_size(&self, item_count: usize) -> (i32, i32);

    /// Find first visible position
    fn find_first_visible_position(&self, scroll_offset: i32) -> usize;

    /// Find last visible position
    fn find_last_visible_position(&self, scroll_offset: i32, viewport_height: i32) -> usize;

    /// Get scroll offset for position (for scrollToPosition)
    fn get_scroll_for_position(&self, position: usize) -> i32;

    /// Update measured height for an item after layout
    fn update_item_height(&self, position: usize, height: i32);

    /// Get the current height for an item
    fn get_item_height(&self, position: usize) -> i32;
}

// ============================================================================
// LinearLayoutManager - Vertical linear layout
// ============================================================================

pub struct LinearLayoutManager {
    /// Item height (fixed or per-item)
    item_heights: RefCell<Vec<i32>>,

    /// Default item height (when not measured)
    default_item_height: i32,

    /// Spacing between items
    item_spacing: i32,

    /// Cached cumulative positions
    cumulative_positions: RefCell<Vec<i32>>
}

impl LinearLayoutManager {
    pub fn new(default_item_height: i32) -> Self {
        Self {
            item_heights: RefCell::new(Vec::new()),
            default_item_height,
            item_spacing: 0,
            cumulative_positions: RefCell::new(Vec::new())
        }
    }

    pub fn set_item_spacing(&mut self, spacing: i32) {
        self.item_spacing = spacing;
    }

    fn get_height(&self, position: usize) -> i32 {
        self.item_heights.borrow()
            .get(position)
            .copied()
            .unwrap_or(self.default_item_height)
    }

    fn set_item_height(&self, position: usize, height: i32) {
        let mut heights = self.item_heights.borrow_mut();

        // Extend the vector if necessary
        if position >= heights.len() {
            heights.resize(position + 1, self.default_item_height);
        }

        // Only update if height changed (to avoid unnecessary recalculations)
        if heights[position] != height {
            heights[position] = height;
            // Clear cumulative positions cache so it gets recalculated
            self.cumulative_positions.borrow_mut().clear();
        }
    }

    fn update_cumulative_positions(&self, item_count: usize) {
        let mut cumulative = self.cumulative_positions.borrow_mut();

        if cumulative.len() >= item_count {
            return;
        }

        let start = cumulative.len();
        let last_pos = cumulative.last().copied().unwrap_or(0);

        cumulative.reserve(item_count - start);

        let mut current_pos = last_pos;
        for i in start..item_count {
            cumulative.push(current_pos);
            current_pos += self.get_height(i) + self.item_spacing;
        }
    }

    fn get_position_y(&self, position: usize) -> i32 {
        self.cumulative_positions.borrow()
            .get(position)
            .copied()
            .unwrap_or(0)
    }
}

impl LayoutManager for LinearLayoutManager {
    fn layout_items(
        &mut self,
        item_count: usize,
        viewport: Rect<i32>,
        scroll_offset: i32,
        adapter: &dyn RecyclerAdapter
    ) -> Vec<LayoutInfo> {
        if item_count == 0 {
            return Vec::new();
        }

        self.update_cumulative_positions(item_count);

        let mut layouts = Vec::new();

        let first_visible = self.find_first_visible_position(scroll_offset);
        let last_visible = self.find_last_visible_position(scroll_offset, viewport.height());

        for pos in first_visible..=last_visible.min(item_count.saturating_sub(1)) {
            // Position items at their absolute positions (not adjusted for scroll)
            // Scroll will be applied during painting
            let y = self.get_position_y(pos);
            let height = self.get_height(pos);

            layouts.push(LayoutInfo {
                position: pos,
                rect: rect(
                    (0, y),
                    (viewport.width(), y + height)
                ),
                view_type: adapter.get_item_view_type(pos)
            });
        }

        layouts
    }

    fn get_content_size(&self, item_count: usize) -> (i32, i32) {
        if item_count == 0 {
            return (0, 0);
        }

        self.update_cumulative_positions(item_count);

        let last_y = self.get_position_y(item_count - 1);
        let last_height = self.get_height(item_count - 1);
        let total_height = last_y + last_height;

        (0, total_height)
    }

    fn find_first_visible_position(&self, scroll_offset: i32) -> usize {
        let positions = self.cumulative_positions.borrow();

        // Binary search for first visible position
        match positions.binary_search(&scroll_offset) {
            Ok(pos) => pos,
            Err(pos) => pos.saturating_sub(1)
        }
    }

    fn find_last_visible_position(&self, scroll_offset: i32, viewport_height: i32) -> usize {
        let bottom = scroll_offset + viewport_height;
        let positions = self.cumulative_positions.borrow();

        match positions.binary_search(&bottom) {
            Ok(pos) => pos,
            Err(pos) => pos.min(positions.len().saturating_sub(1))
        }
    }

    fn get_scroll_for_position(&self, position: usize) -> i32 {
        self.get_position_y(position)
    }

    fn update_item_height(&self, position: usize, height: i32) {
        self.set_item_height(position, height);
    }

    fn get_item_height(&self, position: usize) -> i32 {
        self.get_height(position)
    }
}

// ============================================================================
// RecyclerPool - Manages view reuse
// ============================================================================

struct RecyclerPool {
    /// Pools organized by view type
    pools: RefCell<HashMap<i32, Vec<ViewHolder>>>,

    /// Maximum pool size per type
    max_pool_size: usize
}

impl RecyclerPool {
    fn new() -> Self {
        Self {
            pools: RefCell::new(HashMap::new()),
            max_pool_size: 10
        }
    }

    fn set_max_pool_size(&mut self, size: usize) {
        self.max_pool_size = size;
    }

    /// Get a recycled ViewHolder or None
    fn acquire(&self, view_type: i32) -> Option<ViewHolder> {
        let mut pools = self.pools.borrow_mut();
        pools.get_mut(&view_type)
            .and_then(|pool| pool.pop())
    }

    /// Return ViewHolder to pool
    fn recycle(&self, holder: ViewHolder) {
        let mut pools = self.pools.borrow_mut();
        let pool = pools.entry(holder.view_type).or_insert_with(Vec::new);

        if pool.len() < self.max_pool_size {
            pool.push(holder);
        }
        // else: drop it (ViewHolder goes out of scope)
    }

    /// Clear all pools
    fn clear(&self) {
        self.pools.borrow_mut().clear();
    }
}

// ============================================================================
// RecyclerView - Main component
// ============================================================================

pub struct RecyclerView {
    /// Base view fields
    state: RefCell<FieldsMain>,

    /// The adapter providing data
    adapter: RefCell<Option<Box<dyn RecyclerAdapter>>>,

    /// Layout manager
    layout_manager: RefCell<Box<dyn LayoutManager>>,

    /// View recycler pool
    recycler: RecyclerPool,

    /// Currently attached ViewHolders (visible items)
    attached_holders: RefCell<Vec<ViewHolder>>,

    /// Scroll state (horizontal and vertical)
    scroll_x: RefCell<i32>,
    scroll_y: RefCell<i32>,

    /// Maximum scroll (content height - viewport height)
    max_scroll: RefCell<i32>,

    /// Selected item
    selected_position: RefCell<Option<usize>>,

    /// Item click listener
    on_item_click: RefCell<Option<Box<dyn Fn(usize)>>>,

    /// Needs layout flag
    needs_layout: RefCell<bool>
}

impl HasMainFields for RecyclerView {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        &self.state
    }
}

impl ViewBasics for RecyclerView {}

impl RecyclerView {
    pub fn new(rect: Rect<i32>) -> Self {
        let mut main = FieldsMain::with_rect(rect, Dimension::Max, Dimension::Max);
        main.state.focusable = true;  // Make RecyclerView focusable for keyboard events
        Self {
            state: RefCell::new(main),
            adapter: RefCell::new(None),
            layout_manager: RefCell::new(Box::new(LinearLayoutManager::new(24))),
            recycler: RecyclerPool::new(),
            attached_holders: RefCell::new(Vec::new()),
            scroll_x: RefCell::new(0),
            scroll_y: RefCell::new(0),
            max_scroll: RefCell::new(0),
            selected_position: RefCell::new(None),
            on_item_click: RefCell::new(None),
            needs_layout: RefCell::new(true)
        }
    }

    /// Set adapter
    pub fn set_adapter(&self, adapter: Box<dyn RecyclerAdapter>) {
        self.adapter.replace(Some(adapter));
        self.recycler.clear();
        self.attached_holders.borrow_mut().clear();
        *self.needs_layout.borrow_mut() = true;
    }

    /// Set layout manager
    pub fn set_layout_manager(&self, manager: Box<dyn LayoutManager>) {
        self.layout_manager.replace(manager);
        *self.needs_layout.borrow_mut() = true;
    }

    /// Notify data changed
    pub fn notify_data_set_changed(&self) {
        *self.needs_layout.borrow_mut() = true;
    }

    /// Scroll to position
    pub fn scroll_to_position(&self, position: usize) {
        let offset = self.layout_manager.borrow().get_scroll_for_position(position);
        *self.scroll_y.borrow_mut() = offset.min(*self.max_scroll.borrow()).max(0);
        *self.needs_layout.borrow_mut() = true;
    }

    /// Set item click listener
    pub fn set_on_item_click<F>(&self, callback: F)
    where
        F: Fn(usize) + 'static
    {
        *self.on_item_click.borrow_mut() = Some(Box::new(callback));
    }

    /// Scroll by delta
    pub fn scroll_by(&self, delta: i32) {
        let mut scroll = self.scroll_y.borrow_mut();
        let max = *self.max_scroll.borrow();
        *scroll = (*scroll + delta).clamp(0, max);
    }

    /// Get item at position
    fn get_item_at_position(&self, x: i32, y: i32) -> Option<usize> {
        for holder in self.attached_holders.borrow().iter() {
            let rect = holder.item_view.borrow().get_rect();
            if rect.hit((x, y)) {
                return Some(holder.get_position());
            }
        }
        None
    }

    /// Internal: Recycle off-screen views and bind visible ones
    fn recycle_and_fill(&self, typeface: &Typeface, scale: f64) {
        // Clear needs_layout flag at start (it may be set again if heights change)
        *self.needs_layout.borrow_mut() = false;

        // Check if adapter exists
        if self.adapter.borrow().is_none() {
            return;
        }

        let self_rect = self.state.borrow().rect;
        let padding = self.get_padding(scale);
        let viewport = rect(
            (0, 0),
            (self_rect.width() - padding.left - padding.right,
             self_rect.height() - padding.top - padding.bottom)
        );
        let scroll_y = *self.scroll_y.borrow();
        // Layout manager expects positive scroll offset (absolute position in content)
        // but scroll_y is negative (List convention), so negate it
        let scroll_offset = -scroll_y;

        // Get layout for visible range (need to borrow adapter temporarily)
        let layouts = {
            let adapter_ref = self.adapter.borrow();
            let adapter = adapter_ref.as_ref().unwrap();
            self.layout_manager.borrow_mut().layout_items(
                adapter.get_item_count(),
                viewport,
                scroll_offset,
                adapter.as_ref()
            )
        };

        // Build set of visible positions
        let visible_positions: HashSet<_> =
            layouts.iter().map(|l| l.position).collect();

        // Recycle off-screen holders
        {
            let adapter_ref = self.adapter.borrow();
            let adapter = adapter_ref.as_ref().unwrap();
            let mut attached = self.attached_holders.borrow_mut();
            let recycler = &self.recycler;

            attached.retain(|holder| {
                let pos = holder.get_position();
                if visible_positions.contains(&pos) {
                    true // Keep
                } else {
                    // Recycle
                    adapter.on_view_recycled(holder);
                    recycler.recycle(holder.clone());
                    false
                }
            });
        }

        // Get/create holders for visible items
        for layout in layouts {
            // Check if already attached
            {
                let attached = self.attached_holders.borrow();
                if attached.iter().any(|h| h.get_position() == layout.position) {
                    continue;
                }
            }

            let view_type = layout.view_type;

            // Try to get from pool or create new
            let holder = {
                let mut adapter_ref = self.adapter.borrow_mut();
                let adapter = adapter_ref.as_mut().unwrap();

                self.recycler.acquire(view_type)
                    .unwrap_or_else(|| adapter.create_view_holder(view_type))
            };

            holder.set_position(layout.position);

            // Bind data
            {
                let adapter_ref = self.adapter.borrow();
                let adapter = adapter_ref.as_ref().unwrap();
                adapter.bind_view_holder(&holder, layout.position);
            }

            // Layout the view
            holder.item_view.borrow_mut().layout_content(
                layout.rect.min.x,
                layout.rect.min.y,
                layout.rect.width(),
                layout.rect.height(),
                typeface,
                scale
            );

            // Measure actual height after layout and update LayoutManager
            let actual_height = {
                let view = holder.item_view.borrow();
                let (_, h) = view.calculate_full_size(scale);
                h
            };

            // Update the layout manager with the measured height
            let old_height = self.layout_manager.borrow().get_item_height(layout.position);
            if old_height != actual_height {
                self.layout_manager.borrow().update_item_height(layout.position, actual_height);
                *self.needs_layout.borrow_mut() = true;
            }

            self.attached_holders.borrow_mut().push(holder);
        }

        // If heights changed, immediately relayout with correct positions
        if *self.needs_layout.borrow() {
            *self.needs_layout.borrow_mut() = false; // Clear to avoid infinite loop

            // Get item count
            let item_count = self.adapter.borrow().as_ref().unwrap().get_item_count();

            // Force update of cumulative positions by calling layout_items (this rebuilds the cache)
            {
                let adapter_ref = self.adapter.borrow();
                let adapter = adapter_ref.as_ref().unwrap();
                self.layout_manager.borrow_mut().layout_items(
                    item_count,
                    viewport,
                    scroll_offset,
                    adapter.as_ref()
                );
            }

            // Update positions of all attached holders
            for holder in self.attached_holders.borrow().iter() {
                let pos = holder.get_position();

                // Calculate absolute Y position for this item (scroll applied in paint)
                let y = self.layout_manager.borrow().get_scroll_for_position(pos);
                let height = self.layout_manager.borrow().get_item_height(pos);

                holder.item_view.borrow_mut().layout_content(
                    0,
                    y,
                    viewport.width(),
                    height,
                    typeface,
                    scale
                );
            }
        }

        // Update max scroll
        let item_count = self.adapter.borrow().as_ref().unwrap().get_item_count();
        let (_, content_height) = self.layout_manager.borrow().get_content_size(item_count);
        *self.max_scroll.borrow_mut() = (content_height - viewport.height()).max(0);
    }
}

impl View for RecyclerView {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            return;
        }
        // No RecyclerView-specific properties yet
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.state.borrow_mut().font_manager.set(Some(typeface.clone()));
        self.base_set_scale(scale);

        let (new_width, new_height) = self.calculate_size(width, height, scale);
        let (width, height) = {
            let state = self.state.borrow();
            let ww = match &state.width {
                Dimension::Min => 0,
                Dimension::Max => new_width,
                Dimension::Dip(dip) => *dip as i32,
                Dimension::Percent(p) => (width as f32 * p / 100f32).round() as i32
            };
            let hh = match &state.height {
                Dimension::Min => 0,
                Dimension::Max => new_height,
                Dimension::Dip(dip) => *dip as i32,
                Dimension::Percent(p) => (height as f32 * p / 100f32).round() as i32
            };
            (ww, hh)
        };

        let rect = rect((x, y), (x + width, y + height));
        self.set_rect(rect);

        // Trigger recycle and fill
        if *self.needs_layout.borrow() {
            self.recycle_and_fill(typeface, scale);
        }

        rect
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        let rect = self.get_rect();
        rect.width() <= width && rect.height() <= height
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        let mut rect = self.get_rect();
        let start = rect.min + origin;
        rect.move_by(origin);

        theme.push_clip();
        theme.clip_rect(rect);

        theme.draw_list_back(rect, self.get_state().unwrap());

        let padding = self.get_padding(self.state.borrow().scale);
        let scroll_x = *self.scroll_x.borrow();
        let scroll_y = *self.scroll_y.borrow();
        // Items are positioned at absolute positions, apply scroll offset here
        // Like List: add scroll_y (which is negative or 0)
        let content_start = Point::from((start.x + padding.left + scroll_x, start.y + padding.top + scroll_y));

        for holder in self.attached_holders.borrow().iter() {
            // Item's rect contains absolute position, scroll is applied via content_start
            holder.item_view.borrow().paint(content_start, theme);
        }

        theme.draw_list_body(rect, self.get_state().unwrap());
        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().state)
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

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        // Return content size matching what was set in the rect
        // This is used by Frame to calculate positioning of adjacent views
        let state = self.state.borrow();
        let scale = state.scale;
        let width = match &state.width {
            Dimension::Dip(dip) => *dip as i32,  // Unscaled, matching layout_content
            _ => {
                // For Max/Min/Percent, use the current rect size minus padding
                let rect = self.get_rect();
                let padding = self.get_padding(scale);
                (rect.width() - padding.left - padding.right).max(0)
            }
        };
        let height = match &state.height {
            Dimension::Dip(dip) => *dip as i32,  // Unscaled, matching layout_content
            _ => {
                let rect = self.get_rect();
                let padding = self.get_padding(scale);
                (rect.height() - padding.top - padding.bottom).max(0)
            }
        };
        (width, height)
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

    fn on_event(&mut self, _event: EventType, _func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        // TODO: implement
    }

    fn click(&self, _ui: &mut UI) -> bool {
        false
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.state.borrow().rect.hit((position.x, position.y)) {
            return false;
        }

        if matches!(button, MouseButton::Left) {
            self.state.borrow_mut().state.pressed = true;
            self.state.borrow_mut().state.focused = true;

            let rect = self.state.borrow().rect;
            let local_x = position.x - rect.min.x;
            let local_y = position.y - rect.min.y;

            if let Some(pos) = self.get_item_at_position(local_x, local_y) {
                *self.selected_position.borrow_mut() = Some(pos);

                if let Some(ref callback) = *self.on_item_click.borrow() {
                    callback(pos);
                }
            }

            return true;
        }

        false
    }

    fn on_mouse_button_up(&self, _ui: &mut UI, _position: Vector2<i32>, button: MouseButton) -> bool {
        if matches!(button, MouseButton::Left) {
            if self.state.borrow().state.pressed {
                self.state.borrow_mut().state.pressed = false;
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Vector2<i32>, distance: MouseScrollDistance) -> bool {
        if self.state.borrow().rect.hit((position.x, position.y)) {
            let mut scroll_y = *self.scroll_y.borrow();

            // Get default item height for scroll step
            let scroll_step = self.layout_manager.borrow().get_item_height(0).max(20);

            // Calculate max_scroll (negative value)
            let item_count = self.adapter.borrow().as_ref().map(|a| a.get_item_count()).unwrap_or(0);
            let (_, content_height) = self.layout_manager.borrow().get_content_size(item_count);
            let scale = self.state.borrow().scale;
            let padding = self.get_padding(scale);
            let viewport_height = self.state.borrow().rect.height();
            let available_height = viewport_height - padding.top - padding.bottom;
            let max_scroll = -(content_height - available_height).max(0);

            match &distance {
                MouseScrollDistance::Lines { y, .. } => {
                    // Scroll by lines (positive y = scroll down, negative = scroll up)
                    scroll_y += (*y as i32) * scroll_step;
                }
                MouseScrollDistance::Pixels { y, .. } => {
                    // Scroll by pixels
                    scroll_y += *y as i32;
                }
                MouseScrollDistance::Pages { y, .. } => {
                    // Scroll by pages
                    let page_scroll = self.state.borrow().rect.height();
                    scroll_y += (*y as i32) * page_scroll;
                }
            }

            // Clamp scroll to valid range (max_scroll <= scroll_y <= 0)
            scroll_y = scroll_y.clamp(max_scroll, 0);

            if scroll_y != *self.scroll_y.borrow() {
                *self.scroll_y.borrow_mut() = scroll_y;

                // Recalculate which items are visible and recycle/fill as needed
                let scale = self.state.borrow().scale;
                let typeface = self.state.borrow().font_manager.get().unwrap_or_else(|| {
                    use super::super::themes::Typeface;
                    Typeface::default()
                });
                self.recycle_and_fill(&typeface, scale);
            }

            true
        } else {
            false
        }
    }

    fn on_key_down(&self, _ui: &mut UI, virtual_key_code: Option<VirtualKeyCode>, _scancode: KeyScancode, _state: ModifiersState) -> bool {
        if let Some(key) = virtual_key_code {
            let scroll_step = 50; // Pixels to scroll per key press
            let old_scroll = *self.scroll_y.borrow();

            // Calculate max_scroll (negative value, like List does)
            let item_count = self.adapter.borrow().as_ref().map(|a| a.get_item_count()).unwrap_or(0);
            let (_, content_height) = self.layout_manager.borrow().get_content_size(item_count);
            let scale = self.state.borrow().scale;
            let padding = self.get_padding(scale);
            let viewport_height = self.state.borrow().rect.height();
            let available_height = viewport_height - padding.top - padding.bottom;
            let max_scroll = -(content_height - available_height).max(0);

            let mut new_scroll = old_scroll;

            match key {
                VirtualKeyCode::Up => {
                    // Scrolling up means decreasing scroll_y (more negative)
                    new_scroll = (old_scroll - scroll_step).max(max_scroll);
                }
                VirtualKeyCode::Down => {
                    // Scrolling down means increasing scroll_y (less negative, toward 0)
                    new_scroll = (old_scroll + scroll_step).min(0);
                }
                VirtualKeyCode::PageUp => {
                    let page_scroll = self.state.borrow().rect.height();
                    new_scroll = (old_scroll - page_scroll).max(max_scroll);
                }
                VirtualKeyCode::PageDown => {
                    let page_scroll = self.state.borrow().rect.height();
                    new_scroll = (old_scroll + page_scroll).min(0);
                }
                VirtualKeyCode::Home => {
                    // Home goes to top (scroll_y = 0)
                    new_scroll = 0;
                }
                VirtualKeyCode::End => {
                    // End goes to bottom (scroll_y = max_scroll, which is negative)
                    new_scroll = max_scroll;
                }
                _ => {
                    return false;
                }
            }

            // If scroll changed, update and recalculate visible items
            if new_scroll != old_scroll {
                *self.scroll_y.borrow_mut() = new_scroll;

                // Recalculate which items are visible and recycle/fill as needed
                let scale = self.state.borrow().scale;
                let typeface = self.state.borrow().font_manager.get().unwrap_or_else(|| {
                    // Fallback typeface if not set
                    use super::super::themes::Typeface;
                    Typeface::default()
                });
                self.recycle_and_fill(&typeface, scale);

                return true;
            }
        }
        true
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        // Return true if we need to trigger a relayout/repaint
        *self.needs_layout.borrow()
    }
}

impl Default for RecyclerView {
    fn default() -> Self {
        let rect = rect((0, 0), (100, 200));
        RecyclerView::new(rect)
    }
}
