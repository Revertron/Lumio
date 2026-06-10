use downcast_rs::Downcast;

use super::themes::Typeface;
use super::traits::Element;
use super::types::rect;
use super::views::{Borders, Dimension, Direction, Gravity, HAlign, VAlign, Visibility};

/// A layout strategy for containers: positions children inside a parent's
/// content area. `Frame` delegates all child placement to its `Layout`,
/// keeping container concerns (events, focus, painting) separate from
/// geometry policy.
///
/// The contract follows Lumio's single-pass top-down model: `arrange` must
/// call `layout_content` on every visible child (and/or `set_rect` to move
/// it), giving each child its position relative to the parent's top-left
/// corner. After `arrange` returns, the parent derives its own `Min` size
/// from the bounding box of the children's rects.
pub trait Layout: Downcast {
    /// Positions `children` inside the parent's content area.
    ///
    /// * `bounds` — the parent's configured `Dimension`s (width, height);
    ///   lets a layout react to content-hugging (`Min`) parents.
    /// * `width` / `height` — the parent's resolved size in physical pixels.
    /// * `padding` — the parent's padding, already scaled.
    #[allow(clippy::too_many_arguments)]
    fn arrange(&self, children: &[Element], bounds: (Dimension, Dimension), width: i32, height: i32, padding: &Borders, typeface: &Typeface, scale: f64);

    /// The main axis of this layout, used by containers for arrow-key focus
    /// navigation (Left/Right in horizontal layouts, Up/Down in vertical).
    fn direction(&self) -> Direction { Direction::default() }
}

impl_downcast!(Layout);

/// The default layout: children flow one after another along `direction`.
/// With `breaking` enabled (horizontal only), children wrap to the next row
/// when they run out of width.
#[derive(Default)]
pub struct LinearLayout {
    pub direction: Direction,
    pub breaking: bool
}

impl Layout for LinearLayout {
    fn arrange(&self, children: &[Element], bounds: (Dimension, Dimension), width: i32, height: i32, padding: &Borders, typeface: &Typeface, scale: f64) {
        if self.breaking && self.direction == Direction::Horizontal {
            self.layout_single_pass(children, width, height, padding, typeface, scale);
        } else {
            self.layout_two_pass(children, bounds, width, height, padding, typeface, scale);
        }
    }

    fn direction(&self) -> Direction {
        self.direction
    }
}

/// Returns how far to shift a child along its parent's cross axis based on gravity.
/// In a vertical layout the cross axis is horizontal; in horizontal the cross axis is vertical.
#[allow(clippy::too_many_arguments)]
fn cross_axis_offset(
    gravity: Gravity,
    is_vertical: bool,
    parent_width: i32, parent_height: i32,
    parent_padding: &Borders, child_margin: &Borders,
    child_width: i32, child_height: i32
) -> i32 {
    if is_vertical {
        let band = (parent_width - parent_padding.left - parent_padding.right
            - child_margin.left - child_margin.right - child_width).max(0);
        match gravity.horizontal() {
            HAlign::Left => 0,
            HAlign::Center => band / 2,
            HAlign::Right => band,
        }
    } else {
        let band = (parent_height - parent_padding.top - parent_padding.bottom
            - child_margin.top - child_margin.bottom - child_height).max(0);
        match gravity.vertical() {
            VAlign::Top => 0,
            VAlign::Center => band / 2,
            VAlign::Bottom => band,
        }
    }
}

impl LinearLayout {
    /// Single-pass layout for breaking horizontal layouts (original algorithm).
    fn layout_single_pass(&self, children: &[Element], new_width: i32, new_height: i32, padding: &Borders, typeface: &Typeface, scale: f64) {
        let mut xx = padding.left;
        let mut yy = padding.top;
        let max_x = new_width - padding.right;
        let mut max_height = 0;
        for v in children.iter() {
            let mut v = v.try_borrow_mut().unwrap();
            if v.get_visibility() == Visibility::Gone {
                continue;
            }
            let margins = v.get_margin(scale);
            // Use the rect set by layout_content — it honors configured Dimensions
            // (Dip/Percent), unlike calculate_full_size which re-derives the size
            // from raw content and undersizes fixed-size children (same fix as in
            // layout_two_pass pass 1).
            let r = v.layout_content(xx + margins.left, yy + margins.top, new_width - xx - padding.right, new_height - yy - padding.bottom, typeface, scale);
            let (w, h) = (r.width(), r.height());
            match self.direction {
                Direction::Horizontal => xx = xx + w + margins.left + margins.right,
                Direction::Vertical => yy = yy + h + margins.top + margins.bottom
            }
            if xx > max_x {
                yy += max_height + margins.top;
                xx = padding.left + margins.left;
                let r = v.layout_content(xx, yy + margins.top, new_width - xx - padding.right, new_height - yy - padding.bottom, typeface, scale);
                let (w, h) = (r.width(), r.height());
                xx += w;
                max_height = h + margins.bottom;
            }
            if v.is_break() {
                let h = v.get_rect().height();
                xx = padding.left;
                yy += h + margins.bottom;
            }
            if h > max_height {
                max_height = h;
            }
        }
    }

