# Theme & Styling System Redesign

Design doc for ROADMAP.md Tier 1, item 1. Status: **proposed**.

## Goal

A theme becomes a **resource bundle** (drawables + colors + metrics + typography)
instead of a code module. Creating a new theme means writing XML and a palette,
not implementing ~30 Rust drawing methods. Dark mode becomes a palette swap.

## Current state and the tension in it

Two parallel rendering paths exist today:

1. **~30 hardcoded per-widget methods** on the `Theme` trait
   (`draw_button_back`, `draw_tab_active`, `draw_scrollbar_thumb`, …),
   implemented imperatively in `Classic`. The trait itself marks them
   *"Legacy drawing methods (will be deprecated)"* (`src/themes/mod.rs`).
2. **A data-driven drawable system** — Android-style XML with `<selector>`
   state matching, `<layer-list>`, shapes, strokes, gradients, dash patterns —
   loaded into `DrawableRegistry` (`src/drawing/registry.rs`) and rendered via
   `Theme::draw_component()`. Button, CheckBox, Edit, ComboBox, Dialog and
   ImageButton already paint through it.

The drawable path is crippled in two ways:

- **Views hardcode theme-specific drawable names** —
  `theme.draw_component("button_classic_back", …)` in `src/views/button.rs`.
  The view knows it is being drawn by the Classic theme.
- **Drawable XMLs hardcode hex colors** — `#d4d0c8` is repeated in every file.

Consequence: adding a second theme today requires new drawable XMLs **and** a
new Rust `Theme` impl **and** touching every view. The refactor removes all
three requirements.

Additionally, several views bypass the theme entirely with rogue hardcoded
colors that would break in dark mode: selection blue `0xff000080`, placeholder
gray `0xff808080`, tooltip yellow `0xFFFFFFDD`, TableView selection blue.

## Target architecture

A theme resolves four kinds of resources. The `Theme` trait shrinks to
**primitive drawing + resource lookup**:

```
trait Theme {
    // Primitives (stay in Rust):
    // draw_rect, draw_rounded_rect, draw_text*, draw_image*, clip stack, opacity stack

    // Resource resolvers (new):
    fn drawable(&self, role: &str) -> Option<&Drawable>;   // "button.back" -> theme's drawable
    fn color(&self, token: &str) -> u32;                   // "@selection" -> palette entry
    fn dimension(&self, token: &str) -> i32;               // "scrollbar.width" -> dips
    fn typeface(&self, role: TextRole) -> &Typeface;       // body/button/caption/heading
}
```

(Exact signatures TBD during implementation; the shape is what matters.)

### 1. Drawables (skins)

Finish the migration the code already started:

- Every widget visual goes through the drawable path. Views request **role
  names** (`"button.back"`, `"tab.active"`, `"scrollbar.thumb"`), the theme
  maps role → its own drawable. Views never see theme-specific names.
- Port the remaining legacy draw methods (~20: radiobutton, list, panel, tabs,
  separator, progressbar, scrollbars, combobox arrow, checkbox checkmark) to
  drawable XML and delete them from the trait.
- New theme = new set of drawable XML files, zero Rust.

### 2. Colors (palette)

- A `Palette` struct / named-token table: `background`, `surface`, `text`,
  `text_hint`, `accent`, `border_light`, `border_dark`, `selection`, `error`, …
- Drawable XMLs reference tokens: `<solid color="@surface"/>`,
  `<stroke color="@border_dark"/>` — resolved against the active palette at
  draw time. Literal hex stays allowed.
- **Dark mode = palette swap**, same drawable set. A derived theme (dark
  Classic) is just a palette override.
- Pull the rogue per-view hardcoded colors into tokens (`@selection`,
  `@text_hint`, `@tooltip_back`, …).
- Runtime switching: `ui.set_theme()` + full relayout/redraw.

### 3. Metrics (dimensions)

Numbers scattered through views are theme decisions too:

- scrollbar width (14), `DEFAULT_LEFT_INSET` (4), caret width, checkbox box
  size, default paddings / control heights, corner radii, focus-ring inset.
- Expose as dimension tokens: `theme.dimension("scrollbar.width")`. A flat
  theme wants 8 px scrollbars and 6 px corner radii; Classic wants 14 and 0.

### 4. Typography

- `Typeface::default()` hardcodes NotoSans; `Theme::typeface()` is a static
  method (`where Self: Sized` — not even dyn-callable).
- Make it instance-level and role-based: default / button / caption / heading
  fonts and sizes per theme.

### 5. User-facing styling in layout XML

Once roles/tokens exist, expose them to app developers:

- `style="primary_button"` attribute that expands to a named bundle of
  attributes / drawable overrides.
- `@` references in layout attributes: `background="@drawable/card"`,
  `text_color="@accent"`.
- The `Selector<DrawState>` infrastructure in `src/styles/` is already the
  right shape for per-state styling — it is just not wired to the XML parser.

## Cautions

- **Performance.** Drawable rendering walks a parsed command tree per draw,
  per frame. Fine at current scale, but TableView/RecyclerView paint hundreds
  of components per frame. Consider caching the resolved selector →
  layer-list per state, and benchmark the table example before/after.
- **Don't data-drive everything.** Caret positioning, text layout, scrollbar
  geometry math, sort arrows — the *logic* stays in Rust; only the *visuals*
  (fills, borders, indicators) move to drawables. The line the existing code
  drew (back / body / indicator as separate drawables) is the right one.
- **Scale.** Stroke widths and insets in drawable XML are dips; the engine
  must multiply by scale consistently (watch for the same class of HiDPI bugs
  fixed in layout earlier).

## Phasing

1. **Trait diet** — add the four resolvers; introduce the view → role-name
   indirection (views stop naming `*_classic` drawables).
2. **Color references** — `@token` support in drawable XML + extract the
   Classic palette; pull rogue view colors into tokens.
3. **Port legacy draw methods** to drawables, delete them from the trait,
   widget by widget (each is independently verifiable on screen).
4. **Dark palette** as the proof that the split works end to end.
5. **Metrics + typography tokens.**
6. **`style=` and `@` references in layout XML.**

Each phase keeps the app compiling and rendering; verify visually per phase
(run the examples and look — `cargo build` passing does not prove rendering).
