use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};

pub fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    let s = std::str::from_utf8(head).unwrap_or("");
    s.contains("<svg") || (s.starts_with("<?xml") && s.contains("<svg"))
}

pub fn rasterize(bytes: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    if w == 0 || h == 0 {
        return None;
    }
    let tree = Tree::from_data(bytes, &Options::default()).ok()?;
    let size = tree.size();
    let sx = w as f32 / size.width();
    let sy = h as f32 / size.height();
    let s = sx.min(sy);
    let dx = (w as f32 - size.width() * s) / 2.0;
    let dy = (h as f32 - size.height() * s) / 2.0;
    let transform = Transform::from_translate(dx, dy).pre_scale(s, s);

    let mut pixmap = Pixmap::new(w, h)?;
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.take())
}
