use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use speedy2d::dimen::Vector2;
use speedy2d::font::{FormattedTextBlock, TextLayout, TextOptions};
use speedy2d::window::MouseButton;

use crate::assets::get_font_family;
use crate::events::EventType;
use crate::themes::{FontStyle, Theme, Typeface, ViewState};
use crate::traits::{Element, View, WeakElement};
use crate::types::{Point, Rect, rect};
use crate::ui::UI;
use crate::views::{Borders, Dimension, Gravity, Visibility};
use crate::views::{FieldsMain, FieldsTexted};
use crate::styles::selector::FontSelector;
use crate::view_base::{HasMainFields, ViewBasics, parse_hex_color};

const DEFAULT_LINK_COLOR: u32 = 0xFF3273DC; // Bulma link blue (same as Label)
const DEFAULT_MARK_COLOR: u32 = 0xFFFFF59D; // soft yellow highlight for <mark>
const BIG_FACTOR: f32 = 1.25;
const SMALL_FACTOR: f32 = 0.8;

/// The resolved style of a contiguous run of characters. Cheap to clone — the
/// only heap field is the (shared) link target. Produced by the HTML parser and
/// by the programmatic builder; both feed the same layout pipeline.
#[derive(Clone, PartialEq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Explicit foreground colour. `None` means "use the theme default (or the
    /// link colour when this is a link)".
    pub color: Option<u32>,
    /// Highlight colour drawn behind the glyphs.
    pub background: Option<u32>,
    /// Relative size multiplier (`<big>`/`<small>`); default `1.0`.
    pub size_scale: f32,
    /// Absolute size in dip (`<font size>`); wins over `size_scale`'s base.
    pub size_abs: Option<f32>,
    /// `Some` => render as a link (link colour + underline) and make it clickable.
    pub href: Option<Rc<String>>,
}

impl Default for SpanStyle {
    fn default() -> Self {
        SpanStyle {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            color: None,
            background: None,
            size_scale: 1.0,
            size_abs: None,
            href: None,
        }
    }
}

#[allow(dead_code)]
impl SpanStyle {
    pub fn bold(mut self) -> Self { self.bold = true; self }
    pub fn italic(mut self) -> Self { self.italic = true; self }
    pub fn underline(mut self) -> Self { self.underline = true; self }
    pub fn strike(mut self) -> Self { self.strike = true; self }
    pub fn color(mut self, color: u32) -> Self { self.color = Some(color); self }
    pub fn background(mut self, color: u32) -> Self { self.background = Some(color); self }
    /// Absolute size in dip.
    pub fn size(mut self, dip: f32) -> Self { self.size_abs = Some(dip); self }
    /// Multiply the current relative size factor.
    pub fn relative(mut self, factor: f32) -> Self { self.size_scale *= factor; self }
    pub fn link(mut self, href: &str) -> Self { self.href = Some(Rc::new(href.to_owned())); self }
}

/// One style section over a byte range of the backing text.
struct Section {
    range: Range<usize>,
    style: SpanStyle,
}

/// Plain text + a flat, contiguous list of style sections (egui `LayoutJob`
/// model). Hard breaks are `\n` characters in `text`.
#[derive(Default)]
struct RichContent {
    text: String,
    sections: Vec<Section>,
}

impl RichContent {
    fn push(&mut self, text: &str, style: SpanStyle) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        self.sections.push(Section { range: start..end, style });
    }

    /// Drop a trailing collapsed space (leading whitespace is already
    /// suppressed during parsing). At most one space can be trailing.
    fn trim_trailing_space(&mut self) {
        if self.text.ends_with(' ') {
            self.text.pop();
            if let Some(last) = self.sections.last_mut() {
                last.range.end -= 1;
                if last.range.is_empty() {
                    self.sections.pop();
                }
            }
        }
    }
}

/// A single laid-out word: a speedy2d text block placed at `(x, top)` (relative
/// to the content origin).
struct PlacedWord {
    x: i32,
    top: i32,
    block: FormattedTextBlock,
}

/// A maximal run of words sharing one `SpanStyle` on a single line. Drawn with
/// one styled colour; its `x..x+width` extent (spaces included) gives continuous
/// backgrounds / underlines and a single link hit rectangle.
struct PlacedRun {
    words: Vec<PlacedWord>,
    x: i32,
    width: i32,
    style: SpanStyle,
}

struct LaidLine {
    runs: Vec<PlacedRun>,
    top: i32,
    baseline: i32,
    height: i32,
}

struct LaidOut {
    lines: Vec<LaidLine>,
    width: i32,
    height: i32,
    /// `(wrap_width, scale)` this layout was produced for — the cache key.
    laid_for: (i32, f64),
}

