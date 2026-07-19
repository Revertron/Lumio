//! Android-style 9-patch (`.9.png`) backgrounds.
//!
//! A 9-patch is a PNG with a 1px marker border: black runs on the top edge
//! mark stretchable columns, on the left edge stretchable rows, and runs on
//! the right/bottom edges define the content padding. Multiple stretch runs
//! per axis are supported (the "9" is really an N×M grid).
//!
//! Rendering is CPU-composited: [`crate::ninepatch::NinePatchSource`]
//! stretches the patch to the destination size into one RGBA buffer, caches it
//! (last size only, same shape as `ImageSource::rasterized`), and draws it
//! through [`crate::themes::Renderer::draw_raw_image_tinted`] — so both backends
//! share one code path and there are no GPU seam/filtering artifacts between
//! cells.
//!
//! [`crate::ninepatch::NinePatchBackground`] adds per-state skinning: it is
//! either a single patch or an Android `<selector>` XML whose `<item>`s
//! reference `.9.png` files, matched against the view's
//! [`crate::themes::ViewState`] at paint time.

use image::imageops::{self, FilterType};
use image::RgbaImage;
use log::warn;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::assets::get_asset;
use crate::drawing::parser::DrawableParser;
use crate::drawing::selector::StateMatcher;
use crate::image_source::{next_image_id, push_pending};
use crate::themes::{Renderer, ViewState};
use crate::types::Rect;
use crate::views::Borders;

/// A parsed 9-patch: the content bitmap (marker border stripped) plus the
/// stretch/padding metadata read from the border.
pub(crate) struct NinePatchData {
    /// The image without its 1px marker border: `(w-2) × (h-2)`.
    pub content: RgbaImage,
    /// Stretchable column runs, half-open `[start, end)` in content coords.
    pub stretch_x: Vec<(u32, u32)>,
    /// Stretchable row runs, half-open `[start, end)` in content coords.
    pub stretch_y: Vec<(u32, u32)>,
    /// Content padding from the right/bottom markers, in source pixels.
    pub padding: Option<Borders>,
}

/// Lenient Android marker test: opaque-ish and dark counts as a marker.
fn is_marker(px: &image::Rgba<u8>) -> bool {
    px[3] > 127 && px[0] < 128 && px[1] < 128 && px[2] < 128
}

/// Contiguous `true` runs of a border line as half-open `(start, end)` ranges.
pub(crate) fn marker_runs(flags: impl Iterator<Item = bool>) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut start: Option<u32> = None;
    let mut i = 0u32;
    for flag in flags {
        match (flag, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s, i));
                start = None;
            }
            _ => {}
        }
        i += 1;
    }
    if let Some(s) = start {
        runs.push((s, i));
    }
    runs
}

/// Parse a decoded `.9.png`. Errors on images too small to hold a marker
/// border plus content. An axis without stretch markers is treated as fully
/// stretchable (with a warning), matching lenient Android behavior.
pub(crate) fn parse_nine_patch(img: &RgbaImage) -> Result<NinePatchData, String> {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return Err(format!("9-patch must be at least 3x3, got {}x{}", w, h));
    }
    let (cw, ch) = (w - 2, h - 2);

    let mut stretch_x = marker_runs((1..w - 1).map(|x| is_marker(img.get_pixel(x, 0))));
    let mut stretch_y = marker_runs((1..h - 1).map(|y| is_marker(img.get_pixel(0, y))));
    if stretch_x.is_empty() {
        warn!("9-patch: no horizontal stretch markers, treating full width as stretchable");
        stretch_x.push((0, cw));
    }
    if stretch_y.is_empty() {
        warn!("9-patch: no vertical stretch markers, treating full height as stretchable");
        stretch_y.push((0, ch));
    }

    // Padding: bottom row = horizontal range, right column = vertical range.
    let pad_x = marker_runs((1..w - 1).map(|x| is_marker(img.get_pixel(x, h - 1))));
    let pad_y = marker_runs((1..h - 1).map(|y| is_marker(img.get_pixel(w - 1, y))));
    let padding = if pad_x.is_empty() && pad_y.is_empty() {
        None
    } else {
        let (left, right) = match (pad_x.first(), pad_x.last()) {
            (Some(first), Some(last)) => (first.0 as i32, (cw - last.1) as i32),
            _ => (0, 0),
        };
        let (top, bottom) = match (pad_y.first(), pad_y.last()) {
            (Some(first), Some(last)) => (first.0 as i32, (ch - last.1) as i32),
            _ => (0, 0),
        };
        Some(Borders::new(top, left, right, bottom))
    };

    let content = imageops::crop_imm(img, 1, 1, cw, ch).to_image();
    Ok(NinePatchData { content, stretch_x, stretch_y, padding })
}

