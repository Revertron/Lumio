# Lumio

A declarative, XML-based retained-mode GUI toolkit for Rust desktop apps.

Lumio lets you describe a window's UI in XML, load it into a retained view tree,
wire up event handlers in Rust, and run it — on either an OpenGL or a pure-CPU
rendering backend, selected at compile time with no source changes.

> **Status:** early-stage and in active development. Published on crates.io as
> [`lumio-gui`](https://crates.io/crates/lumio-gui) (the `lumio` name was taken);
> the library itself is still `lumio`, so you `use lumio::…` in code. APIs may
> still shift.

## Highlights

- **Declarative XML layouts** — define the view tree in XML, load with
  `UI::from_xml`; attributes map onto views, styles cascade from parents.
- **Retained-mode tree** with three layout engines behind a `Layout` trait
  (linear, overlay, dock), HiDPI/scale awareness, and per-view gravity.
- **Switchable rendering backends** (see below) behind one window loop.
- **Theming** — palette-driven colors/dimensions/typefaces, light & dark, with
  `@token` references and `<Style>` in layout XML.
- **Rich widget set** — text input with undo/redo & selection, tables, virtualized
  lists, rich text, menus, dialogs, notifications, SVG & raster images.
- **Multi-window + app-modal dialogs**, tooltips, popups, mouse-cursor switching.
- **Headless rendering** (software backend) — render a UI straight to a
  `tiny_skia::Pixmap`/PNG with no window, handy for tests and screenshots.

## Rendering backends

Both backends run on a single Lumio-owned `winit` window loop (`src/window/`);
they differ only in the per-window `RenderSurface`. Pick them via Cargo features:

| Feature | Renderer | Notes |
| --- | --- | --- |
| `backend-gl` *(default)* | OpenGL via the vendored `speedy2d` used as a pure renderer, over a `glutin` GL context Lumio creates | GPU-accelerated |
| `backend-software` | CPU rendering via `tiny-skia` + `fontdue`, blitted with `softbuffer` | also supports headless UI → `Pixmap`/PNG |
| *both together* | GL first, **automatic software fallback** if GL init fails (VMs / emulated framebuffers) | `LUMIO_BACKEND=gl\|software` forces one; `lumio::active_backend()` reports the one in use |

Apps launch with the backend-neutral `lumio::run(ui, WindowConfig)` and never name
a backend in source — switching is a Cargo-feature change. `speedy2d` is an
optional, renderer-only dependency (its windowing feature is off), absent entirely
from a software-only build. Design notes: `docs/unified_window_loop.md`.

## Quick start

```toml
# Cargo.toml — the crate is published as `lumio-gui`, but imported as `lumio`.
[dependencies]
lumio-gui = "0.1"                             # GL backend (default)
# software backend instead:
# lumio-gui = { version = "0.1", default-features = false, features = ["backend-software"] }
# GL with automatic software fallback (e.g. for apps that must run in GPU-less VMs):
# lumio-gui = { version = "0.1", features = ["backend-software"] }
```

```rust
use lumio::prelude::*;

const UI_XML: &str = r#"
<Frame direction="vertical" padding="16">
    <Label text="Hello, Lumio!"/>
    <Button id="btn" text="Click me" width="160"/>
</Frame>
"#;

fn main() {
    let ui = UI::from_xml(UI_XML, 480, 220, default_typeface(), 1.0).unwrap();

    if let Some(btn) = ui.get_view("btn") {
        btn.borrow_mut().on_event(EventType::Click, Box::new(|_ui, view, _data| {
            if let Some(b) = view.as_any().downcast_ref::<Button>() {
                b.set_text("Clicked!");
            }
            true // return true to request a redraw
        }));
    }

    lumio::run(ui, WindowConfig::new("Lumio Demo", 480, 220).center());
}
```

## Widgets

- **Text:** `Label`, `Edit`, `Memo` (multi-line), `RichText` (spannable HTML subset)
- **Buttons & toggles:** `Button`, `ImageButton`, `CheckBox`, `RadioButton`
- **Selection & data:** `ComboBox`, `List`, `RecyclerView` (virtualized),
  `TableView` (sortable/resizable columns)
- **Layout & containers:** `Frame`, `Grid`, `ScrollView`, `TabView`, `SplitPanel`,
  `Separator`
- **Images & indicators:** `ImageView`, `ProgressBar`, `StatusBar`
- **Menus & overlays:** `MenuBar`, `PopupMenu`, `NotificationStack`

## Building & running

```bash
# GL backend (default)
cargo build
cargo run --example example

# software backend (CPU)
cargo build --no-default-features --features backend-software
cargo run --example example --no-default-features --features backend-software

# both backends: GL with runtime software fallback
cargo build --features backend-software
LUMIO_BACKEND=software cargo run --features backend-software --example example

# tests (per backend) and linter
cargo test
cargo test --no-default-features --features backend-software
cargo test --features backend-software
cargo clippy
```

Runnable demos live in [`examples/`](examples/) — most use the neutral
`lumio::run` launcher, so they build on either backend. Two are software-specific
(`headless_render`, `software_window_example`) and need
`--features backend-software`.

## Documentation

- `docs/theming.md` — palettes, `@token` resolution, dark mode, `<Style>`.
- `docs/unified_window_loop.md` — the shared winit loop + `RenderSurface` design.
- `docs/backend_consolidation.md` — the backend-neutrality work and its rationale.
- `ROADMAP.md` — where the project is and what's planned.

## Requirements

Rust 2024 edition. The GL backend needs an OpenGL-capable environment; the
software backend renders on the CPU (no GPU required).
