use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use downcast_rs::{impl_downcast, Downcast};
use crate::input::{KeyScancode, ModifiersState, MouseButton, MouseScrollDistance, VirtualKeyCode};
use super::super::events::{EventCallback, EventData, EventType};
use super::super::themes::{Theme, Typeface, ViewState};
use super::super::traits::{Element, View, WeakElement};
use super::super::types::{Point, Rect, rect};
use super::super::ui::UI;
use super::super::views::{Borders, Dimension, FieldsMain, Gravity, Visibility};
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

pub trait RecyclerAdapter: Downcast {
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
impl_downcast!(RecyclerAdapter);

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

    /// Notify the layout manager that `count` items were inserted at `position`.
    /// Default: no-op.
    fn items_inserted(&self, _position: usize, _count: usize) {}

    /// Notify the layout manager that `count` items were removed starting at `position`.
    /// Default: no-op.
    fn items_removed(&self, _position: usize, _count: usize) {}

    /// Notify the layout manager that the item at `from` moved to `to`.
    /// Default: no-op.
    fn items_moved(&self, _from: usize, _to: usize) {}

    /// Invalidate cached layout for `position` so it is remeasured on next bind.
    /// Default: no-op.
    fn invalidate_item(&self, _position: usize) {}
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

    fn items_inserted(&self, position: usize, count: usize) {
        if count == 0 {
            return;
        }
        let mut heights = self.item_heights.borrow_mut();
        let pos = position.min(heights.len());
        heights.splice(pos..pos, std::iter::repeat_n(self.default_item_height, count));

        let mut cumulative = self.cumulative_positions.borrow_mut();
        if cumulative.len() > pos {
            cumulative.truncate(pos);
        }
    }

    fn items_removed(&self, position: usize, count: usize) {
        if count == 0 {
            return;
        }
        let mut heights = self.item_heights.borrow_mut();
        if position >= heights.len() {
            return;
        }
        let end = (position + count).min(heights.len());
        heights.drain(position..end);

        let mut cumulative = self.cumulative_positions.borrow_mut();
        if cumulative.len() > position {
            cumulative.truncate(position);
        }
    }

    fn items_moved(&self, from: usize, to: usize) {
        if from == to {
            return;
        }
        let mut heights = self.item_heights.borrow_mut();
        if from >= heights.len() || to >= heights.len() {
            return;
        }
        let h = heights.remove(from);
        heights.insert(to, h);

        let mut cumulative = self.cumulative_positions.borrow_mut();
        let trunc_at = from.min(to);
        if cumulative.len() > trunc_at {
            cumulative.truncate(trunc_at);
        }
    }