/// Split one axis of length `total` into alternating fixed/stretch segments
/// covering `[0, total)`. Returns `(src_start, src_end, is_stretch)`.
pub(crate) fn segments(total: u32, stretch: &[(u32, u32)]) -> Vec<(u32, u32, bool)> {
    let mut segs = Vec::new();
    let mut pos = 0u32;
    for &(start, end) in stretch {
        let start = start.min(total);
        let end = end.min(total);
        if start > pos {
            segs.push((pos, start, false));
        }
        if end > start {
            segs.push((start, end, true));
        }
        pos = pos.max(end);
    }
    if pos < total {
        segs.push((pos, total, false));
    }
    segs
}

/// Destination edge positions for the segment grid: `segs.len() + 1` values,
/// starting at 0 and ending exactly at `dest`. Fixed segments target
/// `src × scale`; the leftover is distributed to stretch segments
/// proportionally to their source sizes. If the destination is smaller than
/// the fixed regions, the fixed regions shrink proportionally and stretch
/// cells collapse to zero. Rounding happens per *edge* (cumulative float
/// positions), so adjacent cells always tile seamlessly.
pub(crate) fn dest_edges(segs: &[(u32, u32, bool)], dest: u32, scale: f64) -> Vec<i32> {
    let fixed_total: f64 = segs
        .iter()
        .filter(|s| !s.2)
        .map(|s| (s.1 - s.0) as f64 * scale)
        .sum();
    let stretch_src_total: f64 = segs.iter().filter(|s| s.2).map(|s| (s.1 - s.0) as f64).sum();
    let leftover = (dest as f64 - fixed_total).max(0.0);
    let fixed_factor = if dest as f64 >= fixed_total || fixed_total <= 0.0 {
        1.0
    } else {
        dest as f64 / fixed_total
    };

    let mut edges = Vec::with_capacity(segs.len() + 1);
    let mut pos = 0.0f64;
    edges.push(0);
    for &(start, end, is_stretch) in segs {
        let src = (end - start) as f64;
        pos += if is_stretch {
            if stretch_src_total > 0.0 { leftover * src / stretch_src_total } else { 0.0 }
        } else {
            src * scale * fixed_factor
        };
        edges.push(pos.round() as i32);
    }
    // Absorb any accumulated rounding drift into the last cell.
    *edges.last_mut().unwrap() = dest as i32;
    edges
}

