# Unified window loop — design doc (Tier 3 #8, revised)

**Status:** proposal for review. No code yet.

## Goal

Collapse the duplicated window/event-loop/modal machinery into **one** Lumio
implementation, with the rendering backend reduced to a pluggable per-window
*surface*. speedy2d stops owning a window/event loop and becomes a pure GL
renderer that Lumio hands a context to.

## Why this, and why now

After Tiers 1–2, the per-window glue is thin and the meaty shared logic (escape,
cursor, clip/opacity, eviction) is already factored out. The largest remaining
duplication isn't *within* Lumio — it's that **Lumio's software `App` and the
vendored speedy2d both implement the same winit `ApplicationHandler`** (window
map, `main_window`, `modal_stack`, `is_blocked_when_modal` input gating,
refocus-top-on-close). speedy2d's copy lives at
`window_internal_glutin.rs:514-527, 724-743, 824-901`; Lumio's copy is in
`src/software_window/mod.rs`. We can't share across the crate boundary — *unless*
Lumio owns the loop for both backends and demotes speedy2d to a renderer.

## Feasibility — confirmed, and no speedy2d fork needed

speedy2d already separates renderer from windowing:

- `GLRenderer::new_for_gl_context(viewport_size, |name| ...)` — `lib.rs:468`,
  `unsafe`, builds a renderer over an **externally created/current** GL context
  via a proc-address loader closure.
- `GLRenderer::set_viewport_size_pixels(UVec2)` — `lib.rs:535`, for resize.
- `GLRenderer::draw_frame(|g: &mut Graphics2D| { ... })` — `lib.rs:633`, hands us
  the `Graphics2D`. `RendererGL` drives it unchanged (and `Graphics2D` even has its
  own `create_image_from_*`, `lib.rs:671/697/743`, so the GL image cache path is
  unaffected).
- Both `GLRenderer` and `Graphics2D` are public; the whole winit/glutin window
  path is gated behind speedy2d's **`windowing`** Cargo feature, which we turn off
  (`default-features = false`). `image-loading` stays on (RendererGL needs it).

So speedy2d needs **no source changes** — just feature flags.

## Current vs target

```
            CURRENT                                  TARGET
  GL:  speedy2d GlutinApp (winit loop,        Lumio winit loop (one impl)
       window map, modal stack)  ──┐            ├── owns map, modal stack,
       └─ calls Win<T> per window  │            │   gating, escape, cursor,
                                    │            │   command-pump
  SW:  Lumio App (winit loop, ─────┘            └── per-window RenderSurface:
       window map, modal stack)                      • GlSurface  (glutin + speedy2d GLRenderer → RendererGL)
       └─ WindowState per window                     • SoftwareSurface (softbuffer + tiny-skia → RendererSoftware)
```

`win.rs` and the `Win<T>` per-window handler are **deleted**. The software loop
generalizes into a neutral `src/window/` module.

## Proposed API seam (minimal)

Per-window rendering behind one trait; everything else (input dispatch, modal
stack, command pump) is backend-neutral and written once.

```rust
/// Per-window render target. Owns the backend-specific surface + caches and
/// knows how to paint a laid-out UI and present it.
trait RenderSurface {
    fn resize(&mut self, width: u32, height: u32);
    /// Build the backend Renderer, paint `ui` into the surface, and present.
    /// Eviction drain + pending-palette handling live here (cache type differs
    /// per backend).
    fn paint(&mut self, ui: &UI, palette: &Palette, registry: &DrawableRegistry, scale: f64);
}
```

- **SoftwareSurface** = today's `WindowState::render` body: `Pixmap` +
  softbuffer surface + `GlyphCache`/`SoftwareImageCache`; paints via
  `RendererSoftware`, blits to softbuffer.
- **GlSurface** = glutin `Surface` + `PossiblyCurrentContext` +
  `speedy2d::GLRenderer` + GL `ImageCache`; `paint` = make-current →
  `draw_frame(|g| { let mut t = RendererGL::new(g, registry, palette, &mut cache, w, h, scale); ui.paint(&mut t); })` → `swap_buffers`.