    /// Two-pass layout: measures non-Max children first, then distributes remaining space to Max children.
    #[allow(clippy::too_many_arguments)]
    fn layout_two_pass(&self, children: &[Element], bounds: (Dimension, Dimension), new_width: i32, new_height: i32, padding: &Borders, typeface: &Typeface, scale: f64) {
        let is_vertical = self.direction == Direction::Vertical;

        let total_available = if is_vertical {
            new_height - padding.top - padding.bottom
        } else {
            new_width - padding.left - padding.right
        };

        // Pass 1: Measure non-Max children, count Max children
        let mut fixed_consumed: i32 = 0;
        let mut max_count: i32 = 0;
        let mut child_is_max: Vec<bool> = Vec::with_capacity(children.len());

        for v in children.iter() {
            let mut v = v.try_borrow_mut().unwrap();
            if v.get_visibility() == Visibility::Gone {
                child_is_max.push(false);
                continue;
            }
            let margins = v.get_margin(scale);
            let bounds = v.get_bounds();

            let is_max = if is_vertical {
                matches!(bounds.1, Dimension::Max)
            } else {
                matches!(bounds.0, Dimension::Max)
            };

            let (margin_before, margin_after) = if is_vertical {
                (margins.top, margins.bottom)
            } else {
                (margins.left, margins.right)
            };

            if is_max {
                max_count += 1;
                // Reserve space for margins only; the child's content space is computed later
                fixed_consumed += margin_before + margin_after;
            } else {
                // Layout at temporary position to measure size. Subtract the
                // child's own margins from the available area so wrapping
                // content (e.g. Labels) sizes itself within its content box,
                // not into the margin space — otherwise a long wrapped Label
                // can eat its own margin_right.
                v.layout_content(
                    padding.left + margins.left,
                    padding.top + margins.top,
                    new_width - padding.left - padding.right - margins.left - margins.right,
                    new_height - padding.top - padding.bottom - margins.top - margins.bottom,
                    typeface, scale
                );
                // Use the rect just set by layout_content — it honors configured
                // Dimensions (Dip/Percent), unlike calculate_full_size which
                // re-derives from raw content. Pass 2 advances cursor using
                // child_rect.height() too, so this keeps both passes consistent.
                let measured = v.get_rect();
                let size = if is_vertical { measured.height() } else { measured.width() };
                fixed_consumed += size + margin_before + margin_after;
            }
            child_is_max.push(is_max);
        }

        // Compute space for Max children (per_max excludes Max children's margins)
        let remaining = (total_available - fixed_consumed).max(0);
        let per_max = if max_count > 0 { remaining / max_count } else { 0 };
        let mut extra = if max_count > 0 { remaining % max_count } else { 0 };

        // When the parent shrinks to its content on the cross axis (Min), gravity
        // should align children inside the resolved content width — not the full
        // available width — otherwise a right-gravity child would expand the
        // parent to the available edge instead of sitting flush against the
        // longest sibling.
        let cross_is_min = if is_vertical {
            matches!(bounds.0, Dimension::Min)
        } else {
            matches!(bounds.1, Dimension::Min)
        };
        let (effective_pw, effective_ph) = if cross_is_min {
            let mut max_extent = 0i32;
            for v in children.iter() {
                let v = v.try_borrow().unwrap();
                if v.get_visibility() == Visibility::Gone { continue; }
                let r = v.get_rect();
                let m = v.get_margin(scale);
                let extent = if is_vertical {
                    r.width() + m.left + m.right
                } else {
                    r.height() + m.top + m.bottom
                };
                if extent > max_extent { max_extent = extent; }
            }
            if is_vertical {
                let resolved = (padding.left + max_extent + padding.right).min(new_width);
                (resolved, new_height)
            } else {
                let resolved = (padding.top + max_extent + padding.bottom).min(new_height);
                (new_width, resolved)
            }
        } else {
            (new_width, new_height)
        };

        // When there are no Max children, leftover main-axis space goes before the
        // first child whose main-axis gravity points to the end (right in horizontal,
        // bottom in vertical), pushing it and following siblings against the end edge.
        let main_end_gap_at = if max_count == 0 && remaining > 0 {
            children.iter().enumerate().find_map(|(i, v)| {
                let vb = v.try_borrow().unwrap();
                if vb.get_visibility() == Visibility::Gone { return None; }
                let g = vb.get_gravity();
                let at_end = if is_vertical {
                    g.vertical() == VAlign::Bottom
                } else {
                    g.horizontal() == HAlign::Right
                };
                if at_end { Some(i) } else { None }
            })
        } else {
            None
        };

        // Pass 2: Layout Max children at final positions, move non-Max children
        let mut cursor = if is_vertical { padding.top } else { padding.left };

        for (i, v) in children.iter().enumerate() {
            let mut v = v.try_borrow_mut().unwrap();
            if v.get_visibility() == Visibility::Gone {
                continue;
            }
            if main_end_gap_at == Some(i) {
                cursor += remaining;
            }
            let margins = v.get_margin(scale);
            let is_max = child_is_max[i];

            let (margin_before, margin_after) = if is_vertical {
                (margins.top, margins.bottom)
            } else {
                (margins.left, margins.right)
            };

            if is_max {
                // per_max is the content space (margins already reserved in fixed_consumed).
                // layout_content's width/height param is "available space" — calculate_size
                // for Max subtracts margins internally, so pass per_max + margins.
                let mut slot = per_max;
                if extra > 0 {
                    slot += 1;
                    extra -= 1;
                }
                let avail = slot + margin_before + margin_after;

                if is_vertical {
                    v.layout_content(
                        padding.left + margins.left,
                        cursor + margins.top,
                        new_width - padding.left - padding.right,
                        avail,
                        typeface, scale
                    );
                } else {
                    v.layout_content(
                        cursor + margins.left,
                        padding.top + margins.top,
                        avail,
                        new_height - padding.top - padding.bottom,
                        typeface, scale
                    );
                }
                // Apply cross-axis gravity. The child's cross-axis size may be
                // smaller than the parent's (e.g. Label height=Min inside a
                // tall horizontal Frame); without this, gravity="center_vertical"
                // / "right" / "bottom" on a Max child has no effect. Recompute
                // the absolute target from the canonical anchor (cursor/padding)
                // each pass — some views (Label) cache layout and re-return their
                // last rect on subsequent layout_content calls, so reading the
                // current rect and adding an offset would compound on every relayout.
                let child_rect_now = v.get_rect();
                let cross_offset = cross_axis_offset(
                    v.get_gravity(),
                    is_vertical,
                    effective_pw, effective_ph,
                    padding, &margins,
                    child_rect_now.width(), child_rect_now.height()
                );
                let (anchor_x, anchor_y) = if is_vertical {
                    (padding.left + margins.left + cross_offset, cursor + margins.top)
                } else {
                    (cursor + margins.left, padding.top + margins.top + cross_offset)
                };
                if child_rect_now.min.x != anchor_x || child_rect_now.min.y != anchor_y {
                    let moved = rect(
                        (anchor_x, anchor_y),
                        (anchor_x + child_rect_now.width(), anchor_y + child_rect_now.height()),
                    );
                    v.set_rect(moved);
                }
                // Advance cursor by the child's actual rect size + margins
                let child_rect = v.get_rect();
                let size = if is_vertical { child_rect.height() } else { child_rect.width() };
                cursor += size + margin_before + margin_after;
            } else {
                // Move to correct final position (don't re-call layout_content,
                // as some views like Label cache their layout and skip re-layout)
                let old_rect = v.get_rect();
                let cross_offset = cross_axis_offset(
                    v.get_gravity(),
                    is_vertical,
                    effective_pw, effective_ph,
                    padding, &margins,
                    old_rect.width(), old_rect.height()
                );
                let (new_x, new_y) = if is_vertical {
                    (padding.left + margins.left + cross_offset, cursor + margins.top)
                } else {
                    (cursor + margins.left, padding.top + margins.top + cross_offset)
                };
                if old_rect.min.x != new_x || old_rect.min.y != new_y {
                    let moved = rect(
                        (new_x, new_y),
                        (new_x + old_rect.width(), new_y + old_rect.height())
                    );
                    v.set_rect(moved);
                }
                let size = if is_vertical { old_rect.height() } else { old_rect.width() };
                cursor += size + margin_before + margin_after;
            }
        }
    }
}
