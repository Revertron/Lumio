//! Software drawable renderer over `tiny_skia::Pixmap`. Walks the same
//! `DrawCommand` tree as the speedy `DrawingEngine`, but uses tiny-skia's native
//! fill/stroke/dash (so no 32-segment circles, 4-line rect strokes, or manual
//! dash distribution). Expression evaluation is shared with the GL engine via
//! `super::primitives::eval_expr`.

use tiny_skia::{Color as TsColor, FillRule, Mask, Paint as TsPaint, Path, PathBuilder, Pixmap, Rect as TsRect, Stroke as TsStroke, StrokeDash, Transform};

use super::palette::Palette;
use super::primitives::*;
use crate::types::Rect;

pub struct SoftwareDrawingEngine<'a> {
    pixmap: &'a mut Pixmap,
    scale: f64,
    palette: &'a Palette,
    clip: Option<&'a Mask>,
}

impl<'a> SoftwareDrawingEngine<'a> {
    pub fn new(pixmap: &'a mut Pixmap, scale: f64, palette: &'a Palette, clip: Option<&'a Mask>) -> Self {
        SoftwareDrawingEngine { pixmap, scale, palette, clip }
    }

    pub fn draw_drawable(&mut self, drawable: &Drawable, bounds: Rect<i32>) {
        for cmd in &drawable.commands {
            self.draw_command(cmd, bounds);
        }
    }

