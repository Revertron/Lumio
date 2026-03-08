# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Lumio is a Rust GUI library built on top of `speedy2d` that provides a declarative XML-based layout system for creating desktop applications. It uses a retained-mode GUI architecture where view hierarchies are maintained in memory, with support for theming and event handling.

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

# Run linter
cargo clippy

# Run linter with auto-fix
cargo clippy --fix
```

## Architecture

### Core Components

- **UI System** (`src/ui.rs`): The main UI structure that manages view hierarchies, XML parsing, and view registration. Uses a factory pattern via `register<T>()` to create views dynamically from XML.

- **View Trait** (`src/traits.rs`): Central trait that all UI elements implement. Defines the contract for layout, painting, event handling, and state management. Uses `downcast-rs` for runtime type casting.

- **Win Handler** (`src/win.rs`): Window event handler implementing `speedy2d::WindowHandler`. Manages the render loop, input events, and coordinates between the window system and UI. Spawns a background thread for 60fps update events.

- **Theme System** (`src/themes/`): Pluggable theming via the `Theme` trait. Currently, implements `Classic` theme. Handles rendering of different view states (focused, hovered, pressed, etc.).

- **Prelude** (`src/prelude.rs`): Convenience re-exports for common types. Use `use lumio::prelude::*;` for quick access.

### View Hierarchy

Views are organized in a retained parent-child tree structure:
- **Container** (`src/containers.rs`): `Frame` is the primary container that can hold child views. Supports vertical/horizontal layout with optional line breaking.
- **Views** (`src/views/`): Concrete implementations include `Label`, `Button`, `Edit`, `CheckBox`, `List`, and `RecyclerView`.

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
- `src/tests.rs` is a placeholder for future integration tests (requires mocking `speedy2d::Graphics2D`)

## Rust Edition

Project uses Rust 2021 edition (specified in Cargo.toml).