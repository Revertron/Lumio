//! A single owning home for a view's image: its source bytes, SVG flag, natural
//! size, and (for SVG) the rasterized buffer — plus a globally-unique `id` that
//! is the texture's key in the theme's GPU image cache.
//!
//! Why an id instead of a content/size hash: the id is unique per `ImageSource`
//! instance and stable for its life, so there is exactly **one** texture per
//! image at any time. A content change replaces the whole `ImageSource` (the old
//! one's `Drop` frees its texture); a size change re-rasterizes inside `draw`,
//! bumping the id and retiring the old texture. This retires the old
//! `as_ptr`-keyed cache (which leaked on swap and could draw the wrong image) and
//! the per-size hash key (which leaked one texture per laid-out size on resize).

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};

use image::GenericImageView;

use crate::assets::get_asset;
use crate::svg;
use crate::themes::Theme;
use crate::types::Rect;

/// Source of the next image id. Starts at 1 so 0 can never be a live key.
static NEXT_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Texture ids whose `ImageSource` was dropped or re-rasterized at a new
    /// size. Drained by `Win::on_draw` (when a GL context is current) and removed
    /// from that window's image cache. Only touched from the UI thread.
    static PENDING_IMAGE_EVICTIONS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

fn push_pending(id: u64) {
    PENDING_IMAGE_EVICTIONS.with(|q| q.borrow_mut().push(id));
}

/// Take all pending texture-eviction ids. The window handler calls this at the
/// top of `on_draw` and removes the ids from its own image cache.
pub fn take_pending_evictions() -> Vec<u64> {
    PENDING_IMAGE_EVICTIONS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Put back ids that did not belong to the draining window's cache (each id
/// lives in exactly one window's cache, so another window will claim them).
pub fn requeue_evictions(keys: Vec<u64>) {
    if keys.is_empty() {
        return;
    }
    PENDING_IMAGE_EVICTIONS.with(|q| q.borrow_mut().extend(keys));
}

/// Drain pending texture evictions into `cache`, removing each id from it. Ids
/// not in this cache are requeued — each id lives in exactly one window's cache,
/// so the owning window claims them on its next paint. Both backends call this
/// at the top of their per-frame paint (`Win::on_draw` / software `render`),
/// where the cache's graphics context is current and textures can be freed.
pub fn drain_evictions<V>(cache: &mut HashMap<u64, V>) {
    let mut not_mine = Vec::new();
    for id in take_pending_evictions() {
        if cache.remove(&id).is_none() {
            not_mine.push(id);
        }
    }
    requeue_evictions(not_mine);
}

pub struct ImageSource {
    /// Current GPU cache key. Plain field — every method takes `&mut self`.
    id: u64,
    path: String,
    /// Raw file bytes. For raster images these are handed to `draw_image_tinted`
    /// (speedy decodes them); for SVG they are the source for `svg::rasterize`.
    bytes: Option<Vec<u8>>,
    is_svg: bool,
    natural_size: (u32, u32),
    /// SVG only: the rasterized RGBA at the last drawn size `(w, h)`.
    rasterized: Option<(u32, u32, Vec<u8>)>,
    loaded: bool,
}

impl ImageSource {
    pub fn new(path: &str) -> Self {
        ImageSource {
            id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
            path: path.to_owned(),
            bytes: None,
            is_svg: false,
            natural_size: (0, 0),
            rasterized: None,
            loaded: false,
        }
    }

    /// `None` for an empty path, else `Some(ImageSource::new(path))`. For optional
    /// slots (icons) where an empty path means "no image".
    pub fn for_path(path: &str) -> Option<Self> {
        if path.is_empty() {
            None
        } else {
            Some(Self::new(path))
        }
    }

    /// Loads the asset on first use, decoding the natural size (usvg for SVG,
    /// the `image` crate for raster). A missing asset is logged once. Idempotent
    /// — safe to call eagerly (e.g. during layout) so `is_loaded` is meaningful.
    pub fn ensure_loaded(&mut self) {
        if self.loaded || self.path.is_empty() {
            return;
        }
        self.loaded = true;
        let Some(bytes) = get_asset(&self.path) else {
            println!("ImageSource: asset not found: {}", self.path);
            return;
        };
        let is_svg = self.path.to_ascii_lowercase().ends_with(".svg") || svg::looks_like_svg(&bytes);
        if is_svg {
            if let Ok(tree) = usvg::Tree::from_data(&bytes, &usvg::Options::default()) {
                let s = tree.size();
                self.natural_size = (s.width().ceil() as u32, s.height().ceil() as u32);
            } else {
                println!("ImageSource: failed to parse SVG: {}", self.path);
            }
        } else {
            match image::load(
                Cursor::new(&bytes),
                image::ImageFormat::from_path(&self.path).unwrap_or(image::ImageFormat::Png),
            ) {
                Ok(img) => self.natural_size = img.dimensions(),
                Err(e) => println!("ImageSource: failed to decode image: {}", e),
            }
        }
        self.is_svg = is_svg;
        self.bytes = Some(bytes);
    }

    /// Natural (intrinsic) pixel size of the image, loading it if needed.
    pub fn natural_size(&mut self) -> (u32, u32) {
        self.ensure_loaded();
        self.natural_size
    }

    /// Whether the image source resolved to anything drawable.
    pub fn is_loaded(&self) -> bool {
        self.bytes.is_some()
    }

    /// Draw the image filling exactly `rect`, multiplied by the ARGB `tint`
    /// (`0xFFFFFFFF` = no change). No internal aspect-fit — callers that want
    /// letterboxing compute the fitted rect themselves. For SVG this
    /// re-rasterizes when `rect`'s size changed, retiring the previous texture.
    pub fn draw(&mut self, theme: &mut dyn Theme, rect: Rect<i32>, tint: u32) {
        self.ensure_loaded();
        let w = rect.width().max(0) as u32;
        let h = rect.height().max(0) as u32;
        if self.bytes.is_none() || w == 0 || h == 0 {
            return;
        }

        if self.is_svg {
            let needs_render = match &self.rasterized {
                Some((cw, ch, _)) => *cw != w || *ch != h,
                None => true,
            };
            if needs_render {
                // Render first (borrowing `bytes`), then mutate `rasterized`/`id`.
                let rendered = self.bytes.as_ref().and_then(|src| svg::rasterize(src, w, h));
                if let Some(rgba) = rendered {
                    // A previous texture for this source existed at a different
                    // size: retire its id so the cache frees it, and take a fresh
                    // id so the upload below is a cache miss.
                    if self.rasterized.is_some() {
                        push_pending(self.id);
                        self.id = NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed);
                    }
                    self.rasterized = Some((w, h, rgba));
                }
            }
            if let Some((cw, ch, rgba)) = &self.rasterized {
                theme.draw_raw_image_tinted(rect, rgba, (*cw, *ch), self.id, tint);
            }
        } else if let Some(bytes) = &self.bytes {
            theme.draw_image_tinted(rect, bytes, self.id, tint);
        }
    }
}

impl Drop for ImageSource {
    fn drop(&mut self) {
        // Enqueue this image's texture id for eviction at the next on_draw, when
        // a GL context is current (deleting a GL texture needs its context). Views
        // are Rc-based / !Send, so an ImageSource is only ever dropped on the UI
        // thread — the same thread that drains the queue in Win::on_draw.
        push_pending(self.id);
    }
}
