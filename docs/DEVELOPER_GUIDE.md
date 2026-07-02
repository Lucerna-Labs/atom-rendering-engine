# PMRE Developer Guide

How to embed, drive, and extend the Primitive Math Rendering Engine.

PMRE is a zero-dependency 2D UI engine: two crates, all CPU, everything built from a
small set of mathematical primitives. This guide is for developers building apps on the
engine or extending the engine itself. For the project overview, see the
[README](../README.md).

---

## 1. Architecture in one minute

```
pmre-kit           mechanism only — no decisions
├─ geom            Vec2, Affine (project atom)
├─ paint           Rgba, Shape, Paint, DrawCmd, Porter-Duff over (combine atom)
├─ raster          SDF coverage rasterizer with analytic anti-aliasing
├─ path            scanline polygon/Bézier filler + stroker (nonzero winding)
├─ font            TrueType parser + AA glyph rasterizer (+ 5×7 bitmap fallback)
├─ text            text runs: advance / wrap / draw onto any Surface
├─ ux              UxNode intent tree: Style, Span, no coordinates
├─ layout          box-model + flex solver: intent → LaidBox rects → DrawCmds
├─ framebuffer     Framebuffer, Surface trait, BandView (row-band for lanes)
└─ post            bloom + parallel/tiled variants

pmre-orchestrator  policy only — never touches a pixel directly
└─ lib             draw order, banded parallel painting, interaction state
                   machine (hover/press/click/toggle/scroll/focus), DPI scaling,
                   scrollbars, Quality tiers
```

The rule that keeps the design honest: **if a `pmre-kit` function grows an `if` that
makes a value judgement, that `if` belongs in the orchestrator.** The kit computes;
the orchestrator decides.

Both crates have **zero external dependencies**. Do not add any. Fonts come from the
OS font directory via `std::fs`; the window in `examples/app.rs` is raw Win32 FFI.

---

## 2. Getting started

Workspace layout — add the crates by path (they are not on crates.io):

```toml
[dependencies]
pmre-kit = { path = "../primitive-math-rendering-engine/pmre-kit" }
pmre-orchestrator = { path = "../primitive-math-rendering-engine/pmre-orchestrator" }
```

Render your first frame:

```rust
use pmre_kit::{Dim, Edges, Rgba, Style, UxNode};
use pmre_orchestrator::render_uxi;

fn main() {
    let ui = UxNode::boxed(
        Style::col()
            .w(Dim::Flex(1.0))
            .h(Dim::Flex(1.0))
            .pad(Edges::all(24.0))
            .gap(12.0)
            .bg(Rgba::rgb8(24, 24, 27)),
        vec![
            UxNode::text("Hello from PMRE", 24.0, Rgba::rgb8(250, 250, 250)),
            UxNode::text("laid out, rasterized, and composited on the CPU",
                         14.0, Rgba::rgb8(161, 161, 170)),
        ],
    );
    let fb = render_uxi(&ui, 640, 360, Rgba::rgb8(9, 9, 11));
    std::fs::write("hello.bmp", fb.to_bmp(Rgba::rgb8(9, 9, 11))).unwrap();
}
```

`Framebuffer::to_bmp` needs no image crate — the BMP encoder is built in.
`Framebuffer::to_u32` produces `0x00RRGGBB` pixels ready for any OS blit
(`StretchDIBits`, softbuffer-style surfaces, etc.).

---

## 3. Building UIs: `UxNode` + `Style`

A UI is a tree of intent with **no coordinates** — the layout solver derives every
position. Three node kinds:

| Node | What it is |
|---|---|
| `UxNode::Box { style, children }` | a styled flex container |
| `UxNode::Text { content, size, color }` | one plain text run (wraps on its own) |
| `UxNode::Rich { spans, align }` | inline flow: mixed bold/underline/color/size spans wrap **together** like an HTML paragraph |

`Style` is a builder:

```rust
Style::row()                       // main axis: Row or Column (Style::col())
    .w(Dim::Px(240.0))             // Auto | Px(f32) | Flex(weight) | Pct(0..100)
    .h(Dim::Auto)
    .pad(Edges::all(12.0))         // padding (also Edges::xy(x, y) / per-side struct)
    .margin(Edges::xy(0.0, 8.0))   // margin — outside the border box
    .gap(8.0)                      // space between children on the main axis
    .align(Align::Center)          // cross-axis: Start | Center | End | Stretch
    .justify(Justify::SpaceBetween)// main-axis: Start | Center | End | SpaceBetween
    .bg(Rgba::rgb8(30, 30, 36))
    .radius(10.0)                  // rounded corners
    .border(1.0, Rgba::rgb8(63, 63, 70))
    .shadow(0.0, 4.0, 14.0, Rgba::new(0.0, 0.0, 0.0, 0.35)) // dx, dy, blur, color
    .button(MY_ID)                 // interaction role (see §6)
```

Sizing semantics (reduced CSS flexbox):

- `Auto` — intrinsic content size (text measures with real font metrics and wraps to
  the available width).
- `Px(v)` — fixed border-box size; margins add outside.
- `Flex(w)` — `flex-basis: 0; flex-grow: w`: children share leftover main-axis space
  by weight.