pub struct RichText {
    state: RefCell<FieldsTexted>,
    content: RefCell<RichContent>,
    laid: RefCell<Option<LaidOut>>,
    link_color: RefCell<u32>,
    /// True when the content contains at least one link — gates mouse routing.
    has_link: Cell<bool>,
    /// Link pressed on mouse-down, so the click only fires if release lands on
    /// the same link (drag-off cancels), mirroring `Label`.
    pressed_href: RefCell<Option<Rc<String>>>,
    /// The href of the most recent click, readable from a `Click` handler.
    clicked_href: RefCell<Option<String>>,
    /// `(wrap_width, scale)` of the last layout — lets `paint` re-lay-out at the
    /// correct width when content changed since the last `layout_content`.
    last_wrap: Cell<Option<(i32, f64)>>,
}

impl HasMainFields for RichText {
    fn main_fields(&self) -> &RefCell<FieldsMain> {
        // SAFETY: `FieldsTexted` begins with `main: FieldsMain`, so a
        // `&RefCell<FieldsTexted>` reinterprets as `&RefCell<FieldsMain>`.
        // Same pattern used by every other view (see `Label`).
        unsafe { std::mem::transmute(&self.state) }
    }
}

impl ViewBasics for RichText {}

#[allow(dead_code)]
impl RichText {
    pub fn new(rect: Rect<i32>, text_size: f32) -> RichText {
        let mut main = FieldsMain::with_rect(rect, Dimension::Max, Dimension::Min);
        main.state.focusable = false;
        RichText {
            state: RefCell::new(FieldsTexted {
                main,
                text: String::new(),
                text_size,
                line_height: 0f32,
                single_line: false,
                cached_text: None,
                font: FontSelector::new(),
                listeners: HashMap::new(),
            }),
            content: RefCell::new(RichContent::default()),
            laid: RefCell::new(None),
            link_color: RefCell::new(DEFAULT_LINK_COLOR),
            has_link: Cell::new(false),
            pressed_href: RefCell::new(None),
            clicked_href: RefCell::new(None),
            last_wrap: Cell::new(None),
        }
    }

    /// Replace the content from an HTML-subset string. Supported tags:
    /// `b`/`strong`, `i`/`em`, `u`/`ins`, `s`/`del`/`strike`, `mark`,
    /// `big`/`small`, `font color/size`, `span color/background`, `a href`,
    /// `br`. Entities (`&amp;` `&lt;` `&gt;` `&quot;` `&#NN;`) are decoded and
    /// runs of whitespace are collapsed (like HTML).
    pub fn set_html(&self, html: &str) {
        let (content, has_link) = parse_html(html);
        self.has_link.set(has_link);
        self.base_set_focusable(has_link);
        *self.content.borrow_mut() = content;
        self.invalidate();
    }

    /// Append a styled run programmatically. `\n` inside `text` is a hard break.
    pub fn push(&self, text: &str, style: SpanStyle) {
        if style.href.is_some() {
            self.has_link.set(true);
            self.base_set_focusable(true);
        }
        self.content.borrow_mut().push(text, style);
        self.invalidate();
    }

    /// Append a clickable link run.
    pub fn push_link(&self, text: &str, href: &str) {
        self.push(text, SpanStyle::default().link(href));
    }

    /// Append a hard line break.
    pub fn push_break(&self) {
        self.content.borrow_mut().push("\n", SpanStyle::default());
        self.invalidate();
    }

    /// Remove all content.
    pub fn clear(&self) {
        *self.content.borrow_mut() = RichContent::default();
        self.has_link.set(false);
        self.invalidate();
    }

    /// The href of the most recent link click — read this from a `Click` handler.
    pub fn clicked_href(&self) -> Option<String> {
        self.clicked_href.borrow().clone()
    }

    pub fn set_link_color(&self, color: u32) {
        *self.link_color.borrow_mut() = color;
    }

    fn invalidate(&self) {
        *self.laid.borrow_mut() = None;
    }

    fn get_typeface(&self, parent_typeface: &Typeface) -> Typeface {
        self.state.borrow().main.font_manager.get_typeface(parent_typeface)
    }

    /// Re-lay-out the content for `(wrap_width, scale)` unless the cache already
    /// matches.
    fn ensure_laid(&self, wrap_width: i32, scale: f64) {
        if let Some(laid) = &*self.laid.borrow()
            && laid.laid_for == (wrap_width, scale)
        {
            return;
        }
        let laid = self.relayout(wrap_width, scale);
        self.last_wrap.set(Some((wrap_width, scale)));
        *self.laid.borrow_mut() = Some(laid);
    }

