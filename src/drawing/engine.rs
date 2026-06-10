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

/// Which bounds dimension `Expr::Percent` refers to: X = width, Y = height.
#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
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

                    // Add half-pixel offset for pixel-perfect line centering, scaled by DPI
                    // At 100% scale: 0.5, at 200% scale: 1.0, etc.
                    // This ensures lines are drawn centered on pixel boundaries for crisp rendering
                    let half_pixel = (self.scale / 2.0) as f32;

                    // Determine if line is horizontal or vertical and add centering offset
                    if (y1 - y2).abs() < 0.01 {
                        // Horizontal line - center on Y axis
                        y1 += half_pixel;
                        y2 += half_pixel;
                    } else if (x1 - x2).abs() < 0.01 {
                        // Vertical line - center on X axis
                        x1 += half_pixel;
                        x2 += half_pixel;
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

    /// Evaluate an expression to a concrete float value. `axis` selects which
    /// bounds dimension `Percent` refers to.
    fn eval_expr(&self, expr: &Expr, bounds: Rect<i32>, axis: Axis) -> f32 {
        match expr {
            Expr::Literal(v) => *v * self.scale as f32,
            Expr::Percent(p) => {
                let dimension = match axis {
                    Axis::X => bounds.width(),
                    Axis::Y => bounds.height(),
                };
                dimension as f32 * p / 100.0
            }
            Expr::BoundsWidth => bounds.width() as f32,
            Expr::BoundsHeight => bounds.height() as f32,
            Expr::BoundsLeft => bounds.min.x as f32,
            Expr::BoundsTop => bounds.min.y as f32,
            Expr::BoundsRight => bounds.max.x as f32,
            Expr::BoundsBottom => bounds.max.y as f32,
            Expr::Scale => self.scale as f32,
            Expr::Add(a, b) => self.eval_expr(a, bounds, axis) + self.eval_expr(b, bounds, axis),
            Expr::Sub(a, b) => self.eval_expr(a, bounds, axis) - self.eval_expr(b, bounds, axis),
            Expr::Mul(a, b) => self.eval_expr(a, bounds, axis) * self.eval_expr(b, bounds, axis),
            Expr::Div(a, b) => {
                let divisor = self.eval_expr(b, bounds, axis);
                if divisor != 0.0 {
                    self.eval_expr(a, bounds, axis) / divisor
                } else {
                    0.0
                }
            }
        }
    }

    /// Evaluate paint to concrete color
    fn eval_paint(&self, paint: &Paint) -> Option<Color> {
        match &paint.kind {
            PaintKind::Color(c) => {
                if paint.opacity < 1.0 {
                    // Apply opacity
                    // TODO: Properly apply opacity to color
                    Some(*c)
                } else {
                    Some(*c)
                }
            }
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

    /// Draw a dashed line
    fn draw_dashed_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Color, dash_array: &[f32]) {
        if dash_array.is_empty() {
            self.graphics.draw_line((x1, y1), (x2, y2), width, color);
            return;
        }

        let dx = x2 - x1;
        let dy = y2 - y1;
        let length = (dx * dx + dy * dy).sqrt();

        if length == 0.0 {
            return;
        }

        let ux = dx / length; // Unit vector x
        let uy = dy / length; // Unit vector y

        let mut pos = 0.0;
        let mut dash_index = 0;
        let mut drawing = true;

        while pos < length {
            let dash_length = dash_array[dash_index % dash_array.len()];
            let next_pos = (pos + dash_length).min(length);

            if drawing {
                let start_x = x1 + ux * pos;
                let start_y = y1 + uy * pos;
                let end_x = x1 + ux * next_pos;
                let end_y = y1 + uy * next_pos;

                self.graphics.draw_line((start_x, start_y), (end_x, end_y), width, color);
            }

            pos = next_pos;
            dash_index += 1;
            drawing = !drawing;
        }
    }
}
