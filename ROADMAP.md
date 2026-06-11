# Lumio Evolution Roadmap

A tiered plan for evolving Lumio from its current state (26 view types, 3 layout
engines, one theme, single window) into a complete desktop GUI toolkit.

## Current state

The core is solid: retained view tree, XML layouts, three layout engines behind
the `Layout` trait, virtualized lists (RecyclerView), a full-featured TableView,
RichText, popups/dialogs/notifications/tooltips, SVG support, HiDPI awareness.

The gaps cluster into five areas:

- **Theming is hardcoded** — one Win95-style theme, colors baked in as constants.
- **Text input lacks table-stakes features** — no undo/redo, no password masking.
- **Standard widgets missing** — MenuBar, Slider, TreeView, SpinEdit.
- **Platform integration is thin** — no native dialogs, no IME, single window.
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

- Runtime theme switching: `ui.set_theme()` + full relayout/redraw.
- Pulls rogue hardcoded view colors (selection blue, placeholder gray, tooltip
  yellow, TableView selection) into themed tokens.
- Add a visible **focus indicator** (currently focus has no visual at all).

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

### 4. Event-system gaps

Add to `EventType`:

- `FocusGained` / `FocusLost` (validate-on-blur).
- `HoverEnter` / `HoverExit`.
- `DoubleClick` — detection already exists inside Edit; promote it.
- `KeyDown` exposure to user code.
- Trait-level `ContextMenu` event (right-click is currently swallowed by
  Edit's hardcoded menu).

Plus **keyboard accelerators**: a UI-level shortcut registry
(`ui.add_shortcut(Ctrl+S, handler)`) and Enter/Esc default-button handling
in Dialog.

### 5. Documentation

Gates everything else; cheap and high leverage:

- README with a widget gallery.
- Rustdoc on the public API (RadioButton, `UI::find_with`, Gravity, TableView,
  Grid are already on the pending list).
- Fix the stale view list in CLAUDE.md.

---

## Tier 2 — Platform integration & polish

### 6. Native dialogs and window control

- File open/save and message boxes via the `rfd` crate (small, cross-platform,
  the ecosystem standard) rather than hand-rolling.
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
- **Multi-window** — `Win` wraps exactly one UI. Requires rework of the event
  loop and the thread-local asset/font caches.
- **i18n** — `@string/key` resolution in XML attributes against a locale table.
- **Data binding / reactive models** — `{field}` expressions in XML with
  observable models. A big philosophical shift from the current imperative
  callback style; only pursue if callback wiring proves painful in practice.
  Current lean: no.
- **Testing infrastructure** — `src/tests.rs` is an empty skeleton blocked on
  mocking `Graphics2D`. A headless backend that records draw calls would
  unlock layout regression tests — valuable exactly when the theming refactor
  (item 1) starts churning rendering code.

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
