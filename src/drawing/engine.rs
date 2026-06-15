use speedy2d::Graphics2D;
use speedy2d::color::Color;
use speedy2d::dimen::Vector2;
use speedy2d::shape::Rectangle;

use super::palette::Palette;
use super::primitives::*;
use crate::types::Rect;

/// Context for expression evaluation
pub struct DrawContext {
    pub scale: f64,
}

/// Drawing engine that renders drawables to speedy2d Graphics2D
pub struct DrawingEngine<'a> {
    graphics: &'a mut Graphics2D,
    scale: f64,
    palette: &'a Palette,
}

impl<'a> DrawingEngine<'a> {
    pub fn new(graphics: &'a mut Graphics2D, scale: f64, palette: &'a Palette) -> Self {
        DrawingEngine { graphics, scale, palette }
    }

    /// Draw a drawable within the given bounds
    pub fn draw_drawable(&mut self, drawable: &Drawable, bounds: Rect<i32>) {
        for cmd in &drawable.commands {
            self.draw_command(cmd, bounds);
        }
    }

    /// Draw a single draw command
    fn draw_command(&mut self, cmd: &DrawCommand, bounds: Rect<i32>) {
        match cmd {
            DrawCommand::Rect { x, y, width, height, fill, stroke } => {
                let x = self.eval_expr(x, bounds, Axis::X);
                let y = self.eval_expr(y, bounds, Axis::Y);
                let w = self.eval_expr(width, bounds, Axis::X);
                let h = self.eval_expr(height, bounds, Axis::Y);

                let rect = Rectangle::new(
                    Vector2::new(x, y),
                    Vector2::new(x + w, y + h)
                );

                // Draw fill
                if let Some(paint) = fill {
                    if let Some(color) = self.eval_paint(paint) {
                        self.graphics.draw_rectangle(rect.clone(), color);
                    }
                }

                // Draw stroke
                if let Some(stroke_style) = stroke {
                    self.draw_rect_stroke(rect, stroke_style);
                }
            }

            DrawCommand::Line { x1, y1, x2, y2, stroke } => {
                // Evaluate positions to integer pixels
                let mut x1 = self.eval_expr(x1, bounds, Axis::X).round();
                let mut y1 = self.eval_expr(y1, bounds, Axis::Y).round();
                let mut x2 = self.eval_expr(x2, bounds, Axis::X).round();
                let mut y2 = self.eval_expr(y2, bounds, Axis::Y).round();

                if let Some(stroke_style) = stroke {
                    // Round width to integer for crisp rendering (already scaled)
                    let width = self.eval_expr(&stroke_style.width, bounds, Axis::X).round();

                    // Add a half-pixel offset so a 1px line lands crisply on one
                    // pixel row instead of straddling two. Solid lines always shift
                    // the same way (+half). Dashed lines (e.g. a focus rectangle)
                    // shift toward the interior of the bounds — top/left edges down/
                    // right, bottom/right edges up/left — so the four edges stay
                    // symmetric and pixel-aligned instead of biasing toward bottom-right.
                    let half_pixel = (self.scale / 2.0) as f32;
                    let dashed = stroke_style.dash_array.is_some();

                    // Determine if line is horizontal or vertical and add centering offset
                    if (y1 - y2).abs() < 0.01 {
                        // Horizontal line - center on Y axis
                        let center = (bounds.min.y + bounds.max.y) as f32 / 2.0;
                        let off = if dashed && y1 > center { -half_pixel } else { half_pixel };
                        y1 += off;
                        y2 += off;
                    } else if (x1 - x2).abs() < 0.01 {
                        // Vertical line - center on X axis
                        let center = (bounds.min.x + bounds.max.x) as f32 / 2.0;
                        let off = if dashed && x1 > center { -half_pixel } else { half_pixel };
                        x1 += off;
                        x2 += off;
                    }

                    if let Some(color) = self.eval_paint(&stroke_style.paint) {
                        if let Some(dash_array) = &stroke_style.dash_array {
                            // Dashed line
                            self.draw_dashed_line(x1, y1, x2, y2, width, color, dash_array);
                        } else {
                            // Solid line
                            self.graphics.draw_line((x1, y1), (x2, y2), width, color);
                        }
                    }
                }
            }

            DrawCommand::Circle { cx, cy, radius, fill, stroke } => {
                let cx = self.eval_expr(cx, bounds, Axis::X);
                let cy = self.eval_expr(cy, bounds, Axis::Y);
                let r = self.eval_expr(radius, bounds, Axis::X);

                // Draw fill
                if let Some(paint) = fill {
                    if let Some(color) = self.eval_paint(paint) {
                        self.graphics.draw_circle((cx, cy), r, color);
                    }
                }

                // Outline as 32 line segments (speedy2d has no circle stroke)
                if let Some(stroke_style) = stroke {
                    let width = self.eval_expr(&stroke_style.width, bounds, Axis::X);
                    if let Some(color) = self.eval_paint(&stroke_style.paint) {
                        let segments = 32;
                        for i in 0..segments {
                            let angle1 = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
                            let angle2 = 2.0 * std::f32::consts::PI * (i + 1) as f32 / segments as f32;
                            let p1 = (cx + r * angle1.cos(), cy + r * angle1.sin());
                            let p2 = (cx + r * angle2.cos(), cy + r * angle2.sin());
                            self.graphics.draw_line(p1, p2, width, color);
                        }
                    }
                }
            }

            DrawCommand::Triangle { p1, p2, p3, fill } => {
                if let Some(paint) = fill {
                    if let Some(color) = self.eval_paint(paint) {
                        let points = [
                            Vector2::new(self.eval_expr(&p1.0, bounds, Axis::X), self.eval_expr(&p1.1, bounds, Axis::Y)),
                            Vector2::new(self.eval_expr(&p2.0, bounds, Axis::X), self.eval_expr(&p2.1, bounds, Axis::Y)),
                            Vector2::new(self.eval_expr(&p3.0, bounds, Axis::X), self.eval_expr(&p3.1, bounds, Axis::Y)),
                        ];
                        self.graphics.draw_triangle_three_color(points, [color, color, color]);
                    }
                }
            }

            DrawCommand::Path { commands: _, fill: _, stroke: _ } => {
                // TODO: Implement path rendering
                // This would require converting path commands to speedy2d primitives
            }

            DrawCommand::Group { commands } => {
                for sub_cmd in commands {
                    self.draw_command(sub_cmd, bounds);
                }
            }

            _ => {
                // Other commands not yet implemented
            }
        }
    }

