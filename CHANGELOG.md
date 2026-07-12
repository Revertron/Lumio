# Changelog

All notable changes to Lumio are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
