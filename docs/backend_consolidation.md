# Backend consolidation plan (GL / speedy2d vs. software / tiny-skia)

Analysis of the two compile-time backends and what can be consolidated to keep
their internal APIs and behavior aligned. The backend seam itself is already
well drawn; this document targets *duplication inside the backend-specific
files* and a handful of behavior divergences.

## Already unified (no work needed)

- `lumio::run(ui, WindowConfig)` — neutral launcher (`src/app.rs`).
  `WindowConfig` is the neutral superset of window options.
- `Theme` trait (`src/themes/mod.rs`) — the rendering abstraction;
  `UI::paint(&mut dyn Theme)` drives both.
- `crate::text` — `TextBlock`/`TextLine`/`Glyph`/`FontHandle`/`TextShaper` with
  an opaque per-backend draw payload.
- `crate::input` — `MouseButton`/`MouseScrollDistance`/`MouseCursorType`/
  `ModifiersState`/`VirtualKeyCode`; each backend converts *into* these at its
  boundary (`input/from_speedy2d.rs`, `software_window/input_winit.rs`).
- The `UI` driving contract — `layout`/`start`/`update`/`paint`/`on_mouse_*`/
  `on_key_*`/`take_window_requests`/`take_close_request`/`take_window_command`/
  `take_pending_palette`/`current_cursor`. Both loops call exactly this set.
- `DrawCommand` IR + `Palette` — the same drawable tree is walked by both
  engines.

## Tier 1 — pure duplication, low risk (DOING NOW)

