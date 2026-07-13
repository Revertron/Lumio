//! One-shot generator for the 9-patch demo assets in `examples/assets/`.
//! Kept in-repo as living documentation of how those PNGs were produced.
//! Regenerate with: `cargo run --example gen_ninepatch_assets`
//!
//! Every patch is a rounded rectangle drawn with an anti-aliased SDF, wrapped
//! in the standard Android 1px marker border: black runs on the top/left mark
//! the stretchable middle band, runs on the bottom/right mark content padding.

use image::{Rgba, RgbaImage};

const MARKER: Rgba<u8> = Rgba([0, 0, 0, 255]);

/// Signed distance from point `(px, py)` to a rounded rect inset by `inset`
/// inside a `w × h` box, with corner radius `r`. Negative = inside.
fn rounded_rect_dist(px: f32, py: f32, w: f32, h: f32, inset: f32, r: f32) -> f32 {
    let (hx, hy) = (w / 2.0 - inset - r, h / 2.0 - inset - r);
    let dx = (px - w / 2.0).abs() - hx;
    let dy = (py - h / 2.0).abs() - hy;
    let (ox, oy) = (dx.max(0.0), dy.max(0.0));
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - r
}

fn coverage(dist: f32) -> f32 {
    (0.5 - dist).clamp(0.0, 1.0)
}

fn lerp(a: Rgba<u8>, b: Rgba<u8>, t: f32) -> Rgba<u8> {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgba([mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2]), mix(a[3], b[3])])
}

/// A rounded-rect 9-patch: `content`×`content` px body with a 1px `border`
/// ring, a vertical `fill_top`→`fill_bottom` gradient, corner radius `radius`,
/// stretch markers over the middle band and padding markers `pad` px in.
fn rounded_patch(
    content: u32,
    radius: f32,
    border: Rgba<u8>,
    fill_top: Rgba<u8>,
    fill_bottom: Rgba<u8>,
    pad: u32,
) -> RgbaImage {
    let c = content;
    let mut img = RgbaImage::new(c + 2, c + 2);
    for y in 0..c {
        let fill = lerp(fill_top, fill_bottom, y as f32 / (c - 1) as f32);
        for x in 0..c {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let outer = coverage(rounded_rect_dist(px, py, c as f32, c as f32, 0.0, radius));
            let inner =
                coverage(rounded_rect_dist(px, py, c as f32, c as f32, 1.2, (radius - 1.2).max(0.0)));
            if outer <= 0.0 {
                continue;
            }
            let blend = |b: u8, f: u8| (b as f32 * (1.0 - inner) + f as f32 * inner).round() as u8;
            let a = (outer * (border[3] as f32 * (1.0 - inner) + fill[3] as f32 * inner))
                .round()
                .min(255.0) as u8;
            img.put_pixel(
                x + 1,
                y + 1,
                Rgba([blend(border[0], fill[0]), blend(border[1], fill[1]), blend(border[2], fill[2]), a]),
            );
        }
    }
    // Stretch markers: the middle band, clear of the rounded corners.
    let inset = radius.ceil() as u32 + 1;
    for i in inset..c - inset {
        img.put_pixel(i + 1, 0, MARKER); // top row: stretchable columns
        img.put_pixel(0, i + 1, MARKER); // left column: stretchable rows
    }
    // Padding markers: content area starts `pad` px from each edge.
    for i in pad..c - pad {
        img.put_pixel(i + 1, c + 1, MARKER); // bottom row: horizontal padding
        img.put_pixel(c + 1, i + 1, MARKER); // right column: vertical padding
    }
    img
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/assets");
    let save = |name: &str, img: RgbaImage| {
        let path = dir.join(name);
        img.save(&path).expect("save PNG");
        println!("wrote {}", path.display());
    };

    // A light panel with a soft gray border.
    save(
        "panel.9.png",
        rounded_patch(
            26,
            6.0,
            Rgba([150, 155, 162, 255]),
            Rgba([250, 250, 252, 255]),
            Rgba([233, 236, 241, 255]),
            8,
        ),
    );
    // Blue button: normal / hovered (lighter) / pressed (darker, flipped sheen).
    save(
        "button.9.png",
        rounded_patch(
            22,
            5.0,
            Rgba([42, 90, 140, 255]),
            Rgba([96, 156, 222, 255]),
            Rgba([53, 114, 176, 255]),
            6,
        ),
    );
    save(
        "button_hover.9.png",
        rounded_patch(
            22,
            5.0,
            Rgba([52, 105, 160, 255]),
            Rgba([120, 176, 236, 255]),
            Rgba([72, 134, 198, 255]),
            6,
        ),
    );
    save(
        "button_pressed.9.png",
        rounded_patch(
            22,
            5.0,
            Rgba([30, 70, 112, 255]),
            Rgba([46, 100, 158, 255]),
            Rgba([70, 128, 190, 255]),
            6,
        ),
    );
}
