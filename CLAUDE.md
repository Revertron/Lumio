# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lumio is a Rust GUI library that provides a declarative XML-based layout system for creating desktop applications. It uses a retained-mode GUI architecture where view hierarchies are maintained in memory, with support for theming and event handling.

Both backends run on a single Lumio-owned `winit` window loop (`src/window/`); the
backend selects only the per-window `RenderSurface` (mutually exclusive):
- `backend-gl` (default): OpenGL rendering via the vendored `speedy2d` used as a
  *pure renderer* (its `windowing` feature is off), drawing into a `glutin` GL
  context Lumio creates.
- `backend-software`: CPU rendering via `tiny-skia` + `fontdue`, blitted to a
  `softbuffer` surface. Also supports headless rendering (UI → `tiny_skia::Pixmap`/PNG).

Apps launch backend-agnostically with `lumio::run(ui, WindowConfig::new(..))`; switching backends is a Cargo-feature change, no source edits.

## Build Commands

```bash
# Build the library
cargo build

# Build with optimizations
cargo build --release

# Run tests
cargo test

# Run the example application
cargo run --example example

# Build / run / test the software backend (no GL)
cargo build --no-default-features --features backend-software
cargo run --example example --no-default-features --features backend-software
cargo test  --no-default-features --features backend-software

# Run linter
cargo clippy

# Run linter with auto-fix
cargo clippy --fix
```

Note: the software backend renders on the CPU and is ~30x slower unoptimized, so
`[profile.dev]` in `Cargo.toml` optimizes dependencies (`opt-level = 3` for
`package."*"`) to keep debug builds responsive.

## Architecture

### Core Components

- **UI System** (`src/ui.rs`): The main UI structure that manages view hierarchies, XML parsing, and view registration. Uses a factory pattern via `register<T>()` to create views dynamically from XML.

- **View Trait** (`src/traits.rs`): Central trait that all UI elements implement. Defines the contract for layout, painting, event handling, and state management. Uses `downcast-rs` for runtime type casting.

- **Window loop**: one backend-neutral winit `ApplicationHandler` in **`src/window/`** (15ms tick; multi-window + app-modal dialogs), behind the neutral `lumio::run(ui, WindowConfig)` launcher (`src/app.rs`). The per-window paint sits behind a `RenderSurface` trait, cfg-selected per backend: `GlSurface` (`window/surface_gl.rs`, glutin context + `speedy2d::GLRenderer` → `Classic`) or `SoftwareSurface` (`window/surface_software.rs`, tiny-skia → softbuffer). winit→Lumio input conversions live in `window/input_winit.rs`. Input/event types are backend-neutral (`src/input/`). See docs/unified_window_loop.md for the design.

- **Theme System** (`src/themes/`): Pluggable theming via the `Theme` trait — the rendering abstraction seam. `Classic` (GL) and `SoftwareTheme` (tiny-skia) are the two implementations, cfg-selected by backend. Handles rendering of different view states (focused, hovered, pressed, etc.). Text shaping is abstracted behind `crate::text` (speedy2d vs fontdue).

- **Prelude** (`src/prelude.rs`): Convenience re-exports for common types. Use `use lumio::prelude::*;` for quick access.

### View Hierarchy

Views are organized in a retained parent-child tree structure:
- **Container** (`src/containers.rs`): `Frame` is the primary container that can hold child views. Supports vertical/horizontal layout with optional line breaking.
- **Views** (`src/views/`): Concrete implementations, grouped by role:
  - Text: `Label`, `Edit`, `Memo` (multi-line), `RichText` (spannable HTML subset)
  - Buttons & toggles: `Button`, `ImageButton`, `CheckBox`, `RadioButton`
  - Selection & data: `ComboBox`, `List`, `RecyclerView` (virtualized), `TableView` (sortable/resizable columns), `TreeView` (lazy expand/collapse hierarchy), `IconList` (Explorer-"List"-mode column flow, multi-select)
  - Layout & containers: `Frame`, `Grid`, `ScrollView`, `TabView`, `SplitPanel`, `Separator`
  - Images & indicators: `ImageView`, `ProgressBar`, `StatusBar`
  - Menus & overlays: `MenuBar`, `PopupMenu`, `NotificationStack`

  All are re-exported from the prelude. New views should follow `CheckBox` as a template (see "Creating New View Types" below).

### Layout System

- Uses `Dimension` enum for flexible sizing: `Min`, `Max`, `Dip` (device-independent pixels), `Percent`
- Layout is performed top-down via `layout_content()` calls
- Supports padding/margin via `Borders` struct
- Scale-aware for HiDPI displays

### XML Layout

UI can be defined declaratively in XML and loaded via `UI::from_xml()`. Views are instantiated by matching XML tag names to registered view types. Attributes are passed to views via `set_any()`.

Example pattern:
```xml
<Frame id="main" direction="vertical">
    <Button id="btn1" text="Click me" width="200"/>
    <Edit id="edit1" text="Enter text"/>
</Frame>
```

### Asset System

Fonts and other assets are loaded via the `AssetsProvider` trait (`src/assets.rs`):
- Thread-local storage for global provider and font cache
- Fonts must follow naming convention: `fonts/{FontName}-{Style}.ttf`
- Use `set_provider()` to register a custom asset provider

### Drawing System

XML-based drawable definitions live in `src/drawables/` and are embedded at compile time via `include_str!`. The `DrawableRegistry` (`src/drawing/registry.rs`) manages loading and caching. Drawables support state-based selectors (focused, hovered, pressed, etc.).

### Event System

- Events are typed via `EventType` enum (Click, CheckedChanged, MouseDown, etc.)
- Event handlers are registered via `view.on_event(EventType, callback)`
- Callbacks receive mutable `UI` reference and immutable `View` reference
- Focus management handled by containers via `focus_next()` / `focus_prev()`

### Type System

- `Element` = `Rc<RefCell<dyn View>>` - shared ownership with interior mutability
- `WeakElement` = weak reference for parent pointers to avoid cycles
- Use `downcast_mut::<ConcreteType>()` to access view-specific methods
- All state mutations go through `RefCell::borrow_mut()`

## Common Patterns

### Accessing Views from Callbacks

```rust
fn button_click(ui: &mut UI, view: &dyn View) -> bool {
    // Access another view by ID
    if let Some(edit) = ui.get_view("edit1") {
        if let Some(e) = edit.borrow_mut().downcast_mut::<Edit>() {
            e.set_text("Updated");
        }
    }

    // Access the clicked view
    if let Some(button) = view.as_any().downcast_ref::<Button>() {
        button.set_text("Clicked!");
    }

    true // Return true to request redraw
}
```

### Creating New View Types

1. Implement `View` trait with all required methods
2. Implement `Default` for factory construction
3. Register in `UI::new()` via `ui.register::<MyView>("MyView")`
4. Add to match statement in XML parser if needed

### Working with Typefaces

Fonts cascade from parent to child. Set at container level to apply to all children:
```xml
<Frame font="Akkurat" font_style="Bold">
    <Button text="Uses Akkurat Bold"/>
</Frame>
```

## Testing Notes

- Drawing registry tests are in `src/drawing/registry.rs` and pass
- `src/tests.rs` is a placeholder for future integration tests
- The software backend enables **headless render testing** without a window:
  `render::render_to_pixmap(..)` → `tiny_skia::Pixmap` (see `tests/software_render.rs`
  and `examples/headless_render.rs`). This sidesteps the old need to mock `Graphics2D`.

## Rust Edition

Project uses Rust 2024 edition (specified in Cargo.toml).