    fn invalidate_item(&self, position: usize) {
        let mut cumulative = self.cumulative_positions.borrow_mut();
        if cumulative.len() > position {
            cumulative.truncate(position);
        }
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

    #[allow(dead_code)]
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

    /// Run a closure with mutable access to the adapter, downcast to a concrete type `A`.
    /// Returns `None` if no adapter is set or the adapter's type does not match `A`.
    ///
    /// The closure must finish (be dropped) before calling any `notify_item_*` method,
    /// since those methods need to borrow the adapter immutably.
    pub fn with_adapter_as<A, R, F>(&self, f: F) -> Option<R>
    where
        A: RecyclerAdapter,
        F: FnOnce(&mut A) -> R,
    {
        let mut guard = self.adapter.borrow_mut();
        let adapter = guard.as_mut()?;
        adapter.downcast_mut::<A>().map(f)
    }

    /// Run a closure with mutable access to the adapter as a trait object.
    /// Returns `None` if no adapter is set.
    ///
    /// The closure must finish before calling any `notify_item_*` method.
    pub fn with_adapter_mut<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut dyn RecyclerAdapter) -> R,
    {
        let mut guard = self.adapter.borrow_mut();
        guard.as_deref_mut().map(f)
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

    /// Notify that the item at `position` has changed and needs to be rebound.
    /// If the item is currently visible, it is rebound and remeasured.
    pub fn notify_item_changed(&self, position: usize) {
        self.notify_item_range_changed(position, 1);
    }

    /// Rebind the item at `position` without invalidating the layout. Use this
    /// when the row's content changes but its measured size is guaranteed to
    /// stay the same — e.g. swapping a fixed-size icon. Does nothing if no
    /// holder is currently attached at that position.
    pub fn rebind_item(&self, position: usize) {
        let holder = self.attached_holders.borrow()
            .iter()
            .find(|h| h.get_position() == position)
            .cloned();
        if let Some(holder) = holder {
            let adapter_ref = self.adapter.borrow();
            if let Some(adapter) = adapter_ref.as_ref() {
                adapter.bind_view_holder(&holder, position);
            }
        }
    }

    /// Notify that `count` items starting at `position` have changed.
    /// Visible items in the range are rebound and remeasured.
    pub fn notify_item_range_changed(&self, position: usize, count: usize) {
        if count == 0 {
            return;
        }
        if self.adapter.borrow().is_none() {
            return;
        }

        let end = position + count;

        // Snapshot affected holders so we can drop the attached_holders borrow
        // before calling adapter.bind_view_holder.
        let affected: Vec<ViewHolder> = self.attached_holders.borrow()
            .iter()
            .filter(|h| {
                let p = h.get_position();
                p >= position && p < end
            })
            .cloned()
            .collect();

        if !affected.is_empty() {
            let adapter_ref = self.adapter.borrow();
            let adapter = adapter_ref.as_ref().unwrap();
            for holder in &affected {
                let p = holder.get_position();
                adapter.bind_view_holder(holder, p);
            }
        }

        // Invalidate cached cumulative positions from `position` onward so heights
        // are re-summed once the items remeasure on the next paint.
        {
            let lm = self.layout_manager.borrow();
            lm.invalidate_item(position);
        }
        *self.needs_layout.borrow_mut() = true;
    }

    /// Notify that a single item was inserted at `position`.
    pub fn notify_item_inserted(&self, position: usize) {
        self.notify_item_range_inserted(position, 1);
    }

    /// Notify that `count` items were inserted starting at `position`.
    /// Adjusts attached holders' positions and `selected_position` accordingly.
    pub fn notify_item_range_inserted(&self, position: usize, count: usize) {
        if count == 0 {
            return;
        }
        if self.adapter.borrow().is_none() {
            return;
        }

        debug_assert!(
            position <= self.adapter.borrow().as_ref().unwrap().get_item_count(),
            "notify_item_range_inserted: position {} out of bounds (item_count={})",
            position,
            self.adapter.borrow().as_ref().unwrap().get_item_count()
        );

        self.layout_manager.borrow().items_inserted(position, count);

        for holder in self.attached_holders.borrow().iter() {
            let p = holder.get_position();
            if p >= position {
                holder.set_position(p + count);
            }
        }

        {
            let mut sel = self.selected_position.borrow_mut();
            if let Some(s) = *sel {
                if s >= position {
                    *sel = Some(s + count);
                }
            }
        }

        *self.needs_layout.borrow_mut() = true;
    }

    /// Notify that a single item was removed at `position`.
    pub fn notify_item_removed(&self, position: usize) {
        self.notify_item_range_removed(position, 1);
    }

    /// Notify that `count` items were removed starting at `position`.
    /// Recycles holders for removed items, shifts positions for following items,
    /// and adjusts `selected_position`.
    pub fn notify_item_range_removed(&self, position: usize, count: usize) {
        if count == 0 {
            return;
        }
        if self.adapter.borrow().is_none() {
            return;
        }

        let end = position + count;

        self.layout_manager.borrow().items_removed(position, count);

        // Partition attached_holders: recycle those in the removed range,
        // shift positions for those past it.
        let removed: Vec<ViewHolder> = {
            let mut attached = self.attached_holders.borrow_mut();
            let mut removed = Vec::new();
            attached.retain(|h| {
                let p = h.get_position();
                if p >= position && p < end {
                    removed.push(h.clone());
                    false
                } else {
                    if p >= end {
                        h.set_position(p - count);
                    }
                    true
                }
            });
            removed
        };

        if !removed.is_empty() {
            let adapter_ref = self.adapter.borrow();
            let adapter = adapter_ref.as_ref().unwrap();
            for holder in removed {
                adapter.on_view_recycled(&holder);
                self.recycler.recycle(holder);
            }
        }

        {
            let mut sel = self.selected_position.borrow_mut();
            if let Some(s) = *sel {
                if s >= position && s < end {
                    *sel = None;
                } else if s >= end {
                    *sel = Some(s - count);
                }
            }
        }

        *self.needs_layout.borrow_mut() = true;
    }

    /// Notify that the item at `from` has moved to `to`.
    /// Updates positions on attached holders in place (no full relayout) and
    /// rebinds the moved holder if it remains visible.
    pub fn notify_item_moved(&self, from: usize, to: usize) {
        if from == to {
            return;
        }
        if self.adapter.borrow().is_none() {
            return;
        }

        self.layout_manager.borrow().items_moved(from, to);

        let shift = |p: usize| -> usize {
            if p == from {
                to
            } else if from < to && p > from && p <= to {
                p - 1
            } else if from > to && p >= to && p < from {
                p + 1
            } else {
                p
            }
        };

        // Apply position shifts and capture the moved holder (if attached) so we can
        // rebind it under its new position outside the attached_holders borrow.
        let moved_holder: Option<ViewHolder> = {
            let attached = self.attached_holders.borrow();
            let mut moved = None;
            for holder in attached.iter() {
                let old = holder.get_position();
                let new = shift(old);
                if old != new {
                    holder.set_position(new);
                }
                if old == from {
                    moved = Some(holder.clone());
                }
            }
            moved
        };

        if let Some(holder) = moved_holder {
            let adapter_ref = self.adapter.borrow();
            let adapter = adapter_ref.as_ref().unwrap();
            adapter.bind_view_holder(&holder, to);
        }

        {
            let mut sel = self.selected_position.borrow_mut();
            if let Some(s) = *sel {
                *sel = Some(shift(s));
            }
        }

        *self.needs_layout.borrow_mut() = true;
    }

    /// The selected adapter position, if any (kept consistent across
    /// insert/remove/move notifications).
    pub fn get_selected_position(&self) -> Option<usize> {
        *self.selected_position.borrow()
    }

    /// Scroll so the item at `position` is at the top of the viewport.
    /// `scroll_y` is stored as a non-positive offset (0 = top, -max_scroll = bottom)
    /// to match the convention used by paint and the wheel handler.
    pub fn scroll_to_position(&self, position: usize) {
        let offset = self.layout_manager.borrow().get_scroll_for_position(position);
        let max = *self.max_scroll.borrow();
        *self.scroll_y.borrow_mut() = (-offset).max(-max).min(0);
        *self.needs_layout.borrow_mut() = true;
    }

    /// Scroll all the way to the end of the content.
    pub fn scroll_to_end(&self) {
        let max = *self.max_scroll.borrow();
        *self.scroll_y.borrow_mut() = -max;
        *self.needs_layout.borrow_mut() = true;
    }

    /// True when the viewport is at (or past) the bottom of the content.
    /// Useful for "stick to bottom" patterns like chat lists.
    pub fn is_scrolled_to_end(&self) -> bool {
        *self.scroll_y.borrow() <= -*self.max_scroll.borrow()
    }

    /// Set item click listener
    pub fn set_on_item_click<F>(&self, callback: F)
    where
        F: Fn(usize) + 'static
    {
        *self.on_item_click.borrow_mut() = Some(Box::new(callback));
    }

    /// Scroll by delta. `scroll_y` is non-positive; `delta` follows the same sign
    /// convention used by paint (negative = scroll content up to reveal lower items).
    pub fn scroll_by(&self, delta: i32) {
        let mut scroll = self.scroll_y.borrow_mut();
        let max = *self.max_scroll.borrow();
        *scroll = (*scroll + delta).clamp(-max, 0);
    }

    /// Adapter position of the item under absolute window coordinates —
    /// e.g. the `Position` payload of a `ContextMenu` event. `None` when the
    /// point is outside every attached item. Rects are parent-relative, so
    /// the window coordinates are translated down the ancestor chain first.
    pub fn item_at(&self, x: i32, y: i32) -> Option<usize> {
        let (mut ox, mut oy) = {
            let r = self.state.borrow().rect;
            (r.min.x, r.min.y)
        };
        let mut parent = self.get_parent();
        while let Some(p) = parent {
            let r = p.borrow().get_rect();
            ox += r.min.x;
            oy += r.min.y;
            parent = p.borrow().get_parent();
        }
        self.get_item_at_position(x - ox, y - oy)
    }

    /// Get item at position (view-local coordinates). Holder rects live in
    /// content space, so the point is translated first.
    fn get_item_at_position(&self, x: i32, y: i32) -> Option<usize> {
        let (cx, cy) = self.local_to_content(x, y);
        for holder in self.attached_holders.borrow().iter() {
            let rect = holder.item_view.borrow().get_rect();
            if rect.hit((cx, cy)) {
                return Some(holder.get_position());
            }
        }
        None
    }

    /// Translate view-local coordinates into item content space — the inverse
    /// of the padding + scroll translation `paint` applies to holder rects.
    fn local_to_content(&self, x: i32, y: i32) -> (i32, i32) {
        let padding = self.get_padding(self.state.borrow().scale);
        (
            x - padding.left - *self.scroll_x.borrow(),
            y - padding.top - *self.scroll_y.borrow(),
        )
    }

    /// Dispatch a Click to the deepest view inside the item under `(cx, cy)`
    /// (content space) that has a Click listener. Returns true when a listener
    /// consumed the click — the row-level `on_item_click` should then be
    /// skipped. This is what lets per-row buttons inside recycled item layouts
    /// receive clicks.
    ///
    /// NOTE for listeners: dispatch still holds borrows up the view tree, so
    /// a listener must not `borrow_mut()` this RecyclerView or its ancestors
    /// directly — defer via `ui.handle().run_on_ui_thread(..)` instead.
    fn fire_child_click(&self, ui: &mut UI, cx: i32, cy: i32) -> bool {
        let item = self
            .attached_holders
            .borrow()
            .iter()
            .find(|h| h.item_view.borrow().get_rect().hit((cx, cy)))
            .map(|h| h.item_view.clone());
        match item {
            Some(item) => Self::click_descend(ui, &item, cx, cy),
            None => false,
        }
    }

    /// Depth-first Click dispatch; `(px, py)` is in the view's parent space.
    fn click_descend(ui: &mut UI, view: &Element, px: i32, py: i32) -> bool {
        {
            let v = view.borrow();
            if v.get_visibility() != Visibility::Visible || !v.get_rect().hit((px, py)) {
                return false;
            }
        }
        let rect = view.borrow().get_rect();
        let (lx, ly) = (px - rect.min.x, py - rect.min.y);
        let children = view
            .borrow()
            .as_container()
            .map(|c| c.get_views())
            .unwrap_or_default();
        for child in children.iter().rev() {
            if Self::click_descend(ui, child, lx, ly) {
                return true;
            }
        }
        let v = view.borrow();
        if v.has_listener(EventType::Click) {
            v.fire_event(ui, EventType::Click, &EventData::Position { x: lx, y: ly })
        } else {
            false
        }
    }

    /// Internal: Recycle off-screen views and bind visible ones
    fn recycle_and_fill(&self, typeface: &Typeface, scale: f64) {
        // Remember if a full relayout was requested (e.g. due to resize)
        let full_relayout = *self.needs_layout.borrow();
        // Clear needs_layout flag at start (it may be set again if heights change)
        *self.needs_layout.borrow_mut() = false;

        // Check if adapter exists
        if self.adapter.borrow().is_none() {
            return;
        }

        // On full relayout (e.g. resize), recycle ALL attached holders so they get
        // freshly created and bound. This is necessary because text views cache their
        // laid-out text and skip re-layout when layout_content is called again.
        if full_relayout {
            let adapter_ref = self.adapter.borrow();
            let adapter = adapter_ref.as_ref().unwrap();
            let mut attached = self.attached_holders.borrow_mut();
            for holder in attached.drain(..) {
                adapter.on_view_recycled(&holder);
                self.recycler.recycle(holder);
            }
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

            // Recompute the visible range with the now-measured heights and rebuild
            // the position cache. The first pass used `default_item_height` (a
            // guess), so when items are taller than that guess it attaches far more
            // holders than actually fit (e.g. default 24 vs real 95 → ~4x). Recycle
            // the holders that fall outside the corrected range, otherwise we keep
            // laying out and painting dozens of off-screen items every frame.
            let corrected = {
                let adapter_ref = self.adapter.borrow();
                let adapter = adapter_ref.as_ref().unwrap();
                self.layout_manager.borrow_mut().layout_items(
                    item_count,
                    viewport,
                    scroll_offset,
                    adapter.as_ref()
                )
            };
            let visible: HashSet<usize> = corrected.iter().map(|l| l.position).collect();
            {
                let adapter_ref = self.adapter.borrow();
                let adapter = adapter_ref.as_ref().unwrap();
                let mut attached = self.attached_holders.borrow_mut();
                attached.retain(|holder| {
                    if visible.contains(&holder.get_position()) {
                        true
                    } else {
                        adapter.on_view_recycled(holder);
                        self.recycler.recycle(holder.clone());
                        false
                    }
                });
            }

            // Update positions of the (now culled) attached holders
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
                Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => (width as f32 * p / 100f32).round() as i32
            };
            let hh = match &state.height {
                Dimension::Min => 0,
                Dimension::Max => new_height,
                Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
                Dimension::Percent(p) => (height as f32 * p / 100f32).round() as i32
            };
            (ww, hh)
        };

        let rect = rect((x, y), (x + width, y + height));
        let old_rect = self.get_rect();
        self.set_rect(rect);

        // Trigger re-layout when size changes
        if old_rect.width() != rect.width() || old_rect.height() != rect.height() {
            *self.needs_layout.borrow_mut() = true;
        }

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

        // Step 1: Draw background (before items). An explicit `background`
        // attribute overrides the default sunken-edit surface, letting apps
        // give a list its own pane color (e.g. a chat timeline).
        match self.get_background() {
            Some(color) => theme.draw_rect(rect, color),
            None => theme.draw_component("edit.back", rect, self.get_state().unwrap()),
        }

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

        // Step 2: Draw borders (after items)
        theme.draw_component("edit.body", rect, self.get_state().unwrap());

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
        // Return content size matching what was set in the rect
        // This is used by Frame to calculate positioning of adjacent views
        let state = self.state.borrow();
        let scale = state.scale;
        let width = match &state.width {
            Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
            _ => {
                // For Max/Min/Percent, use the current rect size minus padding
                let rect = self.get_rect();
                let padding = self.get_padding(scale);
                (rect.width() - padding.left - padding.right).max(0)
            }
        };
        let height = match &state.height {
            Dimension::Dip(dip) => (*dip as f64 * scale).round() as i32,
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
        let mut node = accesskit::Node::new(accesskit::Role::List);
        if let Some(adapter) = self.adapter.borrow().as_ref() {
            node.set_size_of_set(adapter.get_item_count());
        }
        node
    }

    /// Expose the currently realized rows (RecyclerView is virtualized: only
    /// attached holders exist as views). Their rects are content-space, so
    /// the offset carries padding + scroll, mirroring paint.
    fn accessibility_child_elements(&self) -> Vec<(Element, Point<i32>)> {
        let padding = self.get_padding(self.state.borrow().scale);
        let offset = Point::new(
            padding.left + *self.scroll_x.borrow(),
            padding.top + *self.scroll_y.borrow(),
        );
        let mut holders: Vec<(usize, Element)> = self.attached_holders.borrow()
            .iter()
            .map(|h| (h.get_position(), Rc::clone(&h.item_view)))
            .collect();
        // Attachment order is recycling order; ATs want adapter order.
        holders.sort_by_key(|(position, _)| *position);
        holders.into_iter().map(|(_, element)| (element, offset)).collect()
    }

    fn click(&self, _ui: &mut UI) -> bool {
        if !self.base_is_enabled() { return false; }
        false
    }

    fn update(&mut self, _ui: &mut UI) -> bool {
        // Return true if we need to trigger a relayout/repaint
        *self.needs_layout.borrow()
    }

    fn on_mouse_button_down(&self, ui: &mut UI, position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
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

                // Child views with Click listeners (per-row buttons) get first
                // shot; the row-level callback fires only when none consumed it.
                let (cx, cy) = self.local_to_content(local_x, local_y);
                if !self.fire_child_click(ui, cx, cy)
                    && let Some(ref callback) = *self.on_item_click.borrow() {
                        callback(pos);
                    }
            }

            return true;
        }

        false
    }

    fn on_mouse_button_up(&self, _ui: &mut UI, _position: Point<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() { return false; }
        if matches!(button, MouseButton::Left) {
            if self.state.borrow().state.pressed {
                self.state.borrow_mut().state.pressed = false;
                return true;
            }
        }
        false
    }

    fn on_mouse_wheel_scroll(&self, _ui: &mut UI, position: Point<i32>, distance: MouseScrollDistance) -> bool {
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

            let new_scroll;

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
}

impl Default for RecyclerView {
    fn default() -> Self {
        let rect = rect((0, 0), (100, 200));
        RecyclerView::new(rect)
    }
}
