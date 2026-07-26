# Changelog

All notable changes to Lumio are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **`EventType::KeyChar`** with `EventData::Char { ch, modifiers }` — the
  character a keystroke produced *after* the keyboard layout, dead keys and IME
  were applied, which `KeyDown`'s virtual key code cannot carry. Dispatched from
  `UI::on_key_char` to the focused view's listener before built-in handling,
  mirroring what `KeyDown` already did, so an app can take the raw character
  stream of a focused view (a `TermGrid`, say) whatever the user's layout is.
- **`EventType::MouseWheel`** with `EventData::Wheel { x, y, distance }` — the
  pointer in absolute window coordinates plus the raw `MouseScrollDistance`,
  passed through unconverted so a listener can tell a line-wise wheel from a
  pixel-wise touchpad.
- **`EventData::Mouse { x, y, button }`** — the payload for `MouseDown` and
  `MouseUp`: `Position` alone could not say which button it was.
- **`TermGrid` pointer and wheel events** — the grid now fires `MouseDown`,
  `MouseUp`, `MouseMove` and `MouseWheel`. `MouseMove`/`MouseUp` are deliberately
  not gated on a hit test, so a selection drag that starts in the grid keeps
  reporting after the pointer leaves it.
- **`TermGrid::cell_at(point)`** — turns an absolute window point (exactly what
  those event payloads carry) into a `(col, row)`, or `None` outside the grid.
- **`TabView` close buttons** — `closable="true"` (or `TabView::set_closable`)
  draws a close button on every tab and fires the new **`EventType::TabClose`**
  with the tab index. The TabView does not remove the tab itself, so an app can
  confirm first.
- **`TabView` strip panning** — the tab strip scrolls horizontally under the
  mouse wheel when the tabs overflow the widget, leaving the active tab alone
  (switching stays a click). Changing the active tab from the keyboard or the
  app pans the strip to bring it into view.
- **`ProgressCircle`** (`src/views/progresscircle.rs`) — a round progress
  indicator, registered as `<ProgressCircle>`. Determinate mode draws a track
  ring plus a round-capped arc sweeping clockwise from 12 o'clock, and eases
  toward a newly set value instead of jumping; indeterminate mode
  (`indeterminate="true"`) orbits a comet of ten dots that shrink and fade
  along the trail, spaced edge to edge (not centre to centre) so the gaps stay
  even as the dots get smaller, with a slow size pulse. Both animations are
  driven off a wall clock, so they run at the same speed whatever the tick rate.
  Attributes: `value`, `indeterminate`, `show_value` (centre percentage, off by
  default), `thickness` (dips), `track_color`, `fill_color`. Demoed in
  `examples/progress_circle_example.rs`.
- **`Renderer::draw_circle`** and **`Renderer::draw_arc`** — float-precision
  circle and round-capped arc primitives (physical pixels; `0` rad points right,
  a positive sweep runs clockwise). `RendererGL` builds the arc as a quad strip
  with disc caps, `RendererSoftware` strokes a tiny-skia path; the trait's
  defaults approximate both from existing primitives, so custom `Renderer`
  implementations keep compiling.
- **Palette dimensions** `progress_circle.size` (32 dip) and
  `progress_circle.thickness` (3 dip).

### Added

- **`UI::on_key_repeat`** — a held key's auto-repeat, which the window loop used
  to drop entirely. Only the focused view's `KeyDown` *listener* sees it: built-in
  widget behaviour still ignores repeats, so a held Enter does not re-click a
  button and a held Tab does not race through the focus order, but an app that
  owns the raw key stream now gets them. Without this a terminal could not hold
  Backspace down to erase a line, since `KeyChar` (which did repeat) carries no
  control keys.

### Changed

- **`TermGrid` caches shaped text per row.** Shaping ran for every colour run of
  every row on every paint, though a terminal repaints far more often than its
  rows change. Rows are now reshaped only when their cells, the cursor or the
  font metrics move — so a keystroke, which changes a single row, no longer
  reshapes the whole screen.

### Fixed

