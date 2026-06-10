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
                let x = self.eval_expr(x, bounds);
                let y = self.eval_expr(y, bounds);
                let w = self.eval_expr(width, bounds);
                let h = self.eval_expr(height, bounds);

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
                let mut x1 = self.eval_expr(x1, bounds).round();
                let mut y1 = self.eval_expr(y1, bounds).round();
                let mut x2 = self.eval_expr(x2, bounds).round();
                let mut y2 = self.eval_expr(y2, bounds).round();

                if let Some(stroke_style) = stroke {
                    // Round width to integer for crisp rendering (already scaled)
                    let width = self.eval_expr(&stroke_style.width, bounds).round();

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

            DrawCommand::Circle { cx, cy, radius, fill, stroke: _ } => {
                let cx = self.eval_expr(cx, bounds);
                let cy = self.eval_expr(cy, bounds);
                let r = self.eval_expr(radius, bounds);

                // Draw fill
                if let Some(paint) = fill {
                    if let Some(color) = self.eval_paint(paint) {
                        self.graphics.draw_circle((cx, cy), r, color);
                    }
                }

                // TODO: stroke for circle (not directly supported by speedy2d)
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

    /// Evaluate an expression to a concrete float value
    fn eval_expr(&self, expr: &Expr, bounds: Rect<i32>) -> f32 {
        match expr {
            Expr::Literal(v) => *v * self.scale as f32,
            Expr::Percent(p) => {
                // Percentage is context-dependent, but for simplicity use width
                // In real implementation, we'd need context to know if this is X or Y
                bounds.width() as f32 * p / 100.0
            }
            Expr::BoundsWidth => bounds.width() as f32,
            Expr::BoundsHeight => bounds.height() as f32,
            Expr::BoundsLeft => bounds.min.x as f32,
            Expr::BoundsTop => bounds.min.y as f32,
            Expr::BoundsRight => bounds.max.x as f32,
            Expr::BoundsBottom => bounds.max.y as f32,
            Expr::Scale => self.scale as f32,
            Expr::Add(a, b) => self.eval_expr(a, bounds) + self.eval_expr(b, bounds),
            Expr::Sub(a, b) => self.eval_expr(a, bounds) - self.eval_expr(b, bounds),
            Expr::Mul(a, b) => self.eval_expr(a, bounds) * self.eval_expr(b, bounds),
            Expr::Div(a, b) => {
                let divisor = self.eval_expr(b, bounds);
                if divisor != 0.0 {
                    self.eval_expr(a, bounds) / divisor
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
        });

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
