//! Screen-reader support: builds a per-window AccessKit tree from the view
//! hierarchy and routes assistive-technology action requests back into the UI.
//!
//! The tree is rebuilt in full on every push — required by the event-loop-proxy
//! adapter anyway, and only paid while a screen reader is actually connected
//! (`Adapter::update_if_active` skips the closure otherwise). The walk mirrors
//! `UI::hit_test_element`: it skips non-`Visible` views, descends through
//! `Container::hit_test_views()` (so e.g. a `TabView`'s inactive tabs are
//! excluded), and accumulates parent offsets to produce absolute window-space
//! bounds.
//!
//! Node identity: AccessKit wants a `u64` per node; Lumio views carry string
//! ids (user-assigned or random-per-construction), stable for the lifetime of
//! the view — which is all AccessKit needs. We hash them with FNV-1a.
//! `ROOT_NODE_ID` (0) is reserved for the synthetic window node.

use std::collections::HashSet;
use std::rc::Rc;

use accesskit::{Action, ActionData, ActionRequest, Node, NodeId, Role, Tree, TreeUpdate};

use crate::events::{EventData, EventType};
use crate::input::MouseButton;
use crate::traits::Element;
use crate::types::Point;
use crate::ui::UI;
use crate::views::{Slider, Visibility};

/// The synthetic window-root node: parent of the UI root and all overlays.
pub const ROOT_NODE_ID: NodeId = NodeId(0);

/// Node id for a synthetic per-item child (list row, tab, menu item) of the
/// view with id `view_id`. Index-derived, so it is stable as long as the item
/// keeps its position. Use this from `View::accessibility_children`.
pub fn item_node_id(view_id: &str, index: usize) -> NodeId {
    node_id_for(&format!("{view_id}#{index}"))
}

/// Per-character UTF-8 byte lengths of `text`, for
/// `Node::set_character_lengths` on a text run (a "character" here is one
/// Rust `char`; grapheme clusters are a future refinement).
pub fn character_lengths(text: &str) -> Vec<u8> {
    text.chars().map(|c| c.len_utf8() as u8).collect()
}

/// Character indices where words start in `text`, for
/// `Node::set_word_starts`. The property stores `u8` indices, so starts
/// beyond 255 are dropped (word navigation degrades on extremely long runs).
pub fn word_starts(text: &str) -> Vec<u8> {
    let mut starts = Vec::new();
    let mut prev_is_space = true;
    for (i, c) in text.chars().enumerate() {
        let is_space = c.is_whitespace();
        if prev_is_space && !is_space && i <= u8::MAX as usize {
            starts.push(i as u8);
        }
        prev_is_space = is_space;
    }
    starts
}

/// Maps a view's string id to its AccessKit node id (FNV-1a, 64-bit).
/// A hash of 0 is remapped so no view can collide with [`ROOT_NODE_ID`].
pub fn node_id_for(view_id: &str) -> NodeId {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in view_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    if hash == 0 {
        hash = 0x9e37_79b9_7f4a_7c15;
    }
    NodeId(hash)
}