- `Pct(p)` — percentage of the parent's content extent on that axis.

Rich text:

```rust
use pmre_kit::Span;
UxNode::rich(vec![
    Span::new("Deploy ", 14.0, text_color),
    Span::new("failed", 14.0, red).bold(),
    Span::new(" — see the ", 14.0, text_color),
    Span::new("logs", 14.0, blue).underline(),
])
```

All spans flow through one greedy word-wrapper and share a common baseline per line,
so mixed sizes/weights sit on the same line correctly.

---

## 4. Text and fonts

Two tiers, selected automatically at first use:

1. **Vector tier** — `pmre_kit::font` finds a system `.ttf` (Segoe UI → Arial →
   Tahoma → Calibri → Verdana on Windows; DejaVu/Liberation on Linux; Arial/Helvetica
   on macOS), parses it, and rasterizes anti-aliased glyphs with a font-rs-style
   accumulation buffer. Glyph bitmaps are cached per `(glyph, quarter-px size)`.
2. **Bitmap tier** — the built-in 5×7 pixel font when no font file exists (containers,
   bare CI images). Everything still renders.

Overrides: set `PMRE_FONT` / `PMRE_FONT_BOLD` to explicit `.ttf`/`.ttc` paths.

Useful APIs (`pmre_kit::text`):

- `advance(str, size) -> f32` / `advance_styled(str, size, bold)` — run width.
- `wrap(str, size, max_width) -> Vec<String>` — greedy word wrap, O(n).
- `v_metrics(size) -> (ascent, descent)` — for baseline math.
- `draw(surface, str, origin, size, color, clip)` — origin is the top of the ascent
  box; the baseline lands at `origin.y + ascent`.

Malformed font files degrade to the bitmap tier — parsing is fully bounds-checked and
glyph rasterization caps its bitmap size; it never panics or aborts.

---

## 5. The HTML/CSS front-end

`pmre_orchestrator::render_html(src, w, h, clear)` (or `pmre_kit::html::parse(src)`
for the tree) reduces an HTML fragment with **inline `style` attributes** onto the same
`UxNode` vocabulary. There is no selector engine and no external stylesheet — the box
model and property subset are the load-bearing core; the cascade is deliberately out of
scope.

Structure handled: comments, doctype, entities (`&amp;` `&#x2014;` …), `<script>` /
`<style>` content skipping, void tags, malformed-input hardening (depth caps, no
quadratic scans).

| Kind | Supported |
|---|---|
| Block tags | `div p h1–h4 ul ol li hr section header footer main nav article` |
| Inline tags (coalesce into one flow) | `b strong i em u a span small code mark br` |
| Layout CSS | `display(flex\|block\|none)`, `flex-direction`, `flex`, `flex-grow`, `width`/`height` (px/%/auto), `padding`/`margin` (+ per-side, 1–4 value shorthand), `gap`, `align-items`, `justify-content` |
| Paint CSS | `background(-color)`, `border`, `border-radius`, `box-shadow`, `opacity` |
| Text CSS | `color`, `font-size`, `font-weight`, `text-align`, `text-decoration` |
| Colors | `#rgb #rgba #rrggbb #rrggbbaa`, `rgb()/rgba()`, `hsl()/hsla()`, ~45 named |

Notes that differ from a browser:

- Inline elements coalesce into a `Rich` flow **unless** the parent is a
  `display:flex` row — there, per CSS, each child is its own flex item and `gap`
  applies between them.
- `<a>` renders underlined + link-blue but is not clickable by itself; give a
  surrounding box an interaction role if you need clicks.
- No italics (no synthetic shear yet) — `i`/`em` currently render upright.

Run `cargo run -p pmre-orchestrator --example html` to see the subset exercised.

---

## 6. Interactive apps

The orchestrator owns an immediate-mode-flavored loop: your app holds domain state and
a `build(&UiState) -> UxNode` function; the engine owns `UiState` (hover, press,
focus, scroll positions) and replays it into your build function every event.

```rust
use pmre_orchestrator::{handle_event, render_ui, UiEvent, UiState};

const SAVE: u32 = 1;
const LIST: u32 = 2;

fn build(ui: &UiState) -> UxNode {
    let save_bg = if ui.is_pressed(SAVE) { pressed_col }
                  else if ui.is_hover(SAVE) { hover_col }
                  else { normal_col };
    UxNode::boxed(Style::col().gap(8.0), vec![
        UxNode::boxed(Style::row().button(SAVE).bg(save_bg) /* … */, vec![/* … */]),
        UxNode::boxed(Style::col().scroll(LIST).h(Dim::Flex(1.0)), rows()),
    ])
}

// event loop:
handle_event(&mut ui_state, &build, UiEvent::PointerMove(x, y));
if ui_state.take_click() == Some(SAVE) { /* domain logic */ }
let fb = render_ui(&build, &ui_state, clear);
```

Roles (`Style::interactive`, or the `button/toggle/scroll/input` shorthands):

- **Button** — `is_hover` / `is_pressed` for styling; `take_click()` fires once per
  completed press+release on the same widget.