    fn relayout(&self, wrap_width: i32, scale: f64) -> LaidOut {
        let scale_f = scale as f32;
        let base_tf = self.get_typeface(&Typeface::default());
        let font_name = base_tf.font_name.clone();
        let base_style = base_tf.font_style;
        let base_px = base_tf
            .font_size
            .map(|dip| dip * scale_f)
            .unwrap_or(self.state.borrow().text_size);

        // Metrics for blank lines (consecutive breaks) and as a fallback.
        let (base_ascent, base_descmag) =
            layout_run_block(&font_name, base_style, base_px, scale_f, "Ag", &SpanStyle::default(), true)
                .map(|(_, _, a, d)| (a, -d))
                .unwrap_or((base_px, base_px * 0.25));

        let content = self.content.borrow();
        let toks = tokenize(&content);
        drop(content);

        let wrap = wrap_width as f32;
        let mut space_cache: HashMap<(u8, u32), f32> = HashMap::new();

        let mut lines_words: Vec<Vec<WordBox>> = Vec::new();
        let mut cur: Vec<WordBox> = Vec::new();
        let mut x = 0f32;
        let mut pending_space: Option<f32> = None;

        for tok in toks {
            match tok {
                Tok::Break => {
                    lines_words.push(std::mem::take(&mut cur));
                    x = 0.0;
                    pending_space = None;
                }
                Tok::Space(style) => {
                    if !cur.is_empty() {
                        let sw = measure_space(&mut space_cache, &font_name, base_style, base_px, scale_f, &style);
                        pending_space = Some(sw);
                    }
                }
                Tok::Word(text, style) => {
                    let laid = layout_run_block(&font_name, base_style, base_px, scale_f, &text, &style, true);
                    let (block, w, asc, desc) = match laid {
                        Some(v) => v,
                        None => continue,
                    };
                    let space_w = if cur.is_empty() { 0.0 } else { pending_space.take().unwrap_or(0.0) };
                    if !cur.is_empty() && wrap > 0.0 && x + space_w + w > wrap {
                        // Wrap: finish this line, start the word at the next line's left.
                        lines_words.push(std::mem::take(&mut cur));
                        cur.push(WordBox { x: 0, width: w.ceil() as i32, asc, desc, block, style });
                        x = w;
                    } else {
                        let wx = x + space_w;
                        cur.push(WordBox { x: wx.round() as i32, width: w.ceil() as i32, asc, desc, block, style });
                        x = wx + w;
                    }
                    pending_space = None;
                }
            }
        }
        lines_words.push(cur);

        let mut lines = Vec::with_capacity(lines_words.len());
        let mut top = 0i32;
        let mut max_w = 0i32;
        for words in lines_words {
            let line = finalize_line(words, top, base_ascent, base_descmag);
            for run in &line.runs {
                max_w = max_w.max(run.x + run.width);
            }
            top += line.height;
            lines.push(line);
        }

        LaidOut { lines, width: max_w, height: top, laid_for: (wrap_width, scale) }
    }

    /// Find the link href whose run rectangle contains `position` (local coords).
    fn link_at(&self, position: Vector2<i32>) -> Option<Rc<String>> {
        let laid = self.laid.borrow();
        let laid = laid.as_ref()?;
        let state = self.state.borrow();
        let scale = state.main.scale;
        let padding = state.main.padding.scaled(scale);
        let r = state.main.rect;
        let ox = r.min.x + padding.left;
        let oy = r.min.y + padding.top;
        for line in &laid.lines {
            for run in &line.runs {
                if let Some(href) = &run.style.href {
                    let rr = rect(
                        (ox + run.x, oy + line.top),
                        (ox + run.x + run.width, oy + line.top + line.height),
                    );
                    if rr.hit((position.x, position.y)) {
                        return Some(href.clone());
                    }
                }
            }
        }
        None
    }

    fn fire_click(&self, ui: &mut UI) -> bool {
        let handler = self.state.borrow_mut().listeners.remove(&EventType::Click);
        if let Some(mut handler) = handler {
            let result = handler(ui, self as &dyn View);
            self.state.borrow_mut().listeners.insert(EventType::Click, handler);
            return result;
        }
        false
    }
}

/// A measured word ready to be placed on a line. `x` is line-relative.
struct WordBox {
    x: i32,
    width: i32,
    asc: f32,
    desc: f32, // negative
    block: FormattedTextBlock,
    style: SpanStyle,
}

enum Tok {
    Word(String, SpanStyle),
    Space(SpanStyle),
    Break,
}

/// Split content into words / spaces / hard breaks. Words never cross style
/// sections (v1 simplification), so each token carries a single style.
fn tokenize(content: &RichContent) -> Vec<Tok> {
    let mut toks = Vec::new();
    for section in &content.sections {
        let style = &section.style;
        let text = &content.text[section.range.clone()];
        let mut word = String::new();
        for ch in text.chars() {
            if ch == '\n' {
                if !word.is_empty() {
                    toks.push(Tok::Word(std::mem::take(&mut word), style.clone()));
                }
                toks.push(Tok::Break);
            } else if ch.is_whitespace() {
                if !word.is_empty() {
                    toks.push(Tok::Word(std::mem::take(&mut word), style.clone()));
                }
                toks.push(Tok::Space(style.clone()));
            } else {
                word.push(ch);
            }
        }
        if !word.is_empty() {
            toks.push(Tok::Word(word, style.clone()));
        }
    }
    toks
}