- **A key held while a window closed was typed into the window behind it.**
  winit re-announces the keyboard state to whichever window gains focus, as
  `is_synthetic` key events; the loop took those for real presses. Dismissing a
  dialog with Enter therefore delivered that Enter to the parent window as well —
  visible in a terminal app as a stray newline in the shell that had just
  started. Synthetic events are now ignored: they describe state at a focus
  change, not something the user did, and modifier state already arrives through
  `ModifiersChanged`.

- **`TermGrid` glyphs sat too high in their cells** — cells are at least 1.2em
  tall, and the leftover leading was left below the text, so an inverse row
  (top's header, the cursor block) looked shifted down relative to its own
  glyphs. The baseline now sits so descenders just reach the cell floor and the
  leading goes above: splitting it evenly still looks top-heavy, because the
  descent below the baseline is already empty for the capitals and digits that
  are most of what a terminal prints. The underline follows the baseline instead
  of the cell bottom.
- **`TermGrid` drew text outside its own cells** — painting resolved the
  typeface against the theme's default while layout resolved it against the
  parent in the view tree. A view with no font size of its own inherits one from
  whatever it is resolved against, so the two disagreed: glyphs advanced by more
  than a cell and crept right, a fraction of a cell per column. The cursor block,
  drawn at the correct cell, ended up further behind the text the longer the line
  got. Painting now reuses the typeface the metrics were measured with.
- **`TermGrid` cell width came from the ink extent, not the layout step** — on
  the software backend `TextBlock::width()` and the reported `advance_width`
  both disagree with the positions the shaper actually produces. The cell is now
  the measured distance between two shaped glyphs, which is what the renderer
  draws with.
- **`TermGrid` rows were an em tall instead of a line** — the pitch came from
  `TextBlock::height()`, which the text layer normalises to the em size, so the
  tails of g, j and p were clipped into the row below. Cells are now measured
  from a probe string with both extremes (the software backend reports the
  extents of the glyphs laid out, not the font's metrics, so a lone "W" comes
  back with no descender at all) and never come out tighter than 1.2em.
- **`TermGrid` defaulted to the theme's light field colour** under its light
  default text, which is invisible, and left the theme colour showing in the
  strip where the widget is not an exact multiple of the cell size. The default
  background is now dark, to agree with the default foreground.
- **`TermGrid` used a font that does not exist on Windows** — the default was
  `Noto Sans Mono`, so on a machine without it the widget had no cell metrics
  and silently drew nothing. It now follows the new
  **`default_mono_font_name()`** (Consolas / Menlo / `monospace`), and a missing
  family is logged instead of leaving a blank widget.
- **`TermGrid` crashed on its first paint** — it asked the palette for
  `edit.back`, which is a *drawable* role (what `Edit` and `CheckBox` draw with
  `draw_component`), not a colour token. `Palette::color` guards unknown tokens
  with a `debug_assert`, so painting a terminal aborted any debug build. It now
  draws the themed field background, and a headless render test covers it.
- **`Container::remove_view` stranded whole subtrees** — the default returned
  `false` without looking at the container's children, so a removal could not
  pass *through* a container that did not override it. A `TabView` inside a
  `SplitPanel` could never have a tab removed, because `SplitPanel` does not
  implement the method. The default now forwards to nested containers.
- **`TabView::remove_view`** — `TabView` never implemented it, so the `Container`
  default applied and a tab could not be removed at all (neither directly nor via
  `UI::remove_view`). It now drops the child and its tab together and keeps the
  active index pointing at the same tab where it can, falling back to the new
  last tab when the active one goes.

## [0.5.3] - 2026-07-24

**XML parser hardening** — `UI::from_xml` can no longer panic on malformed
layout XML, so untrusted (e.g. guest-supplied) layouts can't crash the host:
it now logs a warning and returns `None` for bad attribute syntax
(`<X attr>` without `=`), unknown view-type tags, reader errors such as
mismatched end tags, unbalanced or unclosed tags, and a child element inside
a non-container parent (the child is dropped).

### Added

- **`UI::try_create(name)`** — like `UI::create`, but returns `None` for an
  unregistered view type instead of panicking.

### Fixed

- **Self-closing root element.** A layout consisting of a single self-closing
  tag (`<Frame .../>`) now parses as the root view instead of panicking.

## [0.5.0] - 2026-07-19

**Skins** — a theme is now a swappable *resource bundle* (a palette **plus** the
drawable *forms*), not just a palette recolor. Build a skin, register it, and
select it per-window or swap it live; a custom skin overrides only the roles it
changes, layering over a shared base set. Demoed in `examples/skins.rs`.

### Added

- **`Skin`** (`src/skin.rs`) — bundles a `Palette` with the drawable form set
  (`DrawableRegistry`). Two built-ins, `Skin::light()` and `Skin::dark()`, share
  the classic forms and differ only in palette. Exported as `lumio::Skin`.
- **`Skin::builder`** / **`BuiltinSkin`** — build a custom skin over a built-in
  base (`.base(BuiltinSkin::Dark)`), overriding individual drawable roles with
  XML (`.drawable("button.back", xml)`), then `.build()`. Roles left unset fall
  back to the base form, so a skin only carries what it changes.
- **Palette token overrides.** `Palette::with_color` / `with_dimension` /
  `with_typeface` derive a palette from a built-in one; `SkinBuilder::color` /
  `dimension` / `typeface` layer single tokens on top of the base, so a skin can
  tweak (say) just `selection` without replacing the whole palette.
- **Per-role drawable overrides.** `DrawableRegistry` gained a base + override
  model: overridden roles resolve locally, everything else against a shared base
  (so "dark mode" is still one form set, recolored).
- **9-patch role drawables.** A skin can override a role with a 9-patch instead
  of a shape drawable — `.drawable("button.back", "button.9.png")`, or a
  `<selector>` `.xml` referencing per-state `.9.png`s — falling back to the
  shape/base form otherwise.
- **Skin manifest XML.** `Skin::from_xml` builds a whole skin from one `<skin>`
  document — an optional `base`, `<color>` / `<dimension>` / `<typeface>` token
  overrides, and `<drawable>` role overrides (a 9-patch `src`, or inline shape
  `<selector>` XML). Sugar over `Skin::builder`; `examples/skins.rs` uses it.
- **Named skin registry.** `register_skin(skin)` makes a skin selectable by name
  alongside the built-in `"light"` / `"dark"`; `skin_by_name` resolves them.
- **`WindowConfig::skin(name)`** — choose a window's skin by name; falls back to
  `.palette(..)` when unset or unknown.
- **`UI::set_skin(name)`** — swap a window's skin at runtime from an event
  handler (applied before the next paint). `set_palette` remains for
  palette-only recolors.
- **`Skin`, `SkinBuilder`, `BuiltinSkin`, `register_skin`** re-exported from the
  prelude; **`examples/skins.rs`** cycles light → dark → a custom "flat" skin.

### Changed

- **Renderer trait rename (breaking).** The rendering-abstraction trait `Theme`
  is now **`Renderer`**, and its two implementations `Classic` / `SoftwareTheme`
  are now **`RendererGL`** / **`RendererSoftware`** — matching the trait name and
  freeing "theme" for the palette/skin layer. Prelude re-exports updated
  (`Renderer`, `RendererGL`); custom `View` impls change `&mut dyn Theme` to
  `&mut dyn Renderer`.

## [0.4.0] - 2026-07-13

Android-style **9-patch (`.9.png`) backgrounds** for every widget: PNGs with
1px marker borders (top/left = stretchable regions, right/bottom = content
padding) stretch to any size with crisp corners, per-state skins included.
Demoed in `examples/ninepatch_example.rs`.

### Added

- **9-patch backgrounds** (`src/ninepatch.rs`) via the universal `background`
  attribute, detected by suffix alongside the existing `@token`/`#hex` forms:
  - `background="panel.9.png"` — one patch for all states.
  - `background="fancy_button.xml"` — an Android-style `<selector>` whose
    `<item state_pressed="true" src="button_pressed.9.png"/>` items are
    matched against the live view state at paint time.

  Rendering is CPU-composited: the patch is stretched to the destination size
  once (fixed regions scale with HiDPI, multiple stretch runs per axis are
  distributed proportionally, cells are seam-free by construction), cached at
  the last size, and drawn through the existing raw-image path — identical
  results on both backends, one texture upload per size on GL.
- **Patch content padding**: the right/bottom markers become the view's
  effective padding unless the layout sets an explicit `padding`/`padding_*`
  attribute (which always wins). Text wrapping, hit-testing and child layout
  all honor it.
- **All widgets are wired.** Views with their own full-rect chrome
  (Edit, Memo, ComboBox, Button, ImageButton, List, RecyclerView, TableView,
  TreeView, IconList, ProgressBar, PopupMenu, TabView content, MenuBar,
  Frame/Grid/StatusBar/SplitPanel) *replace* that chrome with the patch;
  content-only views (Label, RichText, ImageView, CheckBox, RadioButton,
  ScrollView, Slider) draw it *behind* their content. Sub-elements
  (scrollbars, headers, dropdown arrow buttons, check/radio indicators)
  stay theme-drawable-based.
- **`NinePatchSource` / `NinePatchBackground`** exported from the prelude for
  programmatic use.
- **`examples/ninepatch_example.rs`** — panels, per-state buttons, and a
  widget gallery on 9-patches; assets generated by the kept-in-repo
  `examples/gen_ninepatch_assets.rs`.
- Headless coverage: exact-pixel stretch/corner tests plus a
  `LUMIO_RENDER_DUMP`-driven visual dump test (`tests/ninepatch_render.rs`).

## [0.3.1] - 2026-07-13

### Changed

- Logging reworked onto the `log` crate facade (apps pick their own logger);
  internal prints became proper `log` records.

## [0.3.0] - 2026-07-13

New widgets: TreeView (lazy, app-managed hierarchy) and IconList (Explorer
"List"-mode multi-select item view), demoed together in `examples/explorer.rs`.

**Breaking:** `EventType` gained the `Expanded` and `Collapsed` variants —
exhaustive `match`es on `EventType` need new arms.

### Added

- **TreeView** — hierarchical tree widget with app-managed nodes: set an
  initial tree with `set_roots`, react to the new `EventType::Expanded` /
  `EventType::Collapsed` events (node via `expanded_key()`) and grow branches
  lazily with `set_children(key, ..)` — nodes created with
  `has_children: true` show a chevron before any children are loaded, and the
  chevron clears itself when an expand yields nothing. Single selection
  (fires `SelectionChanged`, read via `selected_key()`), full keyboard
  navigation (arrows expand/collapse/jump-to-parent, Home/End/PageUp/Down),
  per-node icons with optional tint, vertical scrollbar, `Role::Tree`
  accessibility. XML attrs: `row_height`, `icon_size`, `indent`, `font_size`.
- **IconList** — Windows-Explorer-"List"-mode item view: small icon + text
  per item, items flow top-to-bottom wrapping into uniform-width columns,
  horizontal scrollbar (the mouse wheel scrolls horizontally). Multi-select
  with Explorer semantics: click, Ctrl+Click toggle, Shift+Click range,
  Ctrl+Shift+Click range-add, arrow keys with Shift-extend. Fires
  `SelectionChanged`; `selected_indices()` / `last_selected()` /
  `item_at(x, y)` (pairs with a `DoubleClick` listener for open/navigate).
  XML attrs: `row_height`, `icon_size`, `item_width`, `font_size`.
- **`UI::modifiers()`** — the last known keyboard-modifier state, kept
  current by the window loop, so mouse handlers can implement
  Ctrl/Shift+Click behavior (mouse events carry no modifiers). Settable via
  `UI::set_modifiers` for synthetic dispatch in tests.
- **`examples/explorer.rs`** — file-manager demo wiring both new widgets in
  a `SplitPanel`: lazily-loaded directory tree on the left, directory
  contents (folders first) on the right, double-click to navigate.

## [0.2.1] - 2026-07-13

Keyboard navigation: Tab/Shift+Tab focus traversal and keyboard activation
for all interactive widgets.

### Fixed

- **The Space key never reached views** — the winit→Lumio key table had no
  mapping for the spacebar, so `on_key_down`/`on_key_up` fired with `None`.
  Space now maps to `VirtualKeyCode::Space` (both backends share the table).
- **Tab between two views sharing the same `id` is now consumed** (and
  redraws): the focus-change diff compares ids, so the move used to read as
  "no change" and the window kept painting the old focus.

### Added

- **Keyboard navigation.** Tab / Shift+Tab move focus across the whole view
  tree in document order (wrapping, skipping disabled/invisible views and
  views on inactive `TabView` tabs; confined to a modal overlay while one is
  open). New `UI::focus_next_view()` / `UI::focus_prev_view()`. Focused
  widgets now respond to the keyboard: `Button`/`ImageButton` activate on
  Space/Enter, `CheckBox`/`RadioButton` toggle/select on Space (the checkbox
  draws a focus outline around its label), `ComboBox` opens on
  Space/Enter/Alt+Down and its dropdown is navigable with the arrow keys +
  Enter. The `TabView` tab strip is a focus stop: Left/Right switch tabs
  (firing `SelectionChanged`), with a focus outline on the active tab; a
  strip-focused TabView no longer forwards keys to hidden tab content. New
  `Theme::draw_rect_outline` helper (default impl on both backends).

## [0.2.0] - 2026-07-12

Screen-reader accessibility via [AccessKit](https://accesskit.dev): every
window now exposes a full accessibility tree to the platform API (UIA /
NSAccessibility / AT-SPI), screen readers can read and operate all widgets,
and text fields support caret/selection-level access. Zero overhead until an
assistive technology connects.

### Fixed

- **Memo selection/caret geometry after a scale change.** `Memo` cached its
  line height at the first scale it saw and never invalidated it, so after a
  re-layout at a different HiDPI scale the selection highlight, caret rect
  and click-to-line hit-testing used a wrong (e.g. half-size) line height.
  The cache is now reset on every text re-layout.

### Changed

- **ProgressBar with no explicit height now sizes to its intrinsic 16-dip
  bar height** instead of stretching to all the vertical space its parent
  offers (`Dimension::Min` heights currently resolve to the full available
  height in the generic path; ProgressBar now overrides that). Give it an
  explicit `height=".."` if you relied on the stretch.

### Added

- **Accessibility: depth** (fourth slice — completes the AccessKit
  integration). `Edit`/`Memo` expose full text semantics: per-line `TextRun`
  nodes with per-character geometry and word boundaries, plus live
  caret/selection reporting — screen readers echo typed characters and
  navigate by character/word/line, and UIA TextPattern works (password fields
  still expose nothing). `TableView` publishes real table semantics: a header
  row of `ColumnHeader`s with the live sort direction (AT click on a header
  sorts), and one `Row` per data row grouping its cell views, with selection.
  `RecyclerView` exposes its realized rows (plus the total item count) and
  `NotificationStack` items appear in a polite live region, as do `StatusBar`
  section texts. New universal XML attribute `labelled_by="view_id"` names a
  control by another view's text (like `<label for=..>`). New `View` hooks
  for custom widgets: `accessibility_children()` (synthetic per-item nodes,
  which may group other nodes) and `accessibility_child_elements()` (expose
  child views owned outside the `Container` protocol).
- **Accessibility: assistive-technology actions** (third slice). Screen
  readers can now operate the UI, not just read it: activating a control
  (UIA Invoke / SelectionItem.Select) delivers a synthetic click through the
  ordinary mouse dispatch — including synthetic items like list rows, tabs
  and menu items; AT focus requests move keyboard focus; and
  RangeValue.SetValue / Increment / Decrement drive a `Slider`, firing
  `ValueChanged` exactly like the keyboard path (new `Slider::nudge`).
  New public API: `UI::set_focus_to(&Element)` — the programmatic-focus
  primitive (clears focus tree-wide, focuses the target, fires
  `FocusLost`/`FocusGained`), also the building block for future
  Tab-navigation.
- **Accessibility: full widget coverage + `content_description`** (second
  slice). All remaining widgets now describe themselves to screen readers:
  RadioButton, Memo, ComboBox (with expanded state), ProgressBar, RichText,
  ScrollView, TableView (role + dimensions), RecyclerView, MenuBar, and open
  PopupMenus; List and TabView expose their rows/tabs as synthetic child
  nodes with per-item bounds and selection (new `View::accessibility_children`
  hook), a hovered menu item is reported as the AT focus, and decorative views
  (Separator, undescribed ImageView) opt out of the tree. New universal XML
  attribute `content_description` (Android-style) overrides any widget-derived
  accessible name — use it on `ImageButton`/`ImageView`. New getters:
  `RadioButton::get_text`, `Memo::is_read_only`, `ComboBox::is_open`,
  `List::{get_selected, item_count, item_text}`,
  `RecyclerView::get_selected_position`, `TabView::get_tab_title`,
  `MenuBar::menu_titles`, `RichText::get_plain_text`.
- **Screen-reader accessibility via AccessKit** (first slice). Every window now
  exposes an accessibility tree to the platform API (UIA on Windows,
  NSAccessibility on macOS, AT-SPI on Linux): a per-window
  `accesskit_winit::Adapter` in the shared winit loop, a tree builder that
  mirrors the visible view hierarchy (`lumio::accessibility`), and a new
  defaulted `View::accessibility_node()` for widgets to describe themselves.
  Label, Button, ImageButton, CheckBox, Edit (incl. protected password fields)
  and Slider report role/name/state; focus changes are mirrored to assistive
  tech. Zero overhead until a screen reader connects. New getters:
  `Label/Button/CheckBox::get_text`, `Edit::is_read_only`,
  `Slider::get_min/get_max/get_step`.

## [0.1.1] - 2026-07-12

### Changed

- **ComboBox dropdown border** is now a plain 1px solid outline (new `popup.body`
  drawable, palette `@outline`) instead of the sunken edit-field bevel.

### Added

- **Runtime GL → software fallback.** Enabling both backend features in one
  binary makes the runtime try GL first and automatically fall back to software
  rendering when GL initialization fails (VMs / emulated framebuffers).
  `LUMIO_BACKEND=gl|software` forces a backend; `lumio::active_backend()`
  reports the one in use.

### Removed

- The `TextShaper` trait (public in the `text` module, unused) — shaping is now
  dispatched per `FontHandle`, following the backend each font was loaded for.

## [0.1.0] - 2026-07-11

First crates.io release (as `lumio-gui`; the library is still imported as
`lumio`). This entry captures the 2026 development cycle, which turned an early
retained-mode prototype into a switchable-backend desktop GUI toolkit. The
pre-2026 foundation — the retained view tree, XML layout parsing, the initial
widget set, and the original Win95-style `Classic` theme (then built directly
on speedy2d) — predates this log and is treated as the starting point.

### Added

- **Two rendering backends, switchable by Cargo feature.** `backend-gl`
  (default) draws with OpenGL; `backend-software` renders on the CPU with
  tiny-skia + fontdue. Apps launch backend-agnostically and switch with a
  feature flag, no source changes.
- **Backend-neutral launcher** — `lumio::run(ui, WindowConfig::new(..))` plus a
  `WindowConfig` builder (center, visibility, logical size, window-style
  toggles) that supersedes the old backend-specific entry points.
- **Headless software rendering** — `render::render_to_pixmap(..)` lays out and
  paints a UI into a `tiny_skia::Pixmap` (and on to PNG) with no window, enabling
  pixel-snapshot tests and screenshots.
- **Multi-window and app-modal dialogs** — `UI::open_window` / `UI::close_window`,
  an app-modal window stack, and a `Dialog` builder with `UI::show_message` /
  `show_confirm` / `show_input` (auto-sized modal child windows, Enter/Esc wired).
- **Theming and styling system** — themes become resource bundles (drawables +
  color palette + dimensions + typography). Dark mode is a runtime palette swap
  (`ui.set_palette(Palette::dark())`); layout XML gains `@token` palette
  references and reusable `style=` attribute bundles.
- **New widgets** — `MenuBar` (with submenus and keyboard navigation, shared with
  `PopupMenu`), `RichText` (spannable HTML-subset rich text with clickable
  links), `TableView` (sticky header, sort, V/H scroll, drag-resize columns),
  `Grid` (lightweight non-scrolling 2D layout), and `NotificationStack`
  (click-through, animated toasts).
- **Pluggable layout engines** behind a `Layout` trait — `LinearLayout` (default,
  with per-child `weight`), `OverlayLayout`, and `DockLayout`, selectable via the
  `layout` attribute.
- **Event system** — centralized listeners on every view, an `EventData` payload,
  and `Focus`/`Hover`/`DoubleClick`/`KeyDown`/`ContextMenu` events. Keyboard
  accelerators via `ui.add_shortcut("Ctrl+S", ..)`.
- **Edit/Memo maturity** — undo/redo with run coalescing, password masking
  (`password="true"`), and per-character input filters (`filter="numeric"`,
  `allowed_chars="..."`).
- **Text selection** — mouse-driven selection in `Edit`/`Memo`, plus opt-in
  read-only selection on `Label`/`RichText` (`selectable="true"`).
- **Label and Edit polish** — hyperlink labels (`link="true"`), chip composition
  (`background_color` / `corner_radius` / left & right icons), and Edit
  left/right icons with tint, error underline, and icon-click events.
- **Frame background images** — `background_image` with cover/contain, tiling,
  position, and opacity.
- **Mouse cursor switching** — hand cursor over links, I-beam over editable text.
- **Windows tray-icon facility.**
- **Window-style toggles** — `resizable` / `minimizable` / `maximizable` on
  `WindowConfig` and `WindowRequest` (dialogs fixed by default).
- **Rust 2024 edition.**

### Changed

- **Unified window loop.** Both backends now run on one Lumio-owned winit
  `ApplicationHandler`; the per-window paint sits behind a `RenderSurface` trait
  (GL vs. software). speedy2d is demoted to a pure GL renderer (its `windowing`
  feature off) over a glutin context Lumio creates; the old `win.rs` is gone.
- **Backend-neutral abstractions.** Text shaping moved behind `crate::text`,
  input and events behind `crate::input` — the renderer is the only
  backend-specific seam.
- **speedy2d is now an optional, renderer-only dependency** (vendored, switched
  off a GitHub fork); the software build pulls in zero speedy2d.
- **`Theme` trait slimmed** to primitive drawing plus resource lookup; the legacy
  per-widget `draw_*` methods (~440 lines) were removed in favor of role-named
  drawables.
- **Typography overhaul.** `Typeface::default()` uses the OS UI font (Segoe UI on
  Windows); default sizes moved into palette typeface roles, and `text_size` is
  now device-independent pixels everywhere (scaled by DPI like `font_size`).
- **Escape-key policy.** Esc only dismisses popups and closes child/dialog
  windows by default; the app wires its own Esc-to-quit/hide (the auto-quit
  fallback was removed).
- **Image cache refactor** — id-keyed cache with Drop-driven eviction and
  GPU-multiply tinting, consolidating all image consumers onto one source.

### Fixed

- **HiDPI text.** `Label`/`Edit`/`Memo`/`RichText` treated constructor
  `text_size` as raw pixels, so text rendered half-size on scaled displays;
  `text_size` is now dips everywhere.
- **Breaking-layout overlap.** Wrapping frames advanced the cursor by content
  size instead of each child's laid-out rect, overlapping fixed-size children.
- **MenuBar hover-switch crash** caused by a let-chain holding a borrow across
  `borrow_mut` (let-chains lack the 2024 early-drop of `if let` temporaries).
- **CheckBox/RadioButton** — `set_checked` value handling, and `CheckedChanged`
  now actually fires on state changes.
- **Event, cursor, and popup-position bugs** in `TabView`, popups, and the
  `Edit`/`Label`/`RichText` context menus.
- **Texture cache leak** that grew GPU memory on resize.
- **Selected-text color/contrast** now derives from the selection background.
- **quick-xml deprecation warnings.**