- **Toggle** — engine flips `toggle_on(id)` on click.
- **Scroll** — the box clips and scrolls its children; wheel and scrollbar-thumb drag
  (with grab offset) are handled for you; offsets re-clamp automatically when content
  shrinks.
- **Input** — click to focus; feed `UiEvent::Char/Backspace/Enter`; read
  `input_text(id)`, `take_submit()`.

### DPI contract (important)

- `UiState.width/height` are **physical** pixels; `UiState.scale` is the device pixel
  ratio (`dpi / 96`).
- Layout solves in **logical** units (`width / scale`); painting multiplies back up,
  so glyphs rasterize at native resolution — never bitmap-stretched.
- **Feed pointer events in logical units** (divide OS mouse coordinates by `scale`).
- `examples/app.rs` shows the full Win32 wiring: `SetProcessDpiAwarenessContext`,
  `WM_DPICHANGED`, mouse capture, `TrackMouseEvent` for hover-clear on window leave.

### Post-processing

`render_ui_quality` / `render_uxi_quality` take a `Quality` tier: `Fast` (no post),
CPU / parallel / cache-tiled bloom, or GPU bloom (feature-gated; falls back to CPU).
For typical UI work use `Fast` — the tiers exist to exercise the lane/bus dispatch
work; run `--example sweep` and `--example bench` for the numbers.

---

## 7. Drawing primitives directly

Skip the UI layer entirely when you just need shapes:

```rust
use pmre_kit::{Affine, DrawCmd, Paint, Rgba, Shape, Vec2};
use pmre_orchestrator::{render, Scene};

let mut scene = Scene::new(640, 360, Rgba::rgb8(18, 18, 26));
scene.push(0.0, DrawCmd::new(
    Shape::RoundedRect { half: Vec2::new(120.0, 70.0), radius: 20.0 },
    Paint::Linear { from: Vec2::new(-120.0, -70.0), to: Vec2::new(120.0, 70.0),
                    c0: Rgba::rgb8(80, 140, 250), c1: Rgba::rgb8(175, 80, 230) },
    Affine::translate(320.0, 180.0),
));
let fb = render(&scene); // painter's algorithm by z
```

- Shapes are **SDF-defined in local space**; the `Affine` places them. Anti-aliasing
  is analytic — one smoothstep over the signed distance, no supersampling.
- `DrawCmd.soft` widens the AA band: `0.0` is a crisp edge, larger values give the
  smooth falloff used for drop shadows and glows.
- For arbitrary contours (stars, glyph-like blobs, donuts) use `pmre_kit::path`:
  `fill_cmds` / `stroke_cmds` with `MoveTo/LineTo/Quad/Cubic/Close`, nonzero winding.

---

## 8. Engine invariants (read before changing the kit)

1. **Zero dependencies.** `cargo tree` must show no external crates for the library
   targets. OS access is limited to `std::fs` (fonts) and the examples' raw FFI.
2. **Banded determinism.** `paint_boxes_banded` splits the frame into row bands, one
   thread each, writing disjoint slices — output must be **bit-identical** to the
   serial render and independent of thread count (a test enforces this). If you add
   any paint that can touch pixels outside its box rect (shadow bleed, glyph
   overshoot, new effects), extend `paint_y_extent` in `pmre-orchestrator/src/lib.rs`
   or lanes will skip work and seam.
3. **Kit/orchestrator split.** New mechanism (a shape, a coverage generator, a post
   pass) goes in the kit; anything choosing *when/what/in-which-order* goes in the
   orchestrator.
4. **Malformed input degrades, never panics.** The font parser and HTML parser are
   fuzz-minded: bounds-checked reads, recursion caps, no unbounded allocation.
   Keep new parsing code to that standard.
5. **Logical vs physical units.** Only the paint step (and `draw_scrollbars`) knows
   about `scale`. Layout, hit-testing, and events stay logical.

### Adding a new SDF shape
1. Add the variant to `Shape` (`pmre-kit/src/paint.rs`) with local-space fields.
2. Implement its distance in `raster::signed_distance` and its box in
   `Shape::local_bounds` / `is_degenerate`.
3. Done — AA, paints, clipping, transforms, and banding all compose automatically.

### Adding a CSS property
1. Parse it in `apply_css` (`pmre-kit/src/html.rs`) into `Style`/`Inherited`.
2. If it needs new intent, extend `Style` (+ builder) in `ux.rs`.
3. Consume it in `layout.rs` (measure/solve) or `cmds_for` (paint).
4. Add a parser test in `html.rs` and, if it paints, eyeball
   `cargo run --example html`.

---

## 9. Testing & benchmarking

```sh
cargo test --workspace                          # unit + determinism tests
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p pmre-orchestrator --example bench --release   # ms/frame per quality tier
cargo run -p pmre-orchestrator --example html --release    # writes html.bmp
cargo run -p pmre-orchestrator --example app --release     # live Win32 window
```

Headless examples write `.bmp` files to the working directory (viewable everywhere,
zero encoder dependencies). The live window prints frame time + fps in its title bar;
keys `1–9` switch post-processing tiers.
