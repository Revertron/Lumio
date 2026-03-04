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
    let mut x = x1;
    let mut y = y1;

    while x < x2 {
        let mut end_x = x + dash_len;
        if end_x > x2 {
            end_x = x2;
        }
        graphics.draw_line((x, y1), (end_x, y1), scale, color);
        x = end_x + dash_len;
    }

    x = x2;
    y = y1;

    while y < y2 {
        let mut end_y = y + dash_len;
        if end_y > y2 {
            end_y = y2;
        }
        graphics.draw_line((x2, y), (x2, end_y), scale, color);
        y = end_y + dash_len;
    }

    x = x2;
    y = y2;

    while x > x1 {
        let mut end_x = x - dash_len;
        if end_x < x1 {
            end_x = x1;
        }
        graphics.draw_line((x, y2), (end_x, y2), scale, color);
        x = end_x - dash_len;
    }

    x = x1;
    y = y2;

    while y > y1 {
        let mut end_y = y - dash_len;
        if end_y < y1 {
            end_y = y1;
        }
        graphics.draw_line((x1, y), (x1, end_y), scale, color);
        y = end_y - dash_len;
    }
}