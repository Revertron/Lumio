# Lumio Evolution Roadmap

A tiered plan for evolving Lumio from its current state (26 view types, 3 layout
engines, one theme with light/dark palettes, multi-window, two rendering
backends) into a complete desktop GUI toolkit.

## Current state

The core is solid: retained view tree, XML layouts, three layout engines behind
the `Layout` trait, virtualized lists (RecyclerView), a full-featured TableView,
RichText, popups/dialogs/notifications/tooltips, SVG support, HiDPI awareness.

**Switchable rendering backends — DONE 2026-06.** Two compile-time-selected,
mutually-exclusive backends behind a backend-neutral seam (the `Theme`, text and
input abstractions): `backend-gl` (default — OpenGL via the vendored speedy2d)
and `backend-software` (CPU rendering via tiny-skia + fontdue, in a winit +
softbuffer window, with a headless UI → `Pixmap`/PNG path). Apps launch with the
neutral `lumio::run(ui, WindowConfig)` and switch backends by Cargo feature, no
source edits. `speedy2d` is now an optional dependency (absent from the software
build).

The gaps cluster into five areas:

- **Theming is hardcoded** — one Win95-style theme, colors baked in as constants.
- **Text input lacks table-stakes features** — no undo/redo, no password masking.
- **Standard widgets missing** — MenuBar, Slider, TreeView, SpinEdit.
- **Platform integration is thin** — no native OS file dialogs, no IME
  (multi-window and in-app modal dialogs now done — see items 6 and Tier 3).
- **Developer experience** — no docs, no XML includes, no hot reload, no tests.

---

## Tier 1 — Foundations that unblock real apps

### 1. Theme & styling system: resource-bundle themes — DONE

The big one — full design in **[docs/theming.md](docs/theming.md)**.
Implemented 2026-06: all six phases, including the dark palette
(`ui.set_palette(Palette::dark())`), dimension/typeface tokens, `@token`
references and `style=` bundles in layout XML.

A theme becomes a resource bundle (drawables + color palette + metrics +
typography) instead of a code module; the `Theme` trait shrinks to primitive
drawing + resource lookup. Dark mode becomes a palette swap, new themes are
XML-only, and `style=` / `@token` references become available in layout XML.

- Runtime theme switching — DONE: `ui.set_palette(..)` swaps the palette and
  triggers a full relayout/redraw (themes are palette-driven, so a palette swap
  *is* a theme switch; there is one `Theme` impl).
- Rogue hardcoded view colors (selection blue, placeholder gray, tooltip yellow,
  TableView selection) pulled into palette tokens — DONE (`selection`,
  `text_hint`, `tooltip_back`, `outline`, … in `src/drawing/palette.rs`).
- Visible **focus indicator** — DONE: focus-state drawables + a `focus` palette
  token.

### 2. Edit/Memo maturity — DONE

In rough order of pain:

- **Undo/redo** — DONE 2026-06: snapshot stacks with typing/deleting run
  coalescing in both Edit and Memo (Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z).
- **Password masking** — DONE 2026-06: `password="true"` on Edit renders
  bullets and disables copy/cut.
- **Input filters** — DONE 2026-06: per-char predicate via
  `set_input_filter()`, `filter="numeric"` and `allowed_chars="..."` in XML;
  an insert containing any disallowed character is rejected wholesale.

All contained within the two views.

### 3. Missing everyday widgets

- **MenuBar** with dropdown menus and submenus — DONE 2026-06: `<MenuBar>` /
  `<Menu>` / `<MenuItem>` XML tags (nested `<Menu>` = submenu), Click event
  with `clicked_item()`; PopupMenu gained submenus, keyboard navigation and
  owner routing as part of this work.
- **Slider** and a numeric **SpinEdit/Stepper** (the "spinner" already pending
  in the Alfis migration notes).
- **TreeView** — expand/collapse hierarchy; can reuse RecyclerView's
  virtualization for large trees.
- **Tri-state CheckBox** (indeterminate).
- **Editable / filtering ComboBox** (type-to-filter, autocomplete).

### 4. Event-system gaps — DONE

Implemented 2026-06: listeners centralized in `FieldsMain` (every view,
including Frame, accepts `on_event`), callbacks now receive a universal
`&EventData` payload (`Checked`/`Selected`/`Position`/`Key`).

- `FocusGained` / `FocusLost` — DONE: deferred `sync_focus()` sweep in UI,
  catches mouse, keyboard and programmatic focus changes.
- `HoverEnter` / `HoverExit` — DONE: central hit-test tracking in UI.
- `DoubleClick` — DONE: central detection (400 ms / 4 px / same view);
  Edit's internal word-select stays independent.
- `KeyDown` — DONE: fires on the focused view before built-in handling;
  returning true intercepts the key.
- `ContextMenu` — DONE: fires before dispatch; a consuming handler
  suppresses the built-in Edit/Memo/Label/RichText menus.

