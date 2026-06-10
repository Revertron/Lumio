//! Background image support for containers (currently `Frame`).
//!
//! A `BackgroundImage` is drawn above the background color fill and below
//! all child views. It supports PNG/JPEG/SVG assets, opacity, tiling,
//! keyword positioning with edge offsets, and CSS-like sizing modes
//! (`auto`, `cover`, `contain`, explicit dip / percent-of-frame /
//! factor-of-natural-size components).

use std::hash::{Hash, Hasher};
use std::io::Cursor;

use image::GenericImageView;

use crate::assets::get_asset;
use crate::svg;
use crate::themes::Theme;
use crate::types::{Rect, rect};
use crate::views::{Borders, HAlign, VAlign};

/// Tiling mode for a background image.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BgRepeat {
    #[default]
    None,
    X,
    Y,
    Both,
}

/// Offset applied to a position keyword, measured inward from the named edge
/// (for `center`, positive moves right/down).
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum BgOffset {
    #[default]
    Zero,
    /// Device-independent pixels.
    Dip(f32),
    /// Percent of the origin rect's dimension on that axis.
    Percent(f32),
}

/// Anchor position of the background image inside its origin rect.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BgPosition {
    pub h: HAlign,
    pub h_offset: BgOffset,
    pub v: VAlign,
    pub v_offset: BgOffset,
}

impl Default for BgPosition {
    fn default() -> Self {
        BgPosition { h: HAlign::Center, h_offset: BgOffset::Zero, v: VAlign::Center, v_offset: BgOffset::Zero }
    }
}

/// One component (width or height) of an explicit background size.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum BgSizeComponent {
    /// Derived from the natural size (or the other component's aspect ratio).
    #[default]
    Auto,
    /// Device-independent pixels.
    Dip(f32),
    /// Percent of the origin rect's dimension.
    Percent(f32),
    /// Multiple of the image's natural size, e.g. `1.5x`.
    Factor(f32),
}

/// Sizing mode for a background image.
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub enum BgSize {
    /// Natural image size (dip-scaled).
    #[default]
    Auto,
    /// Scale preserving aspect ratio until the origin rect is fully covered.
    Cover,
    /// Scale preserving aspect ratio so the whole image fits inside.
    Contain,
    Explicit(BgSizeComponent, BgSizeComponent),
}

/// Which rect the image is positioned in and clipped to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BgOrigin {
    /// The full frame rect.
    #[default]
    Frame,
    /// The content area inside the frame's padding.
    Content,
}

/// A configured background image: asset source plus style knobs.
/// Style fields are public — tweak them directly via `Frame::background_image_mut()`.
pub struct BackgroundImage {
    path: String,
    bytes: Option<Vec<u8>>,
    load_attempted: bool,
    is_svg: bool,
    natural_size: (u32, u32),
    /// SVG rasterization cache at the last destination size (w, h, rgba).
    rasterized: Option<(u32, u32, Vec<u8>)>,
    /// 0.0–1.0, multiplied with the current theme opacity.
    pub opacity: f32,
    pub repeat: BgRepeat,
    pub position: BgPosition,
    pub size: BgSize,
    pub origin: BgOrigin,
}

impl Default for BackgroundImage {
    fn default() -> Self {
        BackgroundImage {
            path: String::new(),
            bytes: None,
            load_attempted: false,
            is_svg: false,
            natural_size: (0, 0),
            rasterized: None,
            opacity: 1.0,
            repeat: BgRepeat::None,
            position: BgPosition::default(),
            size: BgSize::Auto,
            origin: BgOrigin::Frame,
        }
    }
}

impl BackgroundImage {
    /// Changes the image source, resetting all loaded/cached data.
    pub fn set_path(&mut self, path: &str) {
        self.path = path.to_owned();
        self.bytes = None;
        self.load_attempted = false;
        self.is_svg = false;
        self.natural_size = (0, 0);
        self.rasterized = None;
    }

    fn ensure_loaded(&mut self) {
        if self.bytes.is_some() || self.load_attempted || self.path.is_empty() {
            return;
        }
        self.load_attempted = true;
        if let Some(bytes) = get_asset(&self.path) {
            let is_svg = self.path.to_ascii_lowercase().ends_with(".svg") || svg::looks_like_svg(&bytes);
            if is_svg {
                if let Ok(tree) = usvg::Tree::from_data(&bytes, &usvg::Options::default()) {
                    let s = tree.size();
                    self.natural_size = (s.width().ceil() as u32, s.height().ceil() as u32);
                } else {
                    println!("Frame background: failed to parse SVG: {}", self.path);
                }
            } else {
                match image::load(Cursor::new(&bytes), image::ImageFormat::from_path(&self.path).unwrap_or(image::ImageFormat::Png)) {
                    Ok(img) => {
                        self.natural_size = img.dimensions();
                    }
                    Err(e) => {
                        println!("Frame background: failed to decode image: {}", e);
                    }
                }
            }
            self.is_svg = is_svg;
            self.bytes = Some(bytes);
        } else {
            println!("Frame background: asset not found: {}", self.path);
        }
    }

    /// Destination size of one image tile in physical pixels.
    fn dest_size(&self, origin: Rect<i32>, scale: f64) -> (i32, i32) {
        let (nw, nh) = self.natural_size;
        if nw == 0 || nh == 0 {
            return (0, 0);
        }
        let nat_w = nw as f64 * scale;
        let nat_h = nh as f64 * scale;
        let ow = origin.width() as f64;
        let oh = origin.height() as f64;
        let (w, h) = match self.size {
            BgSize::Auto => (nat_w, nat_h),
            BgSize::Cover | BgSize::Contain => {
                let kx = ow / nat_w;
                let ky = oh / nat_h;
                let k = if self.size == BgSize::Cover { kx.max(ky) } else { kx.min(ky) };
                (nat_w * k, nat_h * k)
            }
            BgSize::Explicit(cw, ch) => {
                let w = resolve_component(cw, ow, nat_w, scale);
                let h = resolve_component(ch, oh, nat_h, scale);
                match (w, h) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => (w, w * nat_h / nat_w),
                    (None, Some(h)) => (h * nat_w / nat_h, h),
                    (None, None) => (nat_w, nat_h),
                }
            }
        };
        (w.round() as i32, h.round() as i32)
    }

    /// Top-left corner of the anchor tile, per the position keywords and offsets.
    fn anchor(&self, origin: Rect<i32>, dest: (i32, i32), scale: f64) -> (i32, i32) {
        let ho = resolve_offset(self.position.h_offset, origin.width(), scale);
        let vo = resolve_offset(self.position.v_offset, origin.height(), scale);
        let x = match self.position.h {
            HAlign::Left => origin.min.x + ho,
            HAlign::Center => origin.min.x + (origin.width() - dest.0) / 2 + ho,
            HAlign::Right => origin.max.x - dest.0 - ho,
        };
        let y = match self.position.v {
            VAlign::Top => origin.min.y + vo,
            VAlign::Center => origin.min.y + (origin.height() - dest.1) / 2 + vo,
            VAlign::Bottom => origin.max.y - dest.1 - vo,
        };
        (x, y)
    }

    /// Draws the background image. `frame_rect` is the frame's screen rect,
    /// `padding` is already scaled to physical pixels. The caller must have
    /// clipped to `frame_rect` already (partial tiles rely on the scissor).
    pub fn paint(&mut self, theme: &mut dyn Theme, frame_rect: Rect<i32>, padding: &Borders, scale: f64) {
        self.ensure_loaded();
        if self.bytes.is_none() || self.opacity <= 0.0 {
            return;
        }
        let origin = match self.origin {
            BgOrigin::Frame => frame_rect,
            BgOrigin::Content => rect(
                (frame_rect.min.x + padding.left, frame_rect.min.y + padding.top),
                (frame_rect.max.x - padding.right, frame_rect.max.y - padding.bottom),
            ),
        };
        if origin.width() <= 0 || origin.height() <= 0 {
            return;
        }
        let (dw, dh) = self.dest_size(origin, scale);
        if dw < 1 || dh < 1 {
            return;
        }
        let (ax, ay) = self.anchor(origin, (dw, dh), scale);

        // For SVG: (re)rasterize at the destination size and remember the GPU cache key.
        let mut raster_cache_key = None;
        if self.is_svg {
            let (w, h) = (dw as u32, dh as u32);
            let needs_render = match &self.rasterized {
                Some((cw, ch, _)) => *cw != w || *ch != h,
                None => true,
            };
            if needs_render
                && let Some(src) = &self.bytes
                && let Some(rgba) = svg::rasterize(src, w, h)
            {
                self.rasterized = Some((w, h, rgba));
            }
            match &self.rasterized {
                Some((cw, ch, _)) if *cw == w && *ch == h => {
                    raster_cache_key = Some(raster_key(&self.path, w, h));
                }
                _ => return,
            }
        }

        let clip_content = self.origin == BgOrigin::Content;
        if clip_content {
            theme.push_clip();
            theme.clip_rect(origin);
        }
        if self.opacity < 1.0 {
            theme.push_opacity(self.opacity);
        }

        // Anchor tile, stepped back to cover the origin rect's min edge on repeating axes.
        let start_x = if matches!(self.repeat, BgRepeat::X | BgRepeat::Both) {
            origin.min.x - (origin.min.x - ax).rem_euclid(dw)
        } else {
            ax
        };
        let start_y = if matches!(self.repeat, BgRepeat::Y | BgRepeat::Both) {
            origin.min.y - (origin.min.y - ay).rem_euclid(dh)
        } else {
            ay
        };
        let end_x = if matches!(self.repeat, BgRepeat::X | BgRepeat::Both) { origin.max.x } else { start_x + 1 };
        let end_y = if matches!(self.repeat, BgRepeat::Y | BgRepeat::Both) { origin.max.y } else { start_y + 1 };

        let mut y = start_y;
        while y < end_y {
            let mut x = start_x;
            while x < end_x {
                let tile = rect((x, y), (x + dw, y + dh));
                if let Some(key) = raster_cache_key {
                    if let Some((w, h, rgba)) = &self.rasterized {
                        theme.draw_raw_image(tile, rgba, (*w, *h), key);
                    }
                } else if let Some(bytes) = &self.bytes {
                    theme.draw_image(tile, bytes);
                }
                x += dw;
            }
            y += dh;
        }

        if self.opacity < 1.0 {
            theme.pop_opacity();
        }
        if clip_content {
            theme.pop_clip();
        }
    }
}