/// Group a line's placed words into same-style runs and compute baseline.
fn finalize_line(words: Vec<WordBox>, top: i32, base_ascent: f32, base_descmag: f32) -> LaidLine {
    let (line_asc, line_descmag) = if words.is_empty() {
        (base_ascent, base_descmag)
    } else {
        let asc = words.iter().fold(0f32, |m, w| m.max(w.asc));
        let descmag = words.iter().fold(0f32, |m, w| m.max(-w.desc));
        (asc, descmag)
    };
    let baseline = top + line_asc.ceil() as i32;
    let height = (line_asc + line_descmag).ceil().max(1.0) as i32;

    let mut runs: Vec<PlacedRun> = Vec::new();
    let mut iter = words.into_iter().peekable();
    while let Some(w) = iter.next() {
        let block_top = (baseline as f32 - w.asc).round() as i32;
        let mut run = PlacedRun {
            words: vec![PlacedWord { x: w.x, top: block_top, block: w.block }],
            x: w.x,
            width: w.width,
            style: w.style.clone(),
        };
        let mut last_right = w.x + w.width;
        while let Some(n) = iter.peek() {
            if n.style == run.style {
                let n = iter.next().unwrap();
                let nt = (baseline as f32 - n.asc).round() as i32;
                run.words.push(PlacedWord { x: n.x, top: nt, block: n.block });
                last_right = n.x + n.width;
            } else {
                break;
            }
        }
        run.width = last_right - run.x;
        runs.push(run);
    }

    LaidLine { runs, top, baseline, height }
}

/// Resolve the font style and size for a span, lay it out, and return
/// `(block, width, ascent, descent)`. `descent` is negative.
fn layout_run_block(
    font_name: &str,
    base_style: FontStyle,
    base_px: f32,
    scale_f: f32,
    text: &str,
    style: &SpanStyle,
    trim: bool,
) -> Option<(FormattedTextBlock, f32, f32, f32)> {
    let fs = combine_style(base_style, style.bold, style.italic);
    let px = match style.size_abs {
        Some(dip) => dip * scale_f * style.size_scale,
        None => base_px * style.size_scale,
    }
    .max(1.0);
    // Fall back to Regular if the styled variant isn't available, so bold/italic
    // text degrades instead of vanishing.
    let font = match get_font_family(font_name, fs) {
        Some(f) => f,
        None => get_font_family(font_name, FontStyle::Regular)?,
    };
    let opts = if trim {
        TextOptions::new()
    } else {
        TextOptions::new().with_trim_each_line(false)
    };
    let block = font.layout_text(text, px, opts);
    let (asc, desc) = block
        .iter_lines()
        .next()
        .map(|l| (l.ascent(), l.descent()))
        .unwrap_or((px, -px * 0.25));
    let width = block.width();
    Some((block, width, asc, desc))
}

/// Width of a single space in the given style (cached). Laid out with trimming
/// disabled so the trailing space is not stripped.
fn measure_space(
    cache: &mut HashMap<(u8, u32), f32>,
    font_name: &str,
    base_style: FontStyle,
    base_px: f32,
    scale_f: f32,
    style: &SpanStyle,
) -> f32 {
    let fs = combine_style(base_style, style.bold, style.italic);
    let px = match style.size_abs {
        Some(dip) => dip * scale_f * style.size_scale,
        None => base_px * style.size_scale,
    }
    .max(1.0);
    let key = (fs as u8, px.to_bits());
    if let Some(w) = cache.get(&key) {
        return *w;
    }
    let w = layout_run_block(font_name, base_style, base_px, scale_f, " ", style, false)
        .map(|(_, w, _, _)| w)
        .unwrap_or(px * 0.25);
    cache.insert(key, w);
    w
}

fn combine_style(base: FontStyle, bold: bool, italic: bool) -> FontStyle {
    let bold = bold || matches!(base, FontStyle::Bold | FontStyle::BoldItalic);
    let italic = italic || matches!(base, FontStyle::Italic | FontStyle::BoldItalic);
    match (bold, italic) {
        (true, true) => FontStyle::BoldItalic,
        (true, false) => FontStyle::Bold,
        (false, true) => FontStyle::Italic,
        (false, false) => FontStyle::Regular,
    }
}

// ---------------------------------------------------------------------------
// HTML-subset parser (style stack)
// ---------------------------------------------------------------------------