/// Builds the full accessibility tree for one window's UI: a synthetic
/// `Role::Window` root whose children are the root view hierarchy plus every
/// overlay (popups, menus, dropdowns — topmost last), each walked with
/// hit-test visibility rules and absolute window-space bounds.
pub fn build_tree(ui: &UI) -> TreeUpdate {
    let mut walker = Walker { nodes: Vec::new(), seen: HashSet::new() };
    walker.seen.insert(ROOT_NODE_ID);

    let mut top_level = Vec::new();
    if let Some(root) = ui.root_element()
        && let Some(id) = walker.walk(&root, 0, 0)
    {
        top_level.push(id);
    }
    for (overlay, origin) in ui.overlay_elements() {
        if let Some(id) = walker.walk(&overlay, origin.x, origin.y) {
            top_level.push(id);
        }
    }

    let mut window = Node::new(Role::Window);
    window.set_children(top_level);
    walker.nodes.push((ROOT_NODE_ID, window));

    // While a popup menu is open, its hovered/keyboard-selected item is the
    // effective focus (menus track selection via hover, not `focus_owner`).
    // Topmost menu wins. Otherwise report the focused view if it made it into
    // this tree, else fall back to the window node (the platform adapter
    // combines this with the OS-level window focus).
    let menu_focus = ui.overlay_elements().iter().rev().find_map(|(element, _)| {
        let view = element.borrow();
        let menu = view.as_any().downcast_ref::<crate::views::PopupMenu>()?;
        let index = menu.get_hovered_index()?;
        let id = item_node_id(&view.get_id(), index);
        walker.seen.contains(&id).then_some(id)
    });
    let focus = menu_focus
        .or_else(|| {
            ui.access_focus_owner()
                .map(|id| node_id_for(&id))
                .filter(|id| walker.seen.contains(id))
        })
        .unwrap_or(ROOT_NODE_ID);

    let mut tree = Tree::new(ROOT_NODE_ID);
    tree.toolkit_name = Some("Lumio".to_string());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_string());

    // One tree per window; sub-tree support (TreeId) is unused.
    TreeUpdate { nodes: walker.nodes, tree: Some(tree), tree_id: accesskit::TreeId::ROOT, focus }
}

/// Routes an assistive-technology action request back into the UI. Returns
/// `true` when the UI changed and a redraw (which re-pushes the tree) should
/// be requested.
///
/// `Click` is delivered as a synthetic mouse press+release at the target's
/// window-space center, through the ordinary dispatch — press states, popup
/// dismissal and listeners behave exactly as for a real click. `Focus` goes
/// through [`UI::set_focus_to`]. `SetValue`/`Increment`/`Decrement` drive a
/// `Slider` the same way its keyboard path does.
pub fn perform_action(ui: &mut UI, request: &ActionRequest) -> bool {
    // Lumio publishes a single tree per window (TreeId::ROOT).
    if request.target_tree != accesskit::TreeId::ROOT {
        return false;
    }
    let Some(target) = resolve_target(ui, request.target_node) else {
        return false;
    };
    match request.action {
        Action::Focus => ui.set_focus_to(target.element()),
        Action::Click => {
            {
                let view = target.element().borrow();
                if !view.is_enabled() {
                    return false;
                }
            }
            let (ActionTarget::View { center, .. } | ActionTarget::Item { center: Some(center), .. }) = target else {
                return false;
            };
            // All borrows are released; dispatch like a real click.
            let redraw_down = ui.on_mouse_button_down(center, MouseButton::Left);
            let redraw_up = ui.on_mouse_button_up(center, MouseButton::Left);
            redraw_down | redraw_up
        }
        Action::SetValue => {
            let Some(ActionData::NumericValue(value)) = request.data else {
                return false;
            };
            with_enabled_slider(&target, |view, slider| {
                if slider.set_value(value as f32) {
                    view.fire_event(ui, EventType::ValueChanged, &EventData::Value(slider.get_value()));
                    true
                } else {
                    false
                }
            })
        }
        Action::Increment => with_enabled_slider(&target, |_, slider| slider.nudge(ui, 1)),
        Action::Decrement => with_enabled_slider(&target, |_, slider| slider.nudge(ui, -1)),
        _ => false,
    }
}

/// Runs `f` when the target resolves to an enabled `Slider`; `false` otherwise.
fn with_enabled_slider(
    target: &ActionTarget,
    f: impl FnOnce(&dyn crate::traits::View, &Slider) -> bool,
) -> bool {
    let view = target.element().borrow();
    if !view.is_enabled() {
        return false;
    }
    match view.as_any().downcast_ref::<Slider>() {
        Some(slider) => f(&*view, slider),
        None => false,
    }
}

