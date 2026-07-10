# Atom Rendering Engine Integration Guide

This guide explains how to consume Atom Rendering Engine from another Rust application. For
engine internals and extension work, see the [Developer Guide](docs/DEVELOPER_GUIDE.md).

> **Status:** Atom Rendering Engine (2D) and the companion Atom 3D Engine are both functional,
> runnable engines today. Both remain under active development while their APIs, performance,
> platform adapters, tools, and other details are refined. Treat the current APIs as usable but
> not frozen, and pin a tested commit for production work.

## 1. Choose the integration surface

The repository contains two crates:

| Crate | Use it for |
|---|---|
| `pmre-kit` | Colors, geometry, draw commands, paths, text, layout intent, and framebuffer access |
| `pmre-orchestrator` | Scene rendering, UXI and HTML rendering, interaction state, event routing, DPI-aware rendering, and quality tiers |

Most applications should depend on both crates. Use only `pmre-kit` when you deliberately want
the low-level mechanisms and will supply all orchestration yourself.

The default configuration has no external crate dependencies. The optional `gpu` feature on
`pmre-orchestrator` adds `wgpu` and `pollster` for GPU bloom; normal 2D rendering remains CPU-based.

## 2. Add the dependencies

### Local checkout

Place the engine beside your application and use path dependencies:

```toml
[dependencies]
pmre-kit = { path = "../atom-rendering-engine/pmre-kit" }
pmre-orchestrator = { path = "../atom-rendering-engine/pmre-orchestrator" }
```

### Git dependency

To follow current development directly from GitHub:

```toml
[dependencies]
pmre-kit = { git = "https://github.com/Lucerna-Labs/atom-rendering-engine.git", branch = "main" }
pmre-orchestrator = { git = "https://github.com/Lucerna-Labs/atom-rendering-engine.git", branch = "main" }
```

For a reproducible application build, replace `branch = "main"` with the tested revision:

```toml
pmre-kit = { git = "https://github.com/Lucerna-Labs/atom-rendering-engine.git", rev = "FULL_COMMIT_SHA" }
pmre-orchestrator = { git = "https://github.com/Lucerna-Labs/atom-rendering-engine.git", rev = "FULL_COMMIT_SHA" }
```

Both entries must point to the same revision. Cargo records the selected revision in
`Cargo.lock`; commit that lockfile for applications.

To enable GPU bloom, add `features = ["gpu"]` to the `pmre-orchestrator` entry. Do not enable it
just to display ordinary 2D frames.

## 3. Render a first frame

The UXI path is the shortest route to a laid-out 2D interface:

```rust
use pmre_kit::{Dim, Edges, Rgba, Style, UxNode};
use pmre_orchestrator::render_uxi;

fn main() {
    let background = Rgba::rgb8(9, 9, 11);
    let root = UxNode::boxed(
        Style::col()
            .w(Dim::Flex(1.0))
            .h(Dim::Flex(1.0))
            .pad(Edges::all(24.0))
            .gap(12.0)
            .bg(Rgba::rgb8(24, 24, 27)),
        vec![
            UxNode::text("Atom Rendering Engine", 26.0, Rgba::rgb8(250, 250, 250)),
            UxNode::text(
                "A functional 2D frame rendered from mathematical primitives.",
                15.0,
                Rgba::rgb8(161, 161, 170),
            ),
        ],
    );

    let frame = render_uxi(&root, 800, 450, background);
    std::fs::write("first-frame.bmp", frame.to_bmp(background))
        .expect("write first-frame.bmp");
}
```

Run the application normally. The built-in BMP encoder provides a dependency-free first
integration test before you wire the framebuffer into a native window or another surface.

## 4. Select an input model

All high-level paths return the same `pmre_kit::Framebuffer` type.

| Input | Entry point | Best fit |
|---|---|---|
| Ordered drawing primitives | `render(&Scene)` | Games, visualizations, custom drawing, and direct control |
| UXI intent tree | `render_uxi(&UxNode, width, height, clear)` | Static or application-generated layouts |
| Reduced HTML/CSS | `render_html(source, width, height, clear)` | HTML-like content without embedding a browser |
| Stateful UXI tree | `render_ui(build, &UiState, clear)` | Buttons, toggles, inputs, scrolling, and live windows |

### Direct scene rendering

Shapes use local coordinates. `Affine` places each shape in the frame, and `Scene::push` supplies
its painter depth:

```rust
use pmre_kit::{Affine, DrawCmd, Paint, Rgba, Shape, Vec2};
use pmre_orchestrator::{render, Scene};

let clear = Rgba::rgb8(18, 18, 26);
let mut scene = Scene::new(640, 360, clear);
scene.push(
    0.0,
    DrawCmd::new(
        Shape::RoundedRect {
            half: Vec2::new(120.0, 70.0),
            radius: 20.0,
        },
        Paint::Solid(Rgba::rgb8(80, 140, 250)),
        Affine::translate(320.0, 180.0),
    ),
);
let frame = render(&scene);
```

Use `pmre_kit::path::fill_cmds` and `stroke_cmds` for arbitrary contours and flattened Bezier
paths. Use the UXI path when layout should derive coordinates for you.

### HTML rendering

```rust
use pmre_kit::Rgba;
use pmre_orchestrator::render_html;

let source = r#"
<div style="display:flex; flex-direction:column; padding:24px; gap:10px;
            background:#18181b; border-radius:10px">
  <h2 style="color:#fafafa">Status</h2>
  <span style="color:#a1a1aa">Rendering through Atom.</span>
</div>
"#;
let clear = Rgba::rgb8(9, 9, 11);
let frame = render_html(source, 640, 360, clear);
```

This is a reduced HTML/CSS front end, not a web browser. It supports the documented box, flex,
paint, text, and color subset. It does not provide JavaScript, networking, a general CSS cascade,
or browser navigation. See [the HTML section of the Developer Guide](docs/DEVELOPER_GUIDE.md#5-the-htmlcss-front-end).

## 5. Integrate interaction

Your application owns domain state. The engine owns `UiState`, which tracks hover, press, click,
toggle, scroll, drag, focus, input text, resize, and DPI state.

The application loop is:

1. Build a `UxNode` tree from the current domain state and `UiState`.
2. Translate a platform event into `UiEvent` and call `handle_event`.
3. Consume one-shot actions such as `take_click()` and `take_submit()`.
4. Update domain state.
5. Call `render_ui` and present the returned framebuffer.

A minimal button follows this pattern:

```rust
use pmre_kit::{Align, Dim, Justify, Rgba, Style, UxNode};
use pmre_orchestrator::{handle_event, render_ui, UiEvent, UiState};

const SAVE: u32 = 1;

fn build(ui: &UiState) -> UxNode {
    let color = if ui.is_pressed(SAVE) {
        Rgba::rgb8(40, 80, 160)
    } else if ui.is_hover(SAVE) {
        Rgba::rgb8(80, 150, 250)
    } else {
        Rgba::rgb8(60, 120, 220)
    };

    UxNode::boxed(
        Style::row()
            .button(SAVE)
            .w(Dim::Px(120.0))
            .h(Dim::Px(42.0))
            .align(Align::Center)
            .justify(Justify::Center)
            .radius(8.0)
            .bg(color),
        vec![UxNode::text("SAVE", 14.0, Rgba::rgb8(255, 255, 255))],
    )
}

let clear = Rgba::rgb8(9, 9, 11);
let mut ui = UiState::new(800, 450);

// Feed these from the host window's event loop.
handle_event(&mut ui, &build, UiEvent::PointerMove(60.0, 21.0));
handle_event(&mut ui, &build, UiEvent::PointerDown(60.0, 21.0));
handle_event(&mut ui, &build, UiEvent::PointerUp(60.0, 21.0));

if ui.take_click() == Some(SAVE) {
    // Apply application state changes here.
}

let frame = render_ui(&build, &ui, clear);
```

Widget IDs are application-owned `u32` values and must be stable and unique within the current
tree. The full headless event flow is in
[`pmre-orchestrator/examples/ui.rs`](pmre-orchestrator/examples/ui.rs); text input and domain-state
updates are demonstrated in [`todo.rs`](pmre-orchestrator/examples/todo.rs).

## 6. Connect a platform event loop

The renderer does not require a particular window library. A host integration needs to provide:

- the current physical framebuffer width and height;
- the monitor scale factor;
- pointer, wheel, keyboard, and resize events;
- a way to copy the completed framebuffer to a visible surface.

Map host events as follows:

| Host event | Engine action |
|---|---|
| Window resized | `UiEvent::Resize(physical_width, physical_height)` |
| Pointer moved | `UiEvent::PointerMove(logical_x, logical_y)` |
| Primary button down/up | `UiEvent::PointerDown` / `UiEvent::PointerUp` |
| Vertical wheel | `UiEvent::Wheel(logical_x, logical_y, positive_down_pixels)` |
| Text input | `UiEvent::Char(character)` |
| Backspace | `UiEvent::Backspace` |
| Enter | `UiEvent::Enter` |

On Windows, [`pmre-orchestrator/examples/app.rs`](pmre-orchestrator/examples/app.rs) is a complete
raw Win32/GDI adapter with a live window, pointer capture, keyboard input, wheel handling, resize,
and per-monitor DPI changes. It is the reference wiring for a native shell.

On another platform or with another window toolkit, keep the same event/render sequence and swap
only the event translation and final framebuffer presentation.

## 7. Preserve the DPI contract

`UiState::width` and `height` are physical pixel dimensions. `UiState::scale` is physical pixels
per logical unit, normally `monitor_dpi / 96.0`.

```rust
ui.width = physical_width;
ui.height = physical_height;
ui.scale = monitor_scale.max(0.1);
```

Pointer coordinates passed to `UiEvent` must be logical:

```rust
let logical_x = physical_x / ui.scale;
let logical_y = physical_y / ui.scale;
```

Do not pre-scale the `UxNode` dimensions. The engine solves the tree in logical units and scales
the laid-out boxes and text back to physical pixels during rendering.

## 8. Present or export the framebuffer

`Framebuffer` exposes several handoff options:

| API | Format | Use |
|---|---|---|
| `pixels()` | Row-major straight-alpha `Rgba` values | Custom compositor, texture upload, or inspection |
| `pixels_mut()` | Mutable row-major `Rgba` values | Controlled bulk writes or readback integration |
| `to_u32(background)` | Opaque row-major `0x00RRGGBB` | Software/native window blit |
| `to_bmp(background)` | Complete 24-bit BMP byte stream | Files, tests, screenshots, and first-run validation |

`to_u32` and `to_bmp` flatten alpha over the background you pass. Use the same background used to
render the frame unless a different final composite is intentional.

For a GPU texture upload from `pixels()`, convert the normalized float channels into the byte
ordering and alpha convention required by the destination API. The raw pixels are straight-alpha,
not premultiplied-alpha.

## 9. Fonts

The engine searches for a system TrueType font and falls back to its built-in bitmap font when no
usable system font is available. To make font selection deterministic, set:

```text
PMRE_FONT=C:\path\to\Regular.ttf
PMRE_FONT_BOLD=C:\path\to\Bold.ttf
```

Package fonts only when their licenses permit redistribution. A missing or malformed configured
font falls back safely instead of making the renderer unusable.

## 10. Quality tiers and GPU bloom

Use `render_uxi_quality` or `render_ui_quality` when a post-processing tier is required. For normal
application UI, begin with `Quality::Fast`; it performs the core render without bloom.

The `GpuBalanced` and `GpuFull` variants require the `gpu` feature to use `wgpu`. Without that
feature, they deliberately fall back to CPU bloom. GPU bloom is optional and is not required for
the engine to be functional.

## 11. Update an integration safely

Both Atom engines are functional while refinement continues, so update deliberately:

1. Record the currently tested engine commit.
2. Update both engine crate dependencies to the same new commit.
3. Run your application tests and render smoke tests.
4. Inspect representative output frames, input handling, resizing, and DPI behavior.
5. Commit the resulting `Cargo.lock` only after validation passes.

For a branch-based Git dependency, fetch a newer revision with:

```sh
cargo update -p pmre-kit -p pmre-orchestrator
```

Confirm the resolved source with:

```sh
cargo tree -p pmre-kit
cargo tree -p pmre-orchestrator
```

Pin the validated commit before distributing a production build.

## 12. Validation checklist

Before considering an integration complete, verify:

- the application builds without accidentally enabling the optional GPU tier;
- a static frame renders at the expected dimensions;
- colors and alpha flatten correctly on the host surface;
- pointer coordinates use logical units at non-100-percent display scaling;
- resize events update physical dimensions;
- clicks fire once and input focus routes characters correctly;
- scroll regions wheel and drag correctly;
- the chosen regular and bold fonts render or fall back as expected;
- release builds pin the tested engine revision.

Run the engine's own validation from its checkout:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo run -p pmre-orchestrator --example todo
```

The final command is self-verifying: it drives text entry, button clicks, a toggle operation, and
rendering before writing `todo.bmp`.

## 13. Example map

| Example | What it demonstrates |
|---|---|
| `demo` | Direct SDF scene commands and painter order |
| `paths` | Filled contours, holes, and Bezier flattening |
| `stroke` | Path stroking, joins, and caps |
| `gradients` | Linear and radial paint |
| `uxi` | Static UXI layout |
| `html` | Reduced HTML/CSS rendering |
| `ui` | Headless interaction, scrolling, and resize |
| `todo` | Self-verifying text input and domain-state integration |
| `app` | Live raw Win32/GDI window integration |

Run an example from the engine checkout with:

```sh
cargo run -p pmre-orchestrator --example NAME
```