Per-window neutral state (moves out of `WindowState`): `ui`, `registry`,
`palette`, `width`, `height`, `scale`, `mouse_pos`, `mod_state`, `last_cursor`,
`pending_layout`, `is_child`, `hide_on_close`, plus `surface: <the RenderSurface>`.
Backend chooses the concrete surface type at the `cfg` seam (a `#[cfg]` type
alias or a small `create_surface(window, w, h, scale)` factory the `App` owns —
the factory also holds the shared softbuffer `Context` / glutin `Display`).

## Dependencies / Cargo changes

- `speedy2d = { path = "../Speedy2D", default-features = false, features = ["image-loading"], optional = true }` (drop `windowing`).
- Add under `backend-gl` (versions pinned to match the vendored speedy2d so the
  tree has no duplicates and the proc-loader matches glow 0.17):
  - `glutin = "0.32.3"`
  - `glutin-winit` (the winit↔glutin `DisplayBuilder` bridge; version paired with glutin 0.32 — **confirm in Phase 1**)
  - `raw-window-handle = "0.6.2"`
- winit is already 0.30 (matches speedy2d's 0.30.12); `rwh_06` already enabled.
- `lumio::run` routes to the one unified loop under both features.

## The GL context plumbing Lumio must own

speedy2d's context creation (`window_internal_glutin.rs:1238-1301`) is private
and not reusable, so we reimplement it (~60 lines, per-window): build glutin
display+config from the winit window (via `glutin-winit::DisplayBuilder`), create
the `NotCurrentContext`, create the window `Surface`, `make_current`, set swap
interval (vsync), `swap_buffers` after each `draw_frame`, and call
`GLRenderer::set_viewport_size_pixels` on resize. This is the main *new* burden
the move takes on.

## Phase plan

1. **Spike (throwaway) — DONE.** `examples/gl_spike.rs` creates a glutin context
   on a bare winit window and renders a `speedy2d::GLRenderer` frame with no
   speedy2d loop. Validated on real hardware: window + context created, renderer
   built (proc-loader OK), and `draw_frame` + `swap_buffers` ran without panic.
   **Versions pinned & unified with the vendored speedy2d (no duplicates):**
   glutin 0.32.3, glutin-winit 0.5.0, winit 0.30.13, raw-window-handle 0.6.2,
   glow 0.17.0. The glutin sequence mirrors speedy2d's
   (`window_internal_glutin.rs`): `DisplayBuilder` → `create_context` (GL 2.0) →
   `build_surface_attributes`/`create_window_surface` → `make_current` →
   `set_swap_interval` → `GLRenderer::new_for_gl_context(size, |s| display.get_proc_address(CString))`.
2. **Neutral loop, software only — DONE.** `src/software_window/` → `src/window/`:
   neutral `mod.rs` (App, WindowState, modal stack, input dispatch, escape/cursor
   policy, command pump) + `surface_software.rs` (`SoftwareBackend` +
   `SoftwareSurface: RenderSurface`, the only place softbuffer/tiny-skia is
   touched) + the moved `input_winit.rs`. `WindowState` now holds a `Surface`
   (then a cfg alias; today an enum over the compiled backends, enabling the
   runtime GL → software fallback) instead of inline pixmap/caches; rendering
   is `surface.paint(...)`.
   Still gated `backend-software`. No behavior change: GL + software both build,
   all 78 software tests pass, and `software_window_example` opens + renders a
   real window without crashing. References updated in `lib.rs`/`app.rs`/`prelude.rs`.
3. **GL surface — DONE.** Added `window/surface_gl.rs` (`GlBackend` + `GlSurface:
   RenderSurface`: glutin context + `speedy2d::GLRenderer` → `RendererGL`, per-window
   make-current). The backend seam now owns window creation (`create(event_loop,
   attrs)`) because GL must build the window alongside a matching GL config.
   `backend-gl` routes through the neutral loop; `app::run` collapsed to one
   backend-neutral fn. Deleted `win.rs`, the `Win<T>` handler, `input/from_speedy2d.rs`,
   and `VirtualKeyCode::from_speedy2d`. speedy2d flipped to
   `default-features = false, features = ["image-loading"]` — it now pulls **zero**
   winit/glutin (verified via `cargo tree`); Lumio owns those. Both backends build,
   78 tests pass on each, all examples build, and the GL `example` opens + renders
   for ~8s with no GL-setup errors. vsync is on (`SwapInterval::Wait(1)`), matching
   the old speedy2d default.
4. **Validate both** on real windows: input, focus, cursor shapes, Esc policy,
   app-modal dialogs, multi-window, resize/HiDPI, image tinting, palette/dark
   switch. (Window loops aren't unit-testable — this phase is manual and required
   per the project's "test visually" rule.)

Each phase is independently shippable; the software backend keeps working
throughout.

## Open questions / risks

- ~~**Version pinning:**~~ **Resolved in Phase 1** — glutin 0.32.3 /
  glutin-winit 0.5.0 / winit 0.30.13 / raw-window-handle 0.6.2 / glow 0.17.0 all
  resolve to single versions, unified with the vendored speedy2d.
- **`unsafe` make-current discipline:** `new_for_gl_context` assumes exclusive
  control of the current context. Multi-window needs one `GLRenderer` per window
  and a make-current before each paint (speedy2d does this internally via
  `WindowEntry::make_current_and_drop`). Decide: shared `Display`, per-window
  context+surface+renderer.
- ~~**vsync / swap interval:**~~ Phase 3 enables vsync (`SwapInterval::Wait(1)`)
  per window, matching the old speedy2d default. Multi-window behavior to confirm
  in Phase 4.
- **Per-window context-current cost** each frame (speedy2d already pays it).
- **macOS main-thread** GL/window constraints (winit already enforces; confirm GL
  context creation is main-thread-safe in our loop).
- **Eviction/image cache** stays per-surface (GL `ImageHandle` vs software RGBA);
  `image_source::drain_evictions` is already generic and called inside `paint`.
- **Validation is manual** — highest-risk part of the whole consolidation effort.

## Parity with the old speedy2d window

speedy2d's window code set a few things our loop must match:
- **Windows exe-resource icon — restored.** speedy2d loaded `Icon::from_resource(1, None)`
  (the `winres` default) into `with_window_icon` on Windows. Lost in the migration,
  re-added in `window/mod.rs::create_window`. (Found via Alfis: title-bar/taskbar
  icon went missing.)
- **Create-hidden → position → show — restored.** winit has no "center on
  creation" attribute, so positioning happens after the window exists. If the
  window were already visible it would appear at the OS default corner and then
  jump to center. So the loop now always creates the window hidden, centers it
  while hidden, then `set_visible(true)` (honoring `WindowConfig::visible` — a tray
  app stays hidden), re-applying the position after show for Linux WMs that ignore
  a pre-show position. Mirrors speedy2d (`window_internal_glutin.rs:622`). (Found
  via Alfis: the centered window visibly jumped from the corner.) A blank first
  frame between show and the first paint is still possible, as it was with speedy2d.
- `window_level` / `maximized` / `transparent` / `decorations` were speedy2d
  options Lumio's `WindowConfig` never exposed, so there's nothing to match.

## Explicitly NOT changed

`Renderer` trait, `crate::text`, `crate::input`, `RendererGL`/`RendererSoftware`
internals, the drawable engines, and `WindowConfig`/`WindowRequest` semantics.
This is a window-loop/ownership refactor, not a rendering or input change.

## Relation to earlier tiers

Subsumes the deferred Tier 2 **#5 command pump** (it becomes part of the single
neutral loop). The Tier 1–2 shared helpers (`escape_press_action`,
`cursor_transition`, `OpacityStack`, `Rect::intersect`, `drain_evictions`) all
carry over unchanged.