Keyboard accelerators — DONE: `ui.add_shortcut("Ctrl+Shift+S", handler)`
(string or typed `Shortcut`), dispatched as a fallback after the focused
view; blocked under modal dialogs. Dialog: Enter presses the focused/default
button, Esc presses the cancel button (`set_cancel_button`) or closes; the
window-level Esc policy moved from `on_keyboard_char` to after-dispatch
`on_key_down`.

### 5. Documentation

Gates everything else; cheap and high leverage:

- README with a widget gallery.
- Rustdoc on the public API (RadioButton, `UI::find_with`, Gravity, TableView,
  Grid are already on the pending list).
- Fix the stale view list in CLAUDE.md — DONE 2026-06-15 (full grouped widget
  list + backend/edition facts refreshed).

---

## Tier 2 — Platform integration & polish

### 6. Native dialogs and window control — PARTIALLY DONE

- In-app message/confirm/input dialogs — DONE 2026-06-12: `UI::show_message` /
  `show_confirm` / `show_input` plus the `crate::dialog::Dialog` builder, built
  on the multi-window support (auto-sized modal child window, Enter/Esc wired).
- Native OS file open/save dialogs via the `rfd` crate (small, cross-platform,
  the ecosystem standard) — still pending.
- Window title/icon/min-size/fullscreen setters on `WindowHelper` — speedy2d is
  already vendored (cursor support), so the pattern is established.

### 7. Render-on-demand

`win.rs` ticks every 15 ms regardless; an idle app burns CPU repainting an
unchanged screen. Switch to redraw-on-dirty (views already have
`request_redraw` plumbing) with the timer running only while animations are
active. Matters for long-running apps like a node/wallet.

### 8. Animation framework

Easing and timing are hand-rolled three separate times (NotificationStack,
ProgressBar indeterminate, caret blink). Extract a small `Animator`
(value + duration + easing + completion callback), then offer fade/slide on
visibility changes, smooth scrolling, animated popup open/close. Not a
keyframe system — just deduplicate what exists and expose it to user code.

### 9. Layout conveniences

- `gap`/`spacing` attribute on containers (today: margin on every child).
- `min_width`/`max_width` constraints alongside `Dimension`.
- Aspect-ratio on ImageView.
- Colspan/rowspan in Grid.

### 10. XML `<include>` + hot reload

- `<include src="sidebar.xml"/>` for splitting large layouts.
- Debug-mode file watcher that reloads XML on save — instant layout iteration
  instead of recompile-and-restart.

### 11. Clipboard & selection completeness

- Image and rich-text clipboard (arboard already supports images).
- Selectable Label/RichText stays **mouse-only** by design (keyboard/focus
  selection was considered and rejected).

---

## Tier 3 — Long-horizon, decide deliberately

Each deserves its own design discussion before committing.

- **IME support** — required for CJK and dead-key input. Handle composition
  events from winit, render preedit text in Edit/Memo. Moderate effort, high
  importance for international users. (If international input becomes a
  priority, this moves to Tier 1.)
- **Accessibility via AccessKit** — the Rust ecosystem standard (egui, Bevy).
  Map the view tree to an accessibility tree. Significant but well-trodden.
- **Multi-window** — DONE 2026-06-12. `UI::open_window(WindowRequest{.., modal})`
  / `UI::close_window()`; the vendored speedy2d was migrated to winit 0.30
  `run_app` (`ApplicationHandler`), `WindowHelper::create_window` /
  `create_modal_window` / `close_window`; per-window GL `make_current`, an
  app-modal stack, and close-main-exits-app. Example `examples/multiwindow_example.rs`.
- **i18n** — `@string/key` resolution in XML attributes against a locale table.
- **Data binding / reactive models** — `{field}` expressions in XML with
  observable models. A big philosophical shift from the current imperative
  callback style; only pursue if callback wiring proves painful in practice.
  Current lean: no.
- **Testing infrastructure** — `src/tests.rs` is an empty skeleton. The
  `backend-software` renderer now unblocks this: `render::render_to_pixmap(..)`
  produces a `tiny_skia::Pixmap` headlessly (no window, no `Graphics2D` to mock),
  so layout/visual regression tests can snapshot the rendered output
  (`tests/software_render.rs` is the starting point). Pixel-snapshot or
  draw-call-recording harnesses can build on this.

---

## Explicitly deferred

- **RTL/bidi text** — logical→visual reordering is a large, separate effort;
  out of scope for this roadmap.

---

## Suggested sequencing

1. **Theme/palette refactor + dark mode** — touches everything, so do it before
   the widget count grows further.
2. **Edit undo/redo + password mask**, **MenuBar/submenus**, **Slider/SpinEdit**
   — independent of theming, can proceed in parallel.
3. **Event gaps + shortcuts**, then **docs/README** as the Tier-1 capstone.
4. Tier 2 roughly in listed order; **render-on-demand** early because it's
   correctness-adjacent.
5. Tier 3 items one at a time, design doc first.