### #1 Share `eval_expr` + `Axis` between the two drawing engines
`engine.rs::eval_expr` and `engine_software.rs::eval` are the same pure-math
function over `Expr` + bounds + scale + axis (the software file's header comment
already says it's a copy). `Axis` is duplicated too. Move both into
`drawing/primitives.rs` as a `pub fn eval_expr(expr, bounds, axis, scale)` and a
`pub enum Axis`; each engine keeps a thin forwarding `eval`/`eval_expr` method so
call sites are untouched, and the ~30-line match exists once.

### #2 Promote palette-delegating `Theme` methods to defaults
`typeface`, `color`, `dimension`, `get_back_color`, `get_text_color` are
byte-for-byte identical in `classic.rs` and `software.rs` (they only delegate to
`self.palette`). Add `fn palette(&self) -> &Palette` to the `Theme` trait, make
those five default methods, and remove them from both impls (each just provides
`palette()`). Removes ~50 identical lines per theme and prevents drift.

### #3 Share the image-eviction drain
`win.rs::on_draw` and `software_window::render` run the same
`take_pending_evictions` → `remove` → `requeue` loop. Add
`image_source::drain_evictions(&mut HashMap<u64, V>)` and call it from both.
Both caches are `HashMap<u64, _>` aliases, so one generic helper covers both.

## Tier 2 — shared policy, moderate effort (#4/#6/#7 DONE; #5 → Tier 3)

- **#4 Escape-key policy — DONE.** Extracted `UI::escape_press_action(is_child)
  -> EscapeAction` (`ui.rs`); both loops map the action to their own redraw/close
  and the long "defer close to key-release" rationale lives once on the
  `EscapeAction::CloseChildWindow` variant.
- **#5 Per-tick UI command pump — deferred to Tier 3.** A clean shared pump needs
  the `WindowHost` trait below: the two loops have different shapes (GL
  per-window-handler leaning on speedy2d's internal multi-window vs. software's
  manual window-map), and child-window creation/close differ. Forcing it now
  means closure indirection that's *more* complex, not less.
- **#6 Clip/opacity — DONE.** Added `Rect::intersect` (`types.rs`) to dedupe the
  clip-intersection math (both `clip_rect`s + software's `clip_rect_geom`), and an
  `OpacityStack` helper (`themes/mod.rs`) embedded in both themes, hidden behind
  the existing `current_opacity()` (no caller churn). Left `current_clip`/
  `clip_stack` as plain fields — push/pop are one-liners; a `ClipStack` struct
  there is churn (renaming software's hot clip path) without real dedup. `set_clip`
  stays per-backend (software also tracks `clip_full`/`clip_mask`).
- **#7 `apply_cursor` — DONE.** Extracted `input::cursor_transition(current,
  &mut last) -> Option<MouseCursorType>` (returns the cursor to push, or `None`);
  both `apply_cursor`s shrink to one `if let`. Returns-a-value form avoids the
  closure/borrow issue a `set_cursor` callback would hit.

## Tier 3 — structural, biggest payoff (design doc written)

- **#8 Unify the window loop.** Originally scoped as a `WindowHost` trait working
  *around* the asymmetry (speedy2d owns multi-window + modal-stack bookkeeping
  internally; the software backend re-implements it). Investigation found a
  cleaner road: both backends are winit `ApplicationHandler`s, and speedy2d
  already supports renderer-only mode (`GLRenderer::new_for_gl_context` +
  `draw_frame`, gated by its `windowing` feature). So **Lumio owns one winit loop
  for both backends** and demotes speedy2d to a pure GL renderer behind a
  per-window `RenderSurface` seam; `win.rs` is deleted. This also subsumes the
  deferred #5 command pump. Full design + phase plan + risks in
  [unified_window_loop.md](unified_window_loop.md). **Phases 1–3 done** (spike →
  neutral loop → GL surface + `win.rs`/speedy2d-windowing removed); both backends
  build, all tests pass, both open + render. **Phase 4** (manual validation of
  dialogs/multi-window/cursor/Esc/HiDPI on real windows) is the remaining piece.

## Divergences (behavior gaps, not duplication)

- **GL drawable `RoundRect`/`Path` — FIXED.** `engine.rs` now renders `RoundRect`
  (speedy2d `draw_rounded_rectangle` for fill; flattened outline for stroke) and
  `Path` (beziers flattened, filled via tessellated `Polygon`, stroked as line
  segments); the match is exhaustive (no silent `_`). Note this was *latent* — the
  XML parser only emits `rect`/`circle`/`triangle`/`line`, so these `DrawCommand`s
  are programmatic-only today; implemented for engine parity, not runtime-tested.
- **Software image scaling — FIXED.** `SoftwareTheme::blit_rgba` scales the source
  to the destination rect (tiny-skia transform, bilinear when scaling, nearest at
  1:1), matching the GL backend. (The `_cache_key` ignore is fine — raster images
  arrive pre-decoded, nothing to cache; the decode for *file* images is already
  cached.) Verified by `raw_image_scales_to_rect` + `examples/image_scaling_example`.
- **Software text — FIXED.** Now honors `TextOptions::align` (fontdue
  `horizontal_align`), `trim_each_line` (leading whitespace per wrapped line,
  guarded so it's a no-op otherwise), and per-glyph **font fallback** (splits text
  into maximal same-font runs across the chain — fontdue has no auto-fallback). All
  no-ops for the default Latin/left/no-fallback case.
- **`PendingWindow` ↔ `WindowConfig` — FIXED.** `PendingWindow` (`window/mod.rs`)
  now wraps a `WindowConfig` plus just the loop-internal `ui`/`is_child`/`modal`,
  instead of re-listing all 11 window-option fields. The two construction sites
  build the config directly (`run_with_config` passes its own; `tick` uses the
  `WindowConfig` builder for dialogs). No behavior change.

## Recommended order

Tier 1 (#1–#3) and Tier 2 (#4/#6/#7) are **done**. Remaining: Tier 3 #8
(`WindowHost` driver, which also subsumes the deferred #5) once the modal-stack
asymmetry is addressed, plus the divergence fixes — fix the GL `RoundRect`/`Path`
gap regardless of consolidation (it's a correctness bug).