/// What an AccessKit node id resolved to in the live view tree.
enum ActionTarget {
    /// A real view; `center` is its window-space rect center.
    View { element: Element, center: Point<i32> },
    /// A synthetic per-item child (list row, tab, menu item) of `element`.
    /// `center` is the item's on-screen center, or `None` when the item lies
    /// outside its owning view (scrolled out — a click there would land on
    /// the wrong thing).
    Item { element: Element, center: Option<Point<i32>> },
}

impl ActionTarget {
    fn element(&self) -> &Element {
        match self {
            ActionTarget::View { element, .. } | ActionTarget::Item { element, .. } => element,
        }
    }
}

/// Finds the view (or synthetic item) whose node id is `target`, walking the
/// same visible tree the builder emits. Lazy by design: actions are rare and
/// trees are small, so there is no cached map to go stale.
fn resolve_target(ui: &UI, target: NodeId) -> Option<ActionTarget> {
    if let Some(root) = ui.root_element()
        && let Some(found) = resolve_in(&root, target, 0, 0)
    {
        return Some(found);
    }
    for (overlay, origin) in ui.overlay_elements() {
        if let Some(found) = resolve_in(&overlay, target, origin.x, origin.y) {
            return Some(found);
        }
    }
    None
}

fn resolve_in(element: &Element, target: NodeId, offset_x: i32, offset_y: i32) -> Option<ActionTarget> {
    let view = element.borrow();
    if view.get_visibility() != Visibility::Visible {
        return None;
    }
    let rect = view.get_rect();
    let abs_x = rect.min.x + offset_x;
    let abs_y = rect.min.y + offset_y;

    if node_id_for(&view.get_id()) == target {
        let center = Point::new(abs_x + rect.width() / 2, abs_y + rect.height() / 2);
        return Some(ActionTarget::View { element: Rc::clone(element), center });
    }

    for (child_id, child_node) in view.accessibility_children() {
        if child_id != target {
            continue;
        }
        let center = child_node.bounds().and_then(|b| {
            let cx = abs_x + ((b.x0 + b.x1) / 2.0) as i32;
            let cy = abs_y + ((b.y0 + b.y1) / 2.0) as i32;
            let inside = cx >= abs_x && cx < rect.max.x + offset_x
                && cy >= abs_y && cy < rect.max.y + offset_y;
            inside.then_some(Point::new(cx, cy))
        });
        return Some(ActionTarget::Item { element: Rc::clone(element), center });
    }

    if let Some(container) = view.as_container() {
        for child in container.hit_test_views() {
            if let Some(found) = resolve_in(&child, target, abs_x, abs_y) {
                return Some(found);
            }
        }
    }
    None
}

struct Walker {
    nodes: Vec<(NodeId, Node)>,
    seen: HashSet<NodeId>,
}