fn parse_html(input: &str) -> (RichContent, bool) {
    let mut content = RichContent::default();
    let mut stack: Vec<(String, SpanStyle)> = Vec::new();
    let mut has_link = false;
    let mut prev_space = true; // suppress leading whitespace
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            let close = match find_byte(bytes, i + 1, b'>') {
                Some(c) => c,
                None => {
                    emit_text(&mut content, current_style(&stack), &input[i..], &mut prev_space);
                    break;
                }
            };
            let raw = input[i + 1..close].trim();
            i = close + 1;
            if raw.is_empty() {
                continue;
            }
            if let Some(rest) = raw.strip_prefix('/') {
                let tag = rest.trim().to_ascii_lowercase();
                if let Some(pos) = stack.iter().rposition(|(t, _)| *t == tag) {
                    stack.truncate(pos);
                }
            } else {
                let self_closing = raw.ends_with('/');
                let inner = if self_closing { raw[..raw.len() - 1].trim() } else { raw };
                let (name, attrs) = split_name_attrs(inner);
                let name = name.to_ascii_lowercase();
                if name == "br" {
                    content.push("\n", current_style(&stack));
                    prev_space = true;
                    continue;
                }
                let new_style = apply_tag(&current_style(&stack), &name, attrs, &mut has_link);
                if !self_closing {
                    stack.push((name, new_style));
                }
            }
        } else {
            let next = find_byte(bytes, i, b'<').unwrap_or(bytes.len());
            emit_text(&mut content, current_style(&stack), &input[i..next], &mut prev_space);
            i = next;
        }
    }

    content.trim_trailing_space();
    (content, has_link)
}

fn current_style(stack: &[(String, SpanStyle)]) -> SpanStyle {
    stack.last().map(|(_, s)| s.clone()).unwrap_or_default()
}

fn find_byte(bytes: &[u8], from: usize, target: u8) -> Option<usize> {
    (from..bytes.len()).find(|&j| bytes[j] == target)
}

fn split_name_attrs(inner: &str) -> (&str, &str) {
    match inner.find(|c: char| c.is_whitespace()) {
        Some(p) => (&inner[..p], inner[p..].trim_start()),
        None => (inner, ""),
    }
}

fn apply_tag(cur: &SpanStyle, name: &str, attrs: &str, has_link: &mut bool) -> SpanStyle {
    let mut s = cur.clone();
    match name {
        "b" | "strong" => s.bold = true,
        "i" | "em" | "cite" | "dfn" | "var" => s.italic = true,
        "u" | "ins" => s.underline = true,
        "s" | "strike" | "del" => s.strike = true,
        "mark" => {
            s.background = get_attr(attrs, "color")
                .and_then(|c| parse_color(&c))
                .or(Some(DEFAULT_MARK_COLOR));
        }
        "big" => s.size_scale *= BIG_FACTOR,
        "small" => s.size_scale *= SMALL_FACTOR,
        "font" => {
            if let Some(c) = get_attr(attrs, "color").and_then(|c| parse_color(&c)) {
                s.color = Some(c);
            }
            if let Some(sz) = get_attr(attrs, "size").and_then(|v| v.parse::<f32>().ok()) {
                s.size_abs = Some(sz);
            }
        }
        "span" => {
            if let Some(c) = get_attr(attrs, "color").and_then(|c| parse_color(&c)) {
                s.color = Some(c);
            }
            if let Some(bg) = get_attr(attrs, "background")
                .or_else(|| get_attr(attrs, "bg"))
                .and_then(|c| parse_color(&c))
            {
                s.background = Some(bg);
            }
        }
        "a" => {
            if let Some(href) = get_attr(attrs, "href") {
                s.href = Some(Rc::new(href));
                *has_link = true;
            }
        }
        _ => {}
    }
    s
}

