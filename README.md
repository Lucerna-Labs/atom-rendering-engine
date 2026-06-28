# Primitive Math Rendering Engine

A from-scratch 2D UI rendering engine built **entirely from mathematical primitives** —
no GPU vector library (no Vello, Skia, or Cairo), no web engine, no Tauri/Electron. Shapes
are rasterized from signed-distance fields with analytic anti-aliasing, text from a bitmap
font, layout from a reduced flexbox/box-model solver, and the whole thing composites and
runs an interactive widget loop on the CPU.

![screenshot](docs/screenshot.png)

## The shape of it

Two crates, and only two, per the Composition doctrine:

- **`pmre-kit`** — *the kit*: all the dumb, decision-free mechanism. The eight root atoms
  (`scan · hash · fold · project · scale · compare · combine · order`), geometry + affine
  transforms, SDF coverage + smoothstep anti-aliasing, alpha-over compositing, a bitmap
  glyph rasterizer, the reduced layout solver, hit-testing, and clipping. Zero dependencies.
- **`pmre-orchestrator`** — *the orchestrator*: all policy, no mechanism. Painter order, the
  interaction state machine (hover / press / click / toggle / scroll), scrollbars, and the
  resize loop. It drives the kit; it never touches a pixel itself. Zero runtime dependencies.

The rendering pipeline is pure composition all the way to pixels:

```
intent (UxNode, no coordinates)  ─┐
HTML + reduced CSS  ──────────────┼─►  reduced layout (box-model + block/flex)
                                       └─►  drawing primitives (the "math")
                                            └─►  SDF coverage + alpha-over  ─►  framebuffer
```

Every rendering step maps onto a root atom: `project` transforms points, `compare` is the
SDF distance, `smoothstep` is the anti-aliased coverage, `combine` is Porter-Duff *over*,
`order` is the painter's algorithm.

## What it does

- **Shapes** — rect, rounded-rect, circle, line, all via signed-distance fields with
  exact analytic anti-aliasing.
- **Text** — a built-in bitmap font, with word wrapping and clipping.
- **Layout** — a reduced CSS-flexbox/block solver: row/column, `Auto`/`Px`/`Flex` sizing,
  padding, gap, align, justify, borders, radius. Author intent; positions are *derived*.
- **Two front-ends, one core** — a UXI intent tree and an HTML/CSS document reduce onto the
  same box-model + layout + paint core.
- **Interaction** — buttons (hover / press / click), toggles, a scrollable region with
  clipping and a live scrollbar, hit-testing, and auto-resize reflow.
- **Live window** — a winit + softbuffer runner that blits the CPU framebuffer straight to
  the screen with real mouse, wheel, and resize events.

## Build & run

```sh
# Static renders
cargo run -p pmre-orchestrator --example demo   # SDF shapes
cargo run -p pmre-orchestrator --example uxi    # a UXI dashboard
cargo run -p pmre-orchestrator --example html   # HTML/CSS reduced to primitives

# Interaction, driven headlessly to image frames
cargo run -p pmre-orchestrator --example ui

# Live interactive window (real mouse / wheel / resize)
cargo run -p pmre-orchestrator --example app
```

Only the `app` example pulls in dependencies (`winit`, `softbuffer`) for OS windowing and
CPU presentation. The library crates stay dependency-free pure math.
