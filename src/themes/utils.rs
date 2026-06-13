use speedy2d::color::Color;
use speedy2d::Graphics2D;

#[allow(unused)]
pub fn draw_rounded_rectangle(graphics: &mut Graphics2D, x1: f32, y1: f32, x2: f32, y2: f32, radius: f32, scale: f32, color: Color) {
    let xmin = x1.min(x2);
    let xmax = x1.max(x2);
    let ymin = y1.min(y2);
    let ymax = y1.max(y2);

    // Draw top and bottom lines
    graphics.draw_line((xmin + radius - 0.5, ymin + 0.5), (xmax - radius + 0.5, ymin + 0.5), scale, color);
    graphics.draw_line((xmin + radius - 0.5, ymax - 0.5), (xmax - radius + 0.5, ymax - 0.5), scale, color);

    // Draw left and right lines
    graphics.draw_line((xmin + 0.5, ymin + radius - 0.5), (xmin + 0.5, ymax - radius + 0.5), scale, color);
    graphics.draw_line((xmax - 0.5, ymin + radius - 0.5), (xmax - 0.5, ymax - radius + 0.5), scale, color);

    // Draw quarter-circles
    draw_quarter_circle(graphics, xmin + 0.5 + radius, ymin + 0.5 + radius, radius, 0, scale, color);
    draw_quarter_circle(graphics, xmax - 0.5 - radius, ymin + 0.5 + radius, radius, 1, scale, color);
    draw_quarter_circle(graphics, xmax - 0.5 - radius, ymax - 0.5 - radius, radius, 2, scale, color);
    draw_quarter_circle(graphics, xmin + 0.5 + radius, ymax - 0.5 - radius, radius, 3, scale, color);
}

#[allow(unused)]
fn draw_quarter_circle(graphics: &mut Graphics2D, x: f32, y: f32, radius: f32, quadrant: i32, scale: f32, color: Color) {
    let mut xx = radius;
    let mut yy = 0f32;
    let mut decision = 1f32 - xx;
    let shift = 0.5 * scale;

    while yy <= xx {
        match quadrant {
            0 => {
                graphics.draw_line((-xx + x, -yy + y), (-xx + x + shift, -yy + y - shift), scale, color);
                graphics.draw_line((-yy + x, -xx + y), (-yy + x - shift, -xx + y + shift), scale, color);
            },
            1 => {
                graphics.draw_line((xx + x, -yy + y), (xx + x + shift, -yy + y - shift), scale, color);
                graphics.draw_line((yy + x, -xx + y), (yy + x - shift, -xx + y + shift), scale, color);
            },
            2 => {
                graphics.draw_line((xx + x, yy + y), (xx + x + shift, yy + y - shift), scale, color);
                graphics.draw_line((yy + x, xx + y), (yy + x - shift, xx + y + shift), scale, color);
            },
            3 => {
                graphics.draw_line((-xx + x, yy + y), (-xx + x + shift, yy + y - shift), scale, color);
                graphics.draw_line((-yy + x, xx + y), (-yy + x - shift, xx + y + shift), scale, color);
            },
            _ => panic!("Invalid quadrant"),
        }

        yy += 1f32;

        if decision <= 0f32 {
            decision += 2f32 * yy + 1f32;
        } else {
            xx -= 1f32;
            decision += 2f32 * (yy - xx) + 1f32;
        }
    }
}

#[allow(unused)]
pub fn draw_dashed_rectangle(graphics: &mut Graphics2D, x1: f32, y1: f32, x2: f32, y2: f32, dash_len: f32, scale: f32, color: Color) {
    // Each side is split into whole-pixel dashes distributed so a dash lands on
    // BOTH ends of the side: k dashes, k-1 gaps, dash ~= gap ~= dash_len. Dashes
    // keep a fixed integer length on the pixel grid (so they don't shimmer from
    // anti-aliasing and look identical on every side) and the leftover length is
    // spread across the gaps. That keeps all four corners symmetric and free of
    // leftover stubs regardless of the rectangle's size.
    let dash = (dash_len.round() as i32).max(1);
    let cycle = dash * 2; // dash_len drives both dash and gap
    let mut draw_side = |sx: f32, sy: f32, dx: f32, dy: f32, length: f32| {
        let total = length.round() as i32;
        if total <= 0 {
            return;
        }
        let mut k = (((total + dash) as f32) / cycle as f32).round() as i32;
        if k < 1 {
            k = 1;
        }
        while k > 1 && total - k * dash < k - 1 {
            k -= 1;
        }
        if k <= 1 || total <= dash {
            graphics.draw_line((sx, sy), (sx + dx * total as f32, sy + dy * total as f32), scale, color);
            return;
        }
        let total_gap = total - k * dash;
        let base_gap = total_gap / (k - 1);
        let extra = total_gap % (k - 1);
        let mut pos = 0i32;
        for i in 0..k {
            let start = pos as f32;
            let end = (pos + dash) as f32;
            graphics.draw_line(
                (sx + dx * start, sy + dy * start),
                (sx + dx * end, sy + dy * end),
                scale,
                color,
            );
            pos += dash;
            if i < k - 1 {
                pos += base_gap + if i < extra { 1 } else { 0 };
            }
        }
    };

    draw_side(x1, y1, 1.0, 0.0, x2 - x1); // top
    draw_side(x1, y2, 1.0, 0.0, x2 - x1); // bottom
    draw_side(x1, y1, 0.0, 1.0, y2 - y1); // left
    draw_side(x2, y1, 0.0, 1.0, y2 - y1); // right
}