/// Composite the patch into a straight-alpha RGBA buffer of exactly
/// `dest_w × dest_h`, scaling fixed cells by `scale` and stretching the rest.
pub(crate) fn composite(data: &NinePatchData, dest_w: u32, dest_h: u32, scale: f64) -> Vec<u8> {
    let (cw, ch) = data.content.dimensions();
    let xsegs = segments(cw, &data.stretch_x);
    let ysegs = segments(ch, &data.stretch_y);
    let xe = dest_edges(&xsegs, dest_w, scale);
    let ye = dest_edges(&ysegs, dest_h, scale);

    let mut out = vec![0u8; (dest_w * dest_h * 4) as usize];
    let src_raw = data.content.as_raw();

    for (yi, &(sy0, sy1, _)) in ysegs.iter().enumerate() {
        let (dy0, dy1) = (ye[yi], ye[yi + 1]);
        let (sh, dh) = (sy1 - sy0, (dy1 - dy0).max(0) as u32);
        if sh == 0 || dh == 0 {
            continue;
        }
        for (xi, &(sx0, sx1, _)) in xsegs.iter().enumerate() {
            let (dx0, dx1) = (xe[xi], xe[xi + 1]);
            let (sw, dw) = (sx1 - sx0, (dx1 - dx0).max(0) as u32);
            if sw == 0 || dw == 0 {
                continue;
            }
            if (sw, sh) == (dw, dh) {
                // 1:1 cell — copy rows straight out of the content buffer.
                for r in 0..sh {
                    let so = (((sy0 + r) * cw + sx0) * 4) as usize;
                    let dst = (((dy0 as u32 + r) * dest_w + dx0 as u32) * 4) as usize;
                    out[dst..dst + (sw * 4) as usize]
                        .copy_from_slice(&src_raw[so..so + (sw * 4) as usize]);
                }
            } else {
                let sub = imageops::crop_imm(&data.content, sx0, sy0, sw, sh);
                let resized = imageops::resize(&*sub, dw, dh, FilterType::CatmullRom);
                let res_raw = resized.as_raw();
                for r in 0..dh {
                    let so = ((r * dw) * 4) as usize;
                    let dst = (((dy0 as u32 + r) * dest_w + dx0 as u32) * 4) as usize;
                    out[dst..dst + (dw * 4) as usize]
                        .copy_from_slice(&res_raw[so..so + (dw * 4) as usize]);
                }
            }
        }
    }
    out
}

/// One 9-patch image: loads a `.9.png` asset, composites it at the draw size,
/// caches the composite (and the corresponding texture, via the shared image
/// id/eviction machinery of `ImageSource`), and re-composites only when the
/// destination size or scale changes.
pub struct NinePatchSource {
    /// Current texture cache key; retired (evicted) on re-composite and drop.
    id: u64,
    path: String,
    loaded: bool,
    data: Option<NinePatchData>,
    /// The composited RGBA at the last drawn size `(w, h)`.
    rasterized: Option<(u32, u32, Vec<u8>)>,
    /// The scale the cached composite was built at.
    rasterized_scale: f64,
}

impl NinePatchSource {
    pub fn new(path: &str) -> Self {
        NinePatchSource {
            id: next_image_id(),
            path: path.to_owned(),
            loaded: false,
            data: None,
            rasterized: None,
            rasterized_scale: 0.0,
        }
    }

    /// Loads and parses the `.9.png` on first use. A missing asset or invalid
    /// marker border is logged once; the source then stays empty. Idempotent.
    pub fn ensure_loaded(&mut self) {
        if self.loaded || self.path.is_empty() {
            return;
        }
        self.loaded = true;
        let Some(bytes) = get_asset(&self.path) else {
            warn!("NinePatchSource: asset not found: {}", self.path);
            return;
        };
        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                warn!("NinePatchSource: failed to decode {}: {}", self.path, e);
                return;
            }
        };
        match parse_nine_patch(&img) {
            Ok(data) => self.data = Some(data),
            Err(e) => warn!("NinePatchSource: invalid 9-patch {}: {}", self.path, e),
        }
    }

    /// Content padding from the right/bottom markers, in source pixels
    /// (dip-like — the caller multiplies by scale). Loads the asset if needed.
    pub fn content_padding(&mut self) -> Option<Borders> {
        self.ensure_loaded();
        self.data.as_ref().and_then(|d| d.padding)
    }

    /// Draw the patch stretched to exactly `rect`. Fixed regions are sized at
    /// `source px × scale`. Re-composites when the size or scale changed,
    /// retiring the previous texture (see `ImageSource` for the id scheme).
    pub fn draw(&mut self, theme: &mut dyn Renderer, rect: Rect<i32>, scale: f64, tint: u32) {
        self.ensure_loaded();
        let w = rect.width().max(0) as u32;
        let h = rect.height().max(0) as u32;
        if w == 0 || h == 0 || self.data.is_none() {
            return;
        }
        let needs_render = match &self.rasterized {
            Some((cw, ch, _)) => *cw != w || *ch != h || self.rasterized_scale != scale,
            None => true,
        };
        if needs_render {
            let rgba = composite(self.data.as_ref().unwrap(), w, h, scale);
            // A previous texture existed at a different size: retire its id so
            // the cache frees it, and take a fresh id so the upload below is a
            // cache miss.
            if self.rasterized.is_some() {
                push_pending(self.id);
                self.id = next_image_id();
            }
            self.rasterized = Some((w, h, rgba));
            self.rasterized_scale = scale;
        }
        if let Some((cw, ch, rgba)) = &self.rasterized {
            theme.draw_raw_image_tinted(rect, rgba, (*cw, *ch), self.id, tint);
        }
    }
}

impl Drop for NinePatchSource {
    fn drop(&mut self) {
        // Enqueue the texture for eviction at the next paint, like ImageSource.
        push_pending(self.id);
    }
}

/// A view background made of one or more 9-patches selected by view state:
/// either a single `.9.png` (all states) or an Android-style `<selector>` XML
/// whose `<item>`s carry `state_*` attributes and a `src` `.9.png` path.
pub struct NinePatchBackground {
    /// `(matcher, patch)` in document order; the last item is the default.
    items: Vec<(StateMatcher, NinePatchSource)>,
}

impl NinePatchBackground {
    /// A single patch used for every state.
    pub fn from_png(path: &str) -> Self {
        NinePatchBackground { items: vec![(StateMatcher::new(), NinePatchSource::new(path))] }
    }

    /// Load a `<selector>` XML asset. The XML is parsed eagerly; the
    /// referenced `.9.png`s are decoded lazily on first draw.
    pub fn from_selector(path: &str) -> Result<Self, String> {
        let bytes = get_asset(path).ok_or_else(|| format!("asset not found: {}", path))?;
        let xml = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let items = parse_ninepatch_selector(&xml)?;
        if items.is_empty() {
            return Err(format!("selector {} has no <item> with a .9.png src", path));
        }
        Ok(NinePatchBackground { items })
    }

    /// First item whose matcher accepts `state`, else the last item as the
    /// default — same fallback semantics as `StateSelector::get_drawable`.
    fn item_for(&mut self, state: &ViewState) -> Option<&mut NinePatchSource> {
        let idx = self
            .items
            .iter()
            .position(|(matcher, _)| matcher.matches(state))
            .unwrap_or(self.items.len().saturating_sub(1));
        self.items.get_mut(idx).map(|(_, src)| src)
    }

    /// Content padding of the *default* (last) item, in source pixels. Kept
    /// state-independent so state changes never trigger a relayout.
    pub fn content_padding(&mut self) -> Option<Borders> {
        self.items.last_mut().and_then(|(_, src)| src.content_padding())
    }

    /// Draw the state-matched patch stretched over `rect`.
    pub fn paint(&mut self, theme: &mut dyn Renderer, rect: Rect<i32>, state: &ViewState, scale: f64) {
        if let Some(src) = self.item_for(state) {
            src.draw(theme, rect, scale, 0xFFFFFFFF);
        }
    }
}