impl Walker {
    /// Emits the node for `element` (and, recursively, its visible children)
    /// into `self.nodes`. `offset_x`/`offset_y` is the absolute window
    /// position of the parent's content origin; child rects are
    /// parent-relative, exactly as in `UI::hit_test_element`.
    fn walk(&mut self, element: &Element, offset_x: i32, offset_y: i32) -> Option<NodeId> {
        let view = element.borrow();
        if view.get_visibility() != Visibility::Visible {
            return None;
        }
        let node_id = node_id_for(&view.get_id());
        if !self.seen.insert(node_id) {
            // Duplicate view id (already breaks get_view/focus tracking) or a
            // freak hash collision: a TreeUpdate must not contain the same id
            // twice, so drop this subtree rather than corrupt the tree.
            #[cfg(debug_assertions)]
            eprintln!("accessibility: duplicate node for view id {:?}, subtree skipped", view.get_id());
            return None;
        }

        let rect = view.get_rect();
        let abs_x = rect.min.x + offset_x;
        let abs_y = rect.min.y + offset_y;

        let mut node = view.accessibility_node();
        // A widget marks itself hidden to opt out of the tree entirely
        // (decorative views: Separator, an ImageView with no description).
        if node.is_hidden() {
            self.seen.remove(&node_id);
            return None;
        }
        node.set_bounds(accesskit::Rect {
            x0: abs_x as f64,
            y0: abs_y as f64,
            x1: (rect.max.x + offset_x) as f64,
            y1: (rect.max.y + offset_y) as f64,
        });
        if !view.is_enabled() {
            node.set_disabled();
        }
        // An explicit content_description overrides the widget-derived label.
        if let Some(desc) = view.get_content_description()
            && !desc.is_empty()
        {
            node.set_label(desc);
        }
        // `labelled_by="id"`: name this view by another view's text (the
        // platform adapter derives the name from the referenced node).
        if let Some(label_id) = view.get_labelled_by()
            && !label_id.is_empty()
        {
            node.set_labelled_by(vec![node_id_for(&label_id)]);
        }
        if node.description().is_none()
            && let Some(tooltip) = view.get_tooltip()
            && !tooltip.is_empty()
        {
            node.set_description(tooltip);
        }
        if view.get_state().map(|s| s.focusable).unwrap_or(false) {
            node.add_action(Action::Focus);
        }

        // Real child subtrees. A view that hands out explicit child elements
        // takes manual control of exposure (e.g. TableView positions each
        // cell; the elements' rects live in a space the view describes via
        // the extra offset); otherwise the Container protocol supplies them
        // in natural order (= reading order; hit testing iterates reversed
        // for topmost-first, ATs want document order).
        let mut real_ids = Vec::new();
        let extra_elements = view.accessibility_child_elements();
        if extra_elements.is_empty() {
            if let Some(container) = view.as_container() {
                for child in container.hit_test_views() {
                    if let Some(child_id) = self.walk(&child, abs_x, abs_y) {
                        real_ids.push(child_id);
                    }
                }
            }
        } else {
            for (child, extra) in extra_elements {
                if let Some(child_id) = self.walk(&child, abs_x + extra.x, abs_y + extra.y) {
                    real_ids.push(child_id);
                }
            }
        }

        // Synthetic children (list rows, tabs, menu items, table rows). Their
        // bounds are view-local and get translated here. A synthetic node may
        // claim other emitted nodes as ITS children (a table row grouping its
        // cell subtrees); claimed nodes are re-parented under it instead of
        // hanging directly off this view.
        let mut synthetic = Vec::new();
        for (child_id, child_node) in view.accessibility_children() {
            if !self.seen.insert(child_id) {
                #[cfg(debug_assertions)]
                eprintln!("accessibility: duplicate synthetic node under view id {:?}, item skipped", view.get_id());
                continue;
            }
            synthetic.push((child_id, child_node));
        }
        let mut emitted: HashSet<NodeId> = real_ids.iter().copied().collect();
        emitted.extend(synthetic.iter().map(|(id, _)| *id));
        let mut claimed: HashSet<NodeId> = HashSet::new();
        for (_, child_node) in &synthetic {
            claimed.extend(child_node.children().iter().copied().filter(|c| emitted.contains(c)));
        }

        let mut children = Vec::new();
        for (child_id, mut child_node) in synthetic {
            if let Some(bounds) = child_node.bounds() {
                child_node.set_bounds(accesskit::Rect {
                    x0: bounds.x0 + abs_x as f64,
                    y0: bounds.y0 + abs_y as f64,
                    x1: bounds.x1 + abs_x as f64,
                    y1: bounds.y1 + abs_y as f64,
                });
            }
            // Drop references to nodes that never made it into the tree
            // (hidden cells) — a TreeUpdate must not contain dangling ids.
            let kept: Vec<NodeId> = child_node.children().iter().copied()
                .filter(|c| emitted.contains(c))
                .collect();
            child_node.set_children(kept);
            if !claimed.contains(&child_id) {
                children.push(child_id);
            }
            self.nodes.push((child_id, child_node));
        }
        for real_id in real_ids {
            if !claimed.contains(&real_id) {
                children.push(real_id);
            }
        }
        if !children.is_empty() {
            node.set_children(children);
        }

        self.nodes.push((node_id, node));
        Some(node_id)
    }
}