    fn draw_command(&mut self, cmd: &DrawCommand, bounds: Rect<i32>) {
        match cmd {
            DrawCommand::Rect { x, y, width, height, fill, stroke } => {
                let x = self.eval(x, bounds, Axis::X);
                let y = self.eval(y, bounds, Axis::Y);
                let w = self.eval(width, bounds, Axis::X);
                let h = self.eval(height, bounds, Axis::Y);
                if let Some(rect) = TsRect::from_xywh(x, y, w.max(0.0), h.max(0.0)) {
                    if let Some(paint) = fill.as_ref().and_then(|p| self.make_paint(p)) {
                        self.pixmap.as_mut().fill_rect(rect, &paint, Transform::identity(), self.clip);
                    }
                    if let Some(s) = stroke {
                        let mut pb = PathBuilder::new();
                        pb.push_rect(rect);
                        if let Some(path) = pb.finish() {
                            self.stroke(&path, s, bounds);
                        }
                    }
                }
            }
            DrawCommand::RoundRect { x, y, width, height, radius, fill, stroke } => {
                let x = self.eval(x, bounds, Axis::X);
                let y = self.eval(y, bounds, Axis::Y);
                let w = self.eval(width, bounds, Axis::X);
                let h = self.eval(height, bounds, Axis::Y);
                let r = self.eval(radius, bounds, Axis::X);
                if let Some(path) = rounded_rect_path(x, y, w, h, r) {
                    if let Some(paint) = fill.as_ref().and_then(|p| self.make_paint(p)) {
                        self.pixmap.as_mut().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), self.clip);
                    }
                    if let Some(s) = stroke {
                        self.stroke(&path, s, bounds);
                    }
                }
            }
            DrawCommand::Line { x1, y1, x2, y2, stroke } => {
                if let Some(s) = stroke {
                    let mut pb = PathBuilder::new();
                    pb.move_to(self.eval(x1, bounds, Axis::X), self.eval(y1, bounds, Axis::Y));
                    pb.line_to(self.eval(x2, bounds, Axis::X), self.eval(y2, bounds, Axis::Y));
                    if let Some(path) = pb.finish() {
                        self.stroke(&path, s, bounds);
                    }
                }
            }
            DrawCommand::Circle { cx, cy, radius, fill, stroke } => {
                let cx = self.eval(cx, bounds, Axis::X);
                let cy = self.eval(cy, bounds, Axis::Y);
                let r = self.eval(radius, bounds, Axis::X);
                let mut pb = PathBuilder::new();
                pb.push_circle(cx, cy, r.max(0.01));
                if let Some(path) = pb.finish() {
                    if let Some(paint) = fill.as_ref().and_then(|p| self.make_paint(p)) {
                        self.pixmap.as_mut().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), self.clip);
                    }
                    if let Some(s) = stroke {
                        self.stroke(&path, s, bounds);
                    }
                }
            }
            DrawCommand::Triangle { p1, p2, p3, fill } => {
                if let Some(paint) = fill.as_ref().and_then(|p| self.make_paint(p)) {
                    let mut pb = PathBuilder::new();
                    pb.move_to(self.eval(&p1.0, bounds, Axis::X), self.eval(&p1.1, bounds, Axis::Y));
                    pb.line_to(self.eval(&p2.0, bounds, Axis::X), self.eval(&p2.1, bounds, Axis::Y));
                    pb.line_to(self.eval(&p3.0, bounds, Axis::X), self.eval(&p3.1, bounds, Axis::Y));
                    pb.close();
                    if let Some(path) = pb.finish() {
                        self.pixmap.as_mut().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), self.clip);
                    }
                }
            }
            DrawCommand::Path { commands, fill, stroke } => {
                let mut pb = PathBuilder::new();
                for pc in commands {
                    match pc {
                        PathCommand::MoveTo { x, y } => pb.move_to(self.eval(x, bounds, Axis::X), self.eval(y, bounds, Axis::Y)),
                        PathCommand::LineTo { x, y } => pb.line_to(self.eval(x, bounds, Axis::X), self.eval(y, bounds, Axis::Y)),
                        PathCommand::QuadTo { x1, y1, x, y } => pb.quad_to(self.eval(x1, bounds, Axis::X), self.eval(y1, bounds, Axis::Y), self.eval(x, bounds, Axis::X), self.eval(y, bounds, Axis::Y)),
                        PathCommand::CurveTo { x1, y1, x2, y2, x, y } => pb.cubic_to(self.eval(x1, bounds, Axis::X), self.eval(y1, bounds, Axis::Y), self.eval(x2, bounds, Axis::X), self.eval(y2, bounds, Axis::Y), self.eval(x, bounds, Axis::X), self.eval(y, bounds, Axis::Y)),
                        PathCommand::Close => pb.close(),
                    }
                }
                if let Some(path) = pb.finish() {
                    if let Some(paint) = fill.as_ref().and_then(|p| self.make_paint(p)) {
                        self.pixmap.as_mut().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), self.clip);
                    }
                    if let Some(s) = stroke {
                        self.stroke(&path, s, bounds);
                    }
                }
            }
            DrawCommand::Group { commands } => {
                for c in commands {
                    self.draw_command(c, bounds);
                }
            }
        }
    }

    fn stroke(&mut self, path: &Path, stroke: &Stroke, bounds: Rect<i32>) {
        let Some(paint) = self.make_paint(&stroke.paint) else {
            return;
        };
        let mut s = TsStroke { width: self.eval(&stroke.width, bounds, Axis::X).max(0.0), ..TsStroke::default() };
        if let Some(arr) = &stroke.dash_array {
            // dash lengths are in dips; scale to physical pixels.
            let scaled: Vec<f32> = arr.iter().map(|d| d * self.scale as f32).collect();
            s.dash = StrokeDash::new(scaled, 0.0);
        }
        self.pixmap.as_mut().stroke_path(path, &paint, &s, Transform::identity(), self.clip);
    }

    fn make_paint(&self, paint: &Paint) -> Option<TsPaint<'static>> {
        let color = match &paint.kind {
            PaintKind::Color(argb) => argb_to_color(*argb, paint.opacity)?,
            PaintKind::Token(name) => argb_to_color(self.palette.color(name), paint.opacity)?,
            PaintKind::Gradient(_) | PaintKind::None => return None,
        };
        let mut p = TsPaint::default();
        p.set_color(color);
        p.anti_alias = true;
        Some(p)
    }

    /// Forwards to the shared [`primitives::eval_expr`](super::primitives::eval_expr);
    /// see it for the `axis`/`scale` semantics.
    fn eval(&self, expr: &Expr, bounds: Rect<i32>, axis: Axis) -> f32 {
        super::primitives::eval_expr(expr, bounds, axis, self.scale)
    }
}

/// Convert a `0xAARRGGBB` palette color to a tiny-skia color, folding in opacity.
pub(crate) fn argb_to_color(argb: u32, opacity: f32) -> Option<TsColor> {
    let a = ((argb >> 24) & 0xff) as f32 / 255.0 * opacity;
    let r = ((argb >> 16) & 0xff) as f32 / 255.0;
    let g = ((argb >> 8) & 0xff) as f32 / 255.0;
    let b = (argb & 0xff) as f32 / 255.0;
    TsColor::from_rgba(r, g, b, a)
}

/// A rounded-rectangle path (corners approximated with quadratics). `r` is
/// clamped to half the smaller side.
fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (x0, y0, x1, y1) = (x, y, x + w, y + h);
    let mut pb = PathBuilder::new();
    if r <= 0.0 {
        pb.push_rect(TsRect::from_ltrb(x0, y0, x1, y1)?);
        return pb.finish();
    }
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.quad_to(x1, y0, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.quad_to(x1, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.quad_to(x0, y1, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.quad_to(x0, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}