fn resolve_component(c: BgSizeComponent, origin_dim: f64, nat: f64, scale: f64) -> Option<f64> {
    match c {
        BgSizeComponent::Auto => None,
        BgSizeComponent::Dip(d) => Some(d as f64 * scale),
        BgSizeComponent::Percent(p) => Some(origin_dim * p as f64 / 100.0),
        BgSizeComponent::Factor(f) => Some(nat * f as f64),
    }
}

fn resolve_offset(off: BgOffset, origin_dim: i32, scale: f64) -> i32 {
    match off {
        BgOffset::Zero => 0,
        BgOffset::Dip(d) => (d as f64 * scale).round() as i32,
        BgOffset::Percent(p) => (origin_dim as f32 * p / 100.0).round() as i32,
    }
}

fn raster_key(path: &str, w: u32, h: u32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "frame_background".hash(&mut hasher);
    path.hash(&mut hasher);
    w.hash(&mut hasher);
    h.hash(&mut hasher);
    hasher.finish()
}

/// Parses `none | x | y | both` (anything else → `none`).
pub(crate) fn parse_repeat(s: &str) -> BgRepeat {
    match s.trim() {
        "x" => BgRepeat::X,
        "y" => BgRepeat::Y,
        "both" => BgRepeat::Both,
        _ => BgRepeat::None,
    }
}

/// Parses `frame | content` (anything else → `frame`).
pub(crate) fn parse_origin(s: &str) -> BgOrigin {
    match s.trim() {
        "content" => BgOrigin::Content,
        _ => BgOrigin::Frame,
    }
}

/// Parses 1–2 whitespace-separated tokens, each `left|right|top|bottom|center`
/// optionally followed by `+N`, `-N`, `+N%` or `-N%` (offset inward from that
/// edge, in dip or percent of the origin rect). Order-agnostic; a lone token
/// leaves the other axis centered. E.g. `"center"`, `"top left"`, `"right+10 bottom+20%"`.
pub(crate) fn parse_position(s: &str) -> BgPosition {
    let mut h: Option<(HAlign, BgOffset)> = None;
    let mut v: Option<(VAlign, BgOffset)> = None;
    for token in s.split_whitespace().take(2) {
        let (kw, off) = split_pos_token(token);
        match kw {
            "left" => h = Some((HAlign::Left, off)),
            "right" => h = Some((HAlign::Right, off)),
            "top" => v = Some((VAlign::Top, off)),
            "bottom" => v = Some((VAlign::Bottom, off)),
            "center" => {
                if h.is_none() {
                    h = Some((HAlign::Center, off));
                } else {
                    v = Some((VAlign::Center, off));
                }
            }
            _ => {}
        }
    }
    let (h, h_offset) = h.unwrap_or((HAlign::Center, BgOffset::Zero));
    let (v, v_offset) = v.unwrap_or((VAlign::Center, BgOffset::Zero));
    BgPosition { h, h_offset, v, v_offset }
}