/// Decode entities + collapse whitespace, appending to `content` as one section.
fn emit_text(content: &mut RichContent, style: SpanStyle, chunk: &str, prev_space: &mut bool) {
    let decoded = decode_entities(chunk);
    let mut out = String::with_capacity(decoded.len());
    for ch in decoded.chars() {
        if ch.is_ascii_whitespace() {
            if !*prev_space {
                out.push(' ');
                *prev_space = true;
            }
        } else {
            out.push(ch);
            *prev_space = false;
        }
    }
    content.push(&out, style);
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        if bytes[i] == b'&'
            && let Some(semi) = find_byte(bytes, i + 1, b';')
            && semi - i <= 10
        {
            let ent = &s[i + 1..semi];
            let decoded = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{00A0}'),
                _ => decode_numeric_entity(ent),
            };
            if let Some(c) = decoded {
                out.push(c);
                i = semi + 1;
                continue;
            }
        }
        // Not an entity — copy this char verbatim.
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn decode_numeric_entity(ent: &str) -> Option<char> {
    let num = ent.strip_prefix('#')?;
    let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        num.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

/// Read `key="value"` / `key='value'` / `key=value` from a tag's attribute
/// string. Returns the entity-decoded value.
fn get_attr(attrs: &str, key: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(key) {
        let idx = from + rel;
        let before_ok = idx == 0 || lower.as_bytes()[idx - 1].is_ascii_whitespace();
        let after = idx + key.len();
        let rest = attrs[after..].trim_start();
        if before_ok && let Some(eq) = rest.strip_prefix('=') {
            let val = eq.trim_start();
            let vbytes = val.as_bytes();
            if let Some(&q) = vbytes.first() {
                if q == b'"' || q == b'\'' {
                    if let Some(end) = val[1..].find(q as char) {
                        return Some(decode_entities(&val[1..1 + end]));
                    }
                } else {
                    let end = val.find(char::is_whitespace).unwrap_or(val.len());
                    return Some(decode_entities(&val[..end]));
                }
            }
        }
        from = after;
        if from >= lower.len() {
            break;
        }
    }
    None
}

/// Parse `#RRGGBB` / `#AARRGGBB` hex, or a few common colour names.
fn parse_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(c) = parse_hex_color(s) {
        return Some(c);
    }
    let named = match s.to_ascii_lowercase().as_str() {
        "black" => 0xFF000000,
        "white" => 0xFFFFFFFF,
        "red" => 0xFFFF0000,
        "green" => 0xFF008000,
        "lime" => 0xFF00FF00,
        "blue" => 0xFF0000FF,
        "yellow" => 0xFFFFFF00,
        "orange" => 0xFFFFA500,
        "gray" | "grey" => 0xFF808080,
        "silver" => 0xFFC0C0C0,
        _ => return None,
    };
    Some(named)
}

// ---------------------------------------------------------------------------
// View impl
// ---------------------------------------------------------------------------

impl View for RichText {
    fn set_any(&mut self, name: &str, value: &str) {
        if self.base_set_any(name, value) {
            // Geometry / colour attrs handled by the base may affect layout.
            if matches!(name, "width" | "height" | "padding" | "padding_left" | "padding_right"
                | "padding_top" | "padding_bottom")
            {
                self.invalidate();
            }
            return;
        }
        match name {
            "html" | "text" => self.set_html(value),
            "font" => {
                self.state.borrow_mut().main.font_manager.set_font(value);
                self.invalidate();
            }
            "font_style" => {
                self.state.borrow_mut().main.font_manager.set_font_style(value);
                self.invalidate();
            }
            "font_size" => {
                if let Ok(size) = value.parse::<f32>() {
                    self.state.borrow_mut().main.font_manager.set_font_size(size);
                    self.invalidate();
                }
            }
            "link_color" => {
                if let Some(c) = parse_hex_color(value) {
                    *self.link_color.borrow_mut() = c;
                }
            }
            &_ => {}
        }
    }

    fn set_parent(&self, parent: Option<WeakElement>) {
        self.base_set_parent(parent);
    }

    fn get_parent(&self) -> Option<Element> {
        self.base_get_parent()
    }

    fn layout_content(&mut self, x: i32, y: i32, width: i32, height: i32, typeface: &Typeface, scale: f64) -> Rect<i32> {
        self.base_set_scale(scale);
        let typeface = self.get_typeface(typeface);
        self.state.borrow_mut().main.font_manager.set(Some(typeface));
        // Keep focusable in sync with link presence (mouse routing gate).
        self.base_set_focusable(self.has_link.get());

        let padding = self.get_padding(scale);
        let horizontal = padding.left + padding.right;
        let vertical = padding.top + padding.bottom;
        let (new_width, new_height) = self.calculate_size(width - horizontal, height - vertical, scale);
        let wrap_w = new_width.max(0);
        self.ensure_laid(wrap_w, scale);

        let (content_width, content_height) = {
            let laid = self.laid.borrow();
            laid.as_ref().map(|l| (l.width, l.height)).unwrap_or((0, 0))
        };

        let (b_width, b_height) = self.get_bounds();
        let final_width = match b_width {
            Dimension::Min => content_width + horizontal,
            _ => new_width + horizontal,
        };
        let final_height = match b_height {
            Dimension::Min => content_height + vertical,
            _ => new_height + vertical,
        };
        let r = rect((x, y), (x + final_width, y + final_height));
        self.set_rect(r);
        r
    }

    fn fits_in_rect(&self, width: i32, height: i32, _scale: f64) -> bool {
        match &*self.laid.borrow() {
            Some(laid) => laid.width <= width && laid.height <= height,
            None => true,
        }
    }

