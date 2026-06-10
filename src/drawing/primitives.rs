use speedy2d::color::Color;

/// Expression system for dynamic values (like CSS calc())
#[derive(Debug, Clone)]
pub enum Expr {
    Literal(f32),
    Percent(f32),                    // 50% of bounds
    BoundsWidth,                     // rect.width
    BoundsHeight,                    // rect.height
    BoundsLeft,                      // rect.min.x
    BoundsTop,                       // rect.min.y
    BoundsRight,                     // rect.max.x
    BoundsBottom,                    // rect.max.y
    Scale,                           // DPI scale
    Add(Box<Expr>, Box<Expr>),       // expr + expr
    Sub(Box<Expr>, Box<Expr>),       // expr - expr
    Mul(Box<Expr>, Box<Expr>),       // expr * expr
    Div(Box<Expr>, Box<Expr>),       // expr / expr
}

/// Drawing commands - minimal set like SVG
#[derive(Debug, Clone)]
pub enum DrawCommand {
    /// Rectangle
    Rect {
        x: Expr,
        y: Expr,
        width: Expr,
        height: Expr,
        fill: Option<Paint>,
        stroke: Option<Stroke>,
    },

    /// Rounded rectangle
    RoundRect {
        x: Expr,
        y: Expr,
        width: Expr,
        height: Expr,
        radius: Expr,
        fill: Option<Paint>,
        stroke: Option<Stroke>,
    },

    /// Line
    Line {
        x1: Expr,
        y1: Expr,
        x2: Expr,
        y2: Expr,
        stroke: Option<Stroke>,
    },

    /// Circle
    Circle {
        cx: Expr,
        cy: Expr,
        radius: Expr,
        fill: Option<Paint>,
        stroke: Option<Stroke>,
    },

    /// SVG-style path
    Path {
        commands: Vec<PathCommand>,
        fill: Option<Paint>,
        stroke: Option<Stroke>,
    },

    /// Group of commands
    Group {
        commands: Vec<DrawCommand>,
    },
}

/// SVG path commands
#[derive(Debug, Clone)]
pub enum PathCommand {
    MoveTo { x: Expr, y: Expr },
    LineTo { x: Expr, y: Expr },
    CurveTo { x1: Expr, y1: Expr, x2: Expr, y2: Expr, x: Expr, y: Expr },
    QuadTo { x1: Expr, y1: Expr, x: Expr, y: Expr },
    Close,
}

/// Paint (fill or stroke)
#[derive(Debug, Clone)]
pub struct Paint {
    pub kind: PaintKind,
    pub opacity: f32,
}

#[derive(Debug, Clone)]
pub enum PaintKind {
    Color(Color),
    /// Named palette color (`color="@token"` in drawable XML), resolved at draw time.
    Token(String),
    Gradient(Gradient),
    None,
}

#[derive(Debug, Clone)]
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: Vec<GradientStop>,
}

#[derive(Debug, Clone)]
pub enum GradientKind {
    Linear { x1: Expr, y1: Expr, x2: Expr, y2: Expr },
    Radial { cx: Expr, cy: Expr, r: Expr },
}

#[derive(Debug, Clone)]
pub struct GradientStop {
    pub offset: f32,  // 0.0 to 1.0
    pub color: Color,
}

/// Stroke styling
#[derive(Debug, Clone)]
pub struct Stroke {
    pub paint: Paint,
    pub width: Expr,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub dash_array: Option<Vec<f32>>,
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke {
            paint: Paint {
                kind: PaintKind::Color(Color::BLACK),
                opacity: 1.0,
            },
            width: Expr::Literal(1.0),
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            dash_array: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// A drawable is a collection of draw commands
#[derive(Debug, Clone)]
pub struct Drawable {
    pub commands: Vec<DrawCommand>,
}

impl Default for Drawable {
    fn default() -> Self {
        Drawable {
            commands: Vec::new(),
        }
    }
}

/// ViewBox for SVG-style coordinate systems
#[derive(Debug, Clone)]
pub struct ViewBox {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for ViewBox {
    fn default() -> Self {
        ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: 100.0,
            height: 100.0,
        }
    }
}