fn split_pos_token(token: &str) -> (&str, BgOffset) {
    match token.find(['+', '-']) {
        Some(i) => (&token[..i], parse_offset(&token[i..])),
        None => (token, BgOffset::Zero),
    }
}

fn parse_offset(s: &str) -> BgOffset {
    let (num, percent) = match s.strip_suffix('%') {
        Some(n) => (n, true),
        None => (s, false),
    };
    let num = num.strip_prefix('+').unwrap_or(num);
    match num.parse::<f32>() {
        Ok(v) if percent => BgOffset::Percent(v),
        Ok(v) => BgOffset::Dip(v),
        Err(_) => BgOffset::Zero,
    }
}

/// Parses `auto | cover | contain | <w> [<h>]` where each component is
/// `auto`, `N` (dip), `N%` (of the origin rect) or `Nx` (× natural size).
/// A single numeric token sets the width; the height follows the aspect ratio.
pub(crate) fn parse_size(s: &str) -> BgSize {
    match s.trim() {
        "auto" => BgSize::Auto,
        "cover" => BgSize::Cover,
        "contain" => BgSize::Contain,
        other => {
            let mut parts = other.split_whitespace();
            let w = parts.next().map(parse_size_component).unwrap_or_default();
            let h = parts.next().map(parse_size_component).unwrap_or_default();
            BgSize::Explicit(w, h)
        }
    }
}

fn parse_size_component(s: &str) -> BgSizeComponent {
    if s == "auto" {
        return BgSizeComponent::Auto;
    }
    if let Some(num) = s.strip_suffix('%') {
        return num.parse::<f32>().map(BgSizeComponent::Percent).unwrap_or_default();
    }
    if let Some(num) = s.strip_suffix('x') {
        return num.parse::<f32>().map(BgSizeComponent::Factor).unwrap_or_default();
    }
    s.parse::<f32>().map(BgSizeComponent::Dip).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_position() {
        let p = parse_position("center");
        assert_eq!(p.h, HAlign::Center);
        assert_eq!(p.v, VAlign::Center);

        let p = parse_position("top left");
        assert_eq!(p.h, HAlign::Left);
        assert_eq!(p.v, VAlign::Top);

        let p = parse_position("right+10 bottom-20%");
        assert_eq!(p.h, HAlign::Right);
        assert_eq!(p.h_offset, BgOffset::Dip(10.0));
        assert_eq!(p.v, VAlign::Bottom);
        assert_eq!(p.v_offset, BgOffset::Percent(-20.0));

        let p = parse_position("bottom");
        assert_eq!(p.h, HAlign::Center);
        assert_eq!(p.v, VAlign::Bottom);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("cover"), BgSize::Cover);
        assert_eq!(parse_size("contain"), BgSize::Contain);
        assert_eq!(parse_size("auto"), BgSize::Auto);
        assert_eq!(
            parse_size("64 48"),
            BgSize::Explicit(BgSizeComponent::Dip(64.0), BgSizeComponent::Dip(48.0))
        );
        assert_eq!(
            parse_size("50% auto"),
            BgSize::Explicit(BgSizeComponent::Percent(50.0), BgSizeComponent::Auto)
        );
        assert_eq!(
            parse_size("1.5x 0.5x"),
            BgSize::Explicit(BgSizeComponent::Factor(1.5), BgSizeComponent::Factor(0.5))
        );
        assert_eq!(
            parse_size("100"),
            BgSize::Explicit(BgSizeComponent::Dip(100.0), BgSizeComponent::Auto)
        );
    }

    #[test]
    fn test_parse_repeat_origin() {
        assert_eq!(parse_repeat("both"), BgRepeat::Both);
        assert_eq!(parse_repeat("x"), BgRepeat::X);
        assert_eq!(parse_repeat("nonsense"), BgRepeat::None);
        assert_eq!(parse_origin("content"), BgOrigin::Content);
        assert_eq!(parse_origin("frame"), BgOrigin::Frame);
    }
}