    fn paint(&self, origin: Point<i32>, theme: &mut dyn Theme) {
        // Re-lay-out if content changed since the last `layout_content`. Compute
        // the wrap params first so the immutable `laid` borrow is dropped before
        // `ensure_laid` takes a mutable borrow.
        let pending = if self.laid.borrow().is_none() { self.last_wrap.get() } else { None };
        if let Some((w, s)) = pending {
            self.ensure_laid(w, s);
        }
        let state = self.state.borrow();
        let mut r = state.main.rect;
        r.move_by(origin);
        let scale = state.main.scale;
        theme.push_clip();
        theme.clip_rect(r);

        let padding = state.main.padding.scaled(scale);
        let ox = r.min.x + padding.left;
        let oy = r.min.y + padding.top;
        let line_w = ((1.0 * scale).round() as i32).max(1);
        let default_color = theme.get_text_color(state.main.state, state.main.foreground.as_ref());
        let link_color = *self.link_color.borrow();

        let laid = self.laid.borrow();
        if let Some(laid) = &*laid {
            for line in &laid.lines {
                // 1. Backgrounds (highlight) under the whole run.
                for run in &line.runs {
                    if let Some(bg) = run.style.background {
                        let rr = rect(
                            (ox + run.x, oy + line.top),
                            (ox + run.x + run.width, oy + line.top + line.height),
                        );
                        theme.draw_rect(rr, bg);
                    }
                }
                // 2. Text, word by word.
                for run in &line.runs {
                    let color = resolve_color(&run.style, link_color, default_color);
                    for w in &run.words {
                        theme.draw_text((ox + w.x) as f32, (oy + w.top) as f32, color, &w.block);
                    }
                }
                // 3. Underline / strikethrough across the run extent.
                for run in &line.runs {
                    let color = resolve_color(&run.style, link_color, default_color);
                    let underline = run.style.underline || run.style.href.is_some();
                    if underline {
                        let yt = oy + line.baseline + (2.0 * scale).round() as i32;
                        let ur = rect((ox + run.x, yt), (ox + run.x + run.width, yt + line_w));
                        theme.draw_rect(ur, color);
                    }
                    if run.style.strike {
                        let ascent = (line.baseline - line.top) as f32;
                        let ym = oy + line.baseline - (ascent * 0.33).round() as i32;
                        let sr = rect((ox + run.x, ym), (ox + run.x + run.width, ym + line_w));
                        theme.draw_rect(sr, color);
                    }
                }
            }
        }

        theme.pop_clip();
    }

    fn get_state(&self) -> Option<ViewState> {
        Some(self.state.borrow().main.state)
    }

    fn get_rect(&self) -> Rect<i32> {
        self.base_get_rect()
    }

    fn set_rect(&mut self, rect: Rect<i32>) {
        self.base_set_rect(rect);
    }

    fn get_padding(&self, scale: f64) -> Borders {
        self.base_get_padding(scale)
    }

    fn set_padding(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_padding(top, left, right, bottom);
        self.invalidate();
    }

    fn get_margin(&self, scale: f64) -> Borders {
        self.base_get_margin(scale)
    }

    fn set_margin(&self, top: i32, left: i32, right: i32, bottom: i32) {
        self.base_set_margin(top, left, right, bottom);
    }

    fn get_gravity(&self) -> Gravity {
        self.base_get_gravity()
    }

    fn set_gravity(&self, gravity: Gravity) {
        self.base_set_gravity(gravity);
    }

    fn get_bounds(&self) -> (Dimension, Dimension) {
        self.base_get_bounds()
    }

    fn get_content_size(&self) -> (i32, i32) {
        match &*self.laid.borrow() {
            Some(laid) => (laid.width, laid.height),
            None => (0, 0),
        }
    }

    fn is_break(&self) -> bool {
        self.base_is_break()
    }

    fn set_focusable(&self, focusable: bool) {
        self.base_set_focusable(focusable);
    }

    fn set_width(&mut self, width: Dimension) {
        self.base_set_width(width);
        self.invalidate();
    }

    fn set_height(&mut self, height: Dimension) {
        self.base_set_height(height);
        self.invalidate();
    }

    fn set_scale(&mut self, scale: f64) {
        self.base_set_scale(scale);
    }

    fn set_id(&mut self, id: &str) {
        self.base_set_id(id);
    }

    fn get_id(&self) -> String {
        self.base_get_id()
    }

    fn get_tooltip(&self) -> Option<String> {
        self.base_get_tooltip()
    }

    fn set_tooltip(&mut self, tooltip: Option<String>) {
        self.base_set_tooltip(tooltip);
    }

    fn get_background(&self) -> Option<u32> {
        self.base_get_background()
    }

    fn set_background(&mut self, color: Option<u32>) {
        self.base_set_background(color);
    }

    fn get_border_color(&self) -> Option<u32> {
        self.base_get_border_color()
    }

    fn set_border_color(&mut self, color: Option<u32>) {
        self.base_set_border_color(color);
    }

    fn is_enabled(&self) -> bool {
        self.base_is_enabled()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.base_set_enabled(enabled);
    }