/// Parse a 9-patch `<selector>` XML: `<item state_*="..." src="x.9.png"/>`.
/// Items without a `.9.png` `src` are skipped with a warning.
fn parse_ninepatch_selector(xml: &str) -> Result<Vec<(StateMatcher, NinePatchSource)>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().0 == b"item" {
                    let matcher = DrawableParser::parse_state_matcher(&e)?;
                    match DrawableParser::get_attr_opt(&e, "src") {
                        Some(src) if src.to_ascii_lowercase().ends_with(".9.png") => {
                            items.push((matcher, NinePatchSource::new(&src)));
                        }
                        other => warn!(
                            "9-patch selector: skipping <item> without a .9.png src ({:?})",
                            other
                        ),
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    const BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);

    /// Build a (cw+2)x(ch+2) 9-patch image: `fill(x, y)` colors the content,
    /// markers are painted from the given content-coordinate runs.
    fn build_patch(
        cw: u32,
        ch: u32,
        stretch_x: &[(u32, u32)],
        stretch_y: &[(u32, u32)],
        pad_x: &[(u32, u32)],
        pad_y: &[(u32, u32)],
        fill: impl Fn(u32, u32) -> Rgba<u8>,
    ) -> RgbaImage {
        let mut img = RgbaImage::new(cw + 2, ch + 2);
        for y in 0..ch {
            for x in 0..cw {
                img.put_pixel(x + 1, y + 1, fill(x, y));
            }
        }
        for &(s, e) in stretch_x {
            for x in s..e {
                img.put_pixel(x + 1, 0, BLACK);
            }
        }
        for &(s, e) in stretch_y {
            for y in s..e {
                img.put_pixel(0, y + 1, BLACK);
            }
        }
        for &(s, e) in pad_x {
            for x in s..e {
                img.put_pixel(x + 1, ch + 1, BLACK);
            }
        }
        for &(s, e) in pad_y {
            for y in s..e {
                img.put_pixel(cw + 1, y + 1, BLACK);
            }
        }
        img
    }

    #[test]
    fn marker_runs_basic() {
        assert!(marker_runs([false, false].into_iter()).is_empty());
        assert_eq!(marker_runs([false, true, true, false].into_iter()), vec![(1, 3)]);
        assert_eq!(
            marker_runs([true, false, true, true].into_iter()),
            vec![(0, 1), (2, 4)]
        );
        // Run touching both ends
        assert_eq!(marker_runs([true, true, true].into_iter()), vec![(0, 3)]);
    }

    #[test]
    fn parse_stretch_and_padding() {
        // 5x5 content, stretch middle column run (2..3) and rows (1..4),
        // padding 1px left/right (markers 1..4 on bottom), 2 top/0 bottom.
        let img = build_patch(5, 5, &[(2, 3)], &[(1, 4)], &[(1, 4)], &[(2, 5)], |_, _| {
            Rgba([10, 20, 30, 255])
        });
        let data = parse_nine_patch(&img).unwrap();
        assert_eq!(data.content.dimensions(), (5, 5));
        assert_eq!(data.stretch_x, vec![(2, 3)]);
        assert_eq!(data.stretch_y, vec![(1, 4)]);
        let pad = data.padding.unwrap();
        assert_eq!((pad.left, pad.right), (1, 1));
        assert_eq!((pad.top, pad.bottom), (2, 0));
    }

    #[test]
    fn parse_marker_leniency_and_fallbacks() {
        // Dark translucent-ish pixels count as markers; light ones don't.
        assert!(is_marker(&Rgba([50, 50, 50, 200])));
        assert!(!is_marker(&Rgba([0, 0, 0, 100])));
        assert!(!is_marker(&Rgba([200, 0, 0, 255])));

        // No stretch markers at all -> whole axis stretchable, no padding.
        let img = build_patch(4, 3, &[], &[], &[], &[], |_, _| Rgba([1, 2, 3, 255]));
        let data = parse_nine_patch(&img).unwrap();
        assert_eq!(data.stretch_x, vec![(0, 4)]);
        assert_eq!(data.stretch_y, vec![(0, 3)]);
        assert!(data.padding.is_none());

        // Too small to carry content.
        assert!(parse_nine_patch(&RgbaImage::new(2, 5)).is_err());
    }

    #[test]
    fn segments_alternate() {
        assert_eq!(
            segments(10, &[(3, 6)]),
            vec![(0, 3, false), (3, 6, true), (6, 10, false)]
        );
        assert_eq!(segments(4, &[(0, 4)]), vec![(0, 4, true)]);
        assert_eq!(
            segments(10, &[(1, 3), (5, 8)]),
            vec![(0, 1, false), (1, 3, true), (3, 5, false), (5, 8, true), (8, 10, false)]
        );
    }

    #[test]
    fn dest_edges_scales_and_tiles() {
        let segs = segments(10, &[(3, 6)]); // fixed 3 | stretch 3 | fixed 4
        for &(dest, scale) in &[(30u32, 1.0f64), (31, 1.5), (10, 1.0), (100, 2.0)] {
            let edges = dest_edges(&segs, dest, scale);
            assert_eq!(edges.len(), segs.len() + 1);
            assert_eq!(edges[0], 0);
            assert_eq!(*edges.last().unwrap(), dest as i32, "dest={} scale={}", dest, scale);
            assert!(edges.windows(2).all(|w| w[0] <= w[1]), "monotone: {:?}", edges);
        }
        // Exact split at scale 1.0: fixed cells keep source size.
        let edges = dest_edges(&segs, 30, 1.0);
        assert_eq!(edges, vec![0, 3, 26, 30]);

        // Two stretch segments share leftover proportionally (1:3).
        let segs2 = segments(10, &[(1, 3), (5, 8)]); // stretch sizes 2 and 3
        let edges2 = dest_edges(&segs2, 15, 1.0); // fixed total 5, leftover 10
        assert_eq!(edges2, vec![0, 1, 5, 7, 13, 15]); // stretch cells 4 and 6

        // dest smaller than fixed regions: stretch collapses, fixed shrinks.
        let edges3 = dest_edges(&segs, 4, 1.0); // fixed total 7 > 4
        assert_eq!(edges3[0], 0);
        assert_eq!(*edges3.last().unwrap(), 4);
        assert_eq!(edges3[1] - edges3[0] + (edges3[3] - edges3[2]), 4); // stretch cell = 0
    }

    #[test]
    fn composite_preserves_corners_and_stretches_center() {
        // 3x3 content: distinct corner colors, red center.
        let colors = |x: u32, y: u32| -> Rgba<u8> {
            match (x, y) {
                (0, 0) => Rgba([1, 0, 0, 255]),
                (2, 0) => Rgba([2, 0, 0, 255]),
                (0, 2) => Rgba([3, 0, 0, 255]),
                (2, 2) => Rgba([4, 0, 0, 255]),
                _ => Rgba([255, 0, 0, 255]),
            }
        };
        let img = build_patch(3, 3, &[(1, 2)], &[(1, 2)], &[], &[], colors);
        let data = parse_nine_patch(&img).unwrap();

        for &(w, h) in &[(9u32, 7u32), (20, 11)] {
            let out = composite(&data, w, h, 1.0);
            assert_eq!(out.len(), (w * h * 4) as usize);
            let px = |x: u32, y: u32| {
                let i = ((y * w + x) * 4) as usize;
                [out[i], out[i + 1], out[i + 2], out[i + 3]]
            };
            assert_eq!(px(0, 0), [1, 0, 0, 255]);
            assert_eq!(px(w - 1, 0), [2, 0, 0, 255]);
            assert_eq!(px(0, h - 1), [3, 0, 0, 255]);
            assert_eq!(px(w - 1, h - 1), [4, 0, 0, 255]);
            // Stretched center stays solid red.
            assert_eq!(px(w / 2, h / 2), [255, 0, 0, 255]);
        }
    }

    #[test]
    fn selector_xml_parses_states_in_order() {
        let xml = r#"
            <selector>
                <item state_pressed="true" src="btn_pressed.9.png"/>
                <item state_hovered="true" src="btn_hover.9.png"/>
                <item src="btn.9.png"/>
                <item src="not_a_ninepatch.png"/>
            </selector>
        "#;
        let items = parse_ninepatch_selector(xml).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].0.pressed, Some(true));
        assert_eq!(items[1].0.hovered, Some(true));
        assert!(items[2].0.pressed.is_none() && items[2].0.hovered.is_none());

        let mut state = ViewState { pressed: true, ..Default::default() };
        assert!(items[0].0.matches(&state));
        state.pressed = false;
        assert!(!items[0].0.matches(&state));
        assert!(items[2].0.matches(&state));
    }
}