    /// Evaluate an expression to a concrete float value. Forwards to the shared
    /// [`primitives::eval_expr`](super::primitives::eval_expr); see it for the
    /// `axis`/`scale` semantics.
    fn eval_expr(&self, expr: &Expr, bounds: Rect<i32>, axis: Axis) -> f32 {
        super::primitives::eval_expr(expr, bounds, axis, self.scale)
    }

    /// Evaluate paint to concrete color
    fn eval_paint(&self, paint: &Paint) -> Option<Color> {
        match &paint.kind {
            // TODO: fold paint.opacity into the alpha channel.
            PaintKind::Color(argb) => Some(Color::from_hex_argb(*argb)),
            PaintKind::Token(name) => Some(Color::from_hex_argb(self.palette.color(name))),
            PaintKind::Gradient(_) => {
                // TODO: Gradient support
                None
            }
            PaintKind::None => None,
        }
    }

    /// Draw rectangle stroke (outline)
    fn draw_rect_stroke(&mut self, rect: Rectangle, stroke: &Stroke) {
        let width = self.eval_expr(&stroke.width, Rect {
            min: crate::types::Point { x: rect.top_left().x as i32, y: rect.top_left().y as i32 },
            max: crate::types::Point { x: rect.bottom_right().x as i32, y: rect.bottom_right().y as i32 },
        }, Axis::X);

        if let Some(color) = self.eval_paint(&stroke.paint) {
            let tl = rect.top_left();
            let br = rect.bottom_right();
            let half = width / 2.0;

            // Top
            self.graphics.draw_line((tl.x, tl.y + half), (br.x, tl.y + half), width, color);
            // Bottom
            self.graphics.draw_line((tl.x, br.y - half), (br.x, br.y - half), width, color);
            // Left
            self.graphics.draw_line((tl.x + half, tl.y), (tl.x + half, br.y), width, color);
            // Right
            self.graphics.draw_line((br.x - half, tl.y), (br.x - half, br.y), width, color);
        }
    }

    /// Draw a dashed line with whole-pixel dashes distributed so a dash sits on
    /// BOTH ends of the line. Dashes keep a fixed integer length and land on the
    /// pixel grid, so they don't shimmer from anti-aliasing and look identical on
    /// horizontal and vertical edges; the leftover length is spread across the
    /// gaps (each at most one pixel larger than the rest). When four of these
    /// form a rectangle, every corner gets a matching dash and stays symmetric
    /// regardless of the line length — unlike walking a fixed dash and clamping
    /// the remainder into a variable stub at the far end.
    fn draw_dashed_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color, dash_array: &[f32]) {
        if dash_array.is_empty() {
            self.graphics.draw_line((x1, y1), (x2, y2), width, color);
            return;
        }

        let dx = x2 - x1;
        let dy = y2 - y1;
        let length = (dx * dx + dy * dy).sqrt();

        if length <= 0.0 {
            return;
        }

        let ux = dx / length; // Unit vector x
        let uy = dy / length; // Unit vector y

        // Whole-pixel nominal dash / gap (SVG-style: first entry = dash, second = gap).
        let dash = (dash_array[0].round() as i32).max(1);
        let nominal_gap = if dash_array.len() > 1 {
            (dash_array[1].round() as i32).max(1)
        } else {
            dash
        };
        let total = length.round() as i32;
        let cycle = dash + nominal_gap;

        // Choose a dash count that puts a dash on both ends: k dashes, k-1 gaps,
        // gaps as close to nominal as possible. Shrink k until the gaps are at
        // least 1px; lines too short to dash are drawn solid.
        let mut k = (((total + nominal_gap) as f32) / cycle as f32).round() as i32;
        if k < 1 {
            k = 1;
        }
        while k > 1 && total - k * dash < k - 1 {
            k -= 1;
        }
        if k <= 1 || total <= dash {
            self.graphics.draw_line((x1, y1), (x2, y2), width, color);
            return;
        }

        // Spread the leftover length across the k-1 gaps; the first `extra` gaps
        // are one pixel larger. Dashes themselves stay a fixed integer length.
        let total_gap = total - k * dash;
        let base_gap = total_gap / (k - 1);
        let extra = total_gap % (k - 1);

        let mut pos = 0i32;
        for i in 0..k {
            let start = pos as f32;
            let end = (pos + dash) as f32;
            self.graphics.draw_line(
                (x1 + ux * start, y1 + uy * start),
                (x1 + ux * end, y1 + uy * end),
                width,
                color,
            );
            pos += dash;
            if i < k - 1 {
                pos += base_gap + if i < extra { 1 } else { 0 };
            }
        }
    }
}