    fn get_visibility(&self) -> Visibility {
        self.base_get_visibility()
    }

    fn set_visibility(&mut self, visibility: Visibility) {
        self.base_set_visibility(visibility);
    }

    fn wants_raw_content(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: EventType, func: Box<dyn FnMut(&mut UI, &dyn View) -> bool>) {
        self.state.borrow_mut().listeners.insert(event, func);
    }

    fn click(&self, ui: &mut UI) -> bool {
        if !self.base_is_enabled() {
            return false;
        }
        self.fire_click(ui)
    }

    fn on_mouse_move(&self, _ui: &mut UI, position: Vector2<i32>) -> bool {
        if !self.has_link.get() {
            return false;
        }
        let hit = self.link_at(position).is_some();
        let old = self.state.borrow().main.state.hovered;
        self.state.borrow_mut().main.state.hovered = hit;
        old != hit
    }

    fn on_mouse_button_down(&self, _ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !self.base_is_enabled() || !matches!(button, MouseButton::Left) {
            return false;
        }
        if let Some(href) = self.link_at(position) {
            *self.pressed_href.borrow_mut() = Some(href);
            self.state.borrow_mut().main.state.pressed = true;
            return true;
        }
        false
    }

    fn on_mouse_button_up(&self, ui: &mut UI, position: Vector2<i32>, button: MouseButton) -> bool {
        if !matches!(button, MouseButton::Left) {
            return false;
        }
        let pressed = self.pressed_href.borrow_mut().take();
        self.state.borrow_mut().main.state.pressed = false;
        if let Some(href) = pressed
            && let Some(over) = self.link_at(position)
            && over == href
        {
            *self.clicked_href.borrow_mut() = Some((*href).clone());
            self.fire_click(ui);
            return true;
        }
        false
    }
}

fn resolve_color(style: &SpanStyle, link_color: u32, default_color: u32) -> u32 {
    if let Some(c) = style.color {
        c
    } else if style.href.is_some() {
        link_color
    } else {
        default_color
    }
}

impl Default for RichText {
    fn default() -> Self {
        let r = rect((0, 0), (200, 40));
        RichText::new(r, 18_f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_single_section() {
        let (c, has_link) = parse_html("Hello world");
        assert_eq!(c.text, "Hello world");
        assert_eq!(c.sections.len(), 1);
        assert!(!has_link);
        assert!(!c.sections[0].style.bold);
    }

    #[test]
    fn bold_inside_regular() {
        let (c, _) = parse_html("Hello <b>world</b>!");
        assert_eq!(c.text, "Hello world!");
        // "Hello ", "world", "!"
        assert_eq!(c.sections.len(), 3);
        assert!(!c.sections[0].style.bold);
        assert!(c.sections[1].style.bold);
        assert!(!c.sections[2].style.bold);
    }

    #[test]
    fn nested_styles_combine() {
        let (c, _) = parse_html("<b>bold <i>both</i></b>");
        assert_eq!(c.text, "bold both");
        let both = c.sections.iter().find(|s| &c.text[s.range.clone()] == "both").unwrap();
        assert!(both.style.bold && both.style.italic);
    }

    #[test]
    fn whitespace_is_collapsed_and_trimmed() {
        let (c, _) = parse_html("  Hello   \n  world  ");
        assert_eq!(c.text, "Hello world");
    }

    #[test]
    fn br_becomes_newline() {
        let (c, _) = parse_html("a<br/>b");
        assert_eq!(c.text, "a\nb");
    }

    #[test]
    fn entities_are_decoded() {
        let (c, _) = parse_html("a &amp; b &lt;tag&gt; &#65;");
        assert_eq!(c.text, "a & b <tag> A");
    }

    #[test]
    fn link_sets_href_and_flag() {
        let (c, has_link) = parse_html(r#"see <a href="https://x.example">here</a>"#);
        assert!(has_link);
        let link = c.sections.iter().find(|s| &c.text[s.range.clone()] == "here").unwrap();
        assert_eq!(link.style.href.as_deref().map(String::as_str), Some("https://x.example"));
    }

    #[test]
    fn font_color_and_size() {
        let (c, _) = parse_html(r##"<font color="#FF0000" size="24">red</font>"##);
        let red = &c.sections[0];
        assert_eq!(red.style.color, Some(0xFFFF0000));
        assert_eq!(red.style.size_abs, Some(24.0));
    }

    #[test]
    fn mismatched_close_is_tolerated() {
        // Stray </i> with no open italic should not panic or corrupt later spans.
        let (c, _) = parse_html("a</i><b>b</b>");
        assert_eq!(c.text, "ab");
        let b = c.sections.iter().find(|s| &c.text[s.range.clone()] == "b").unwrap();
        assert!(b.style.bold);
    }
}
