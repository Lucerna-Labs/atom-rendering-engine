//! pmre-orchestrator — the single orchestrator. ALL policy, no mechanism.
//!
//! It owns draw order, the empty-slot check, the interaction state machine (hover / press /
//! click / toggle / scroll), and resize. It drives `pmre-kit`; it never rasterizes a pixel
//! itself. Two render paths sit on the same kit: `render`/`render_uxi`/`render_html` for
//! static frames, and the stateful `render_ui` + `handle_event` for interactive UIs.

use std::collections::HashMap;

use pmre_kit::{
    atoms,
    framebuffer::Framebuffer,
    geom::{Affine, Vec2},
    html,
    layout::{self, LaidBox, Painted},
    paint::{Bounds, Paint, Rgba, Shape},
    raster, text,
    ux::{Role, UxNode},
    DrawCmd,
};

// ----------------------------------------------------------------------------
// Static shape scene (used by the SDF demo)
// ----------------------------------------------------------------------------

/// A draw command plus its painter depth (`z`): larger `z` is nearer / drawn on top.
pub struct Item {
    pub z: f32,
    pub cmd: DrawCmd,
}

/// An ordered set of draw commands plus the surface to render them onto.
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub clear: Rgba,
    pub items: Vec<Item>,
}

impl Scene {
    pub fn new(width: u32, height: u32, clear: Rgba) -> Self {
        Self {
            width,
            height,
            clear,
            items: Vec::new(),
        }
    }
    pub fn push(&mut self, z: f32, cmd: DrawCmd) {
        self.items.push(Item { z, cmd });
    }
}

/// Render the scene with the painter's algorithm (the `order` atom, back-to-front).
pub fn render(scene: &Scene) -> Framebuffer {
    let mut fb = Framebuffer::new(scene.width, scene.height, scene.clear);
    for i in atoms::order(&scene.items, |it| -it.z) {
        let item = &scene.items[i];
        if item.cmd.shape.is_degenerate() {
            continue;
        }
        raster::scan_convert(&item.cmd, &mut fb, None);
    }
    fb
}

// ----------------------------------------------------------------------------
// Shared box-tree painting
// ----------------------------------------------------------------------------

fn paint_boxes(fb: &mut Framebuffer, boxes: &[LaidBox]) {
    let mut cmds: Vec<DrawCmd> = Vec::new();
    for laid in boxes {
        match &laid.kind {
            Painted::Box { .. } => {
                cmds.clear();
                layout::cmds_for(laid, &mut cmds);
                for cmd in &cmds {
                    if !cmd.shape.is_degenerate() {
                        raster::scan_convert(cmd, fb, laid.clip);
                    }
                }
            }
            Painted::Text {
                content,
                size,
                color,
            } => {
                let max_w = laid.rect.max.x - laid.rect.min.x;
                let line_h = *size * 1.3;
                let mut y = laid.rect.min.y;
                for line in text::wrap(content, *size, max_w) {
                    let origin = Vec2::new(laid.rect.min.x, y + (line_h - *size).max(0.0) * 0.5);
                    text::draw(fb, &line, origin, *size, *color, laid.clip);
                    y += line_h;
                }
            }
        }
    }
}

fn viewport(w: u32, h: u32) -> Bounds {
    Bounds {
        min: Vec2::new(0.0, 0.0),
        max: Vec2::new(w as f32, h as f32),
    }
}

/// Render a UXI tree (no interaction). Reduced layout → identical raster path.
pub fn render_uxi(root: &UxNode, width: u32, height: u32, clear: Rgba) -> Framebuffer {
    let mut fb = Framebuffer::new(width, height, clear);
    let boxes = layout::solve(root, viewport(width, height), &|_| 0.0);
    paint_boxes(&mut fb, &boxes);
    fb
}

/// Render an HTML/CSS document: the reduced front-end parses it into the same box tree.
pub fn render_html(src: &str, width: u32, height: u32, clear: Rgba) -> Framebuffer {
    render_uxi(&html::parse(src), width, height, clear)
}

// ----------------------------------------------------------------------------
// Interactive UI: state, events, stateful render
// ----------------------------------------------------------------------------

/// All UI interaction state. The app's `build(&UiState) -> UxNode` reads this to style
/// widgets (hover/press/toggle) so the tree always reflects current state.
#[derive(Default)]
pub struct UiState {
    pub width: u32,
    pub height: u32,
    pub hover: Option<u32>,
    pub pressed: Option<u32>,
    pub clicked: Option<u32>,
    pub toggles: HashMap<u32, bool>,
    pub scrolls: HashMap<u32, f32>,
    /// Scroll region whose scrollbar thumb is currently being dragged.
    pub drag: Option<u32>,
}

impl UiState {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }
    pub fn is_hover(&self, id: u32) -> bool {
        self.hover == Some(id)
    }
    pub fn is_pressed(&self, id: u32) -> bool {
        self.pressed == Some(id)
    }
    pub fn toggle_on(&self, id: u32) -> bool {
        self.toggles.get(&id).copied().unwrap_or(false)
    }
    pub fn scroll_of(&self, id: u32) -> f32 {
        self.scrolls.get(&id).copied().unwrap_or(0.0)
    }
    /// True exactly once for the widget clicked on the most recent PointerUp.
    pub fn take_click(&mut self) -> Option<u32> {
        self.clicked.take()
    }
}

/// Pointer / window events fed to `handle_event`.
pub enum UiEvent {
    Resize(u32, u32),
    PointerMove(f32, f32),
    PointerDown(f32, f32),
    PointerUp(f32, f32),
    /// Vertical wheel: cursor position and a positive-down delta in pixels.
    Wheel(f32, f32, f32),
}

fn solve_for(build: &dyn Fn(&UiState) -> UxNode, state: &UiState) -> Vec<LaidBox> {
    let tree = build(state);
    layout::solve(&tree, viewport(state.width, state.height), &|id| {
        state.scroll_of(id)
    })
}

fn rect_contains(b: Bounds, x: f32, y: f32) -> bool {
    x >= b.min.x && x < b.max.x && y >= b.min.y && y < b.max.y
}

/// Scrollbar track + thumb geometry for a scroll region: `(bar_x, track_top, track_h,
/// thumb_y, thumb_h, max_scroll)`. `None` when there is nothing to scroll.
fn scrollbar_geom(b: &LaidBox, scroll: f32) -> Option<(f32, f32, f32, f32, f32, f32)> {
    if b.role != Role::Scroll {
        return None;
    }
    let view_h = b.rect.max.y - b.rect.min.y;
    let max = (b.content_len - view_h).max(0.0);
    if max <= 0.0 {
        return None;
    }
    let track_top = b.rect.min.y + 4.0;
    let track_h = (view_h - 8.0).max(1.0);
    let bar_x = b.rect.max.x - 7.0;
    let thumb_h = (view_h / b.content_len * track_h).clamp(24.0, track_h);
    let t = (scroll / max).clamp(0.0, 1.0);
    let thumb_y = track_top + t * (track_h - thumb_h);
    Some((bar_x, track_top, track_h, thumb_y, thumb_h, max))
}

/// The solved rectangle of the box with the given id under the current state.
/// Useful for placing synthetic events and for tests.
pub fn widget_rect(build: &dyn Fn(&UiState) -> UxNode, state: &UiState, id: u32) -> Option<Bounds> {
    solve_for(build, state)
        .into_iter()
        .find(|b| b.id == Some(id))
        .map(|b| b.rect)
}

/// Advance the interaction state machine by one event. `build` produces the current tree.
pub fn handle_event(state: &mut UiState, build: &dyn Fn(&UiState) -> UxNode, ev: UiEvent) {
    match ev {
        UiEvent::Resize(w, h) => {
            state.width = w;
            state.height = h;
        }
        UiEvent::PointerMove(x, y) => {
            if let Some(id) = state.drag {
                let boxes = solve_for(build, state);
                if let Some(b) = boxes.iter().find(|b| b.id == Some(id)) {
                    if let Some((_bx, track_top, track_h, _ty, thumb_h, max)) =
                        scrollbar_geom(b, state.scroll_of(id))
                    {
                        let denom = (track_h - thumb_h).max(1e-3);
                        let t = ((y - track_top - thumb_h * 0.5) / denom).clamp(0.0, 1.0);
                        state.scrolls.insert(id, t * max);
                    }
                }
                return;
            }
            let boxes = solve_for(build, state);
            state.hover = layout::hit_test(&boxes, x, y).map(|(id, _)| id);
        }
        UiEvent::PointerDown(x, y) => {
            let boxes = solve_for(build, state);
            state.drag = None;
            for b in &boxes {
                let Some(id) = b.id else { continue };
                if let Some((bar_x, _tt, _th, thumb_y, thumb_h, _max)) =
                    scrollbar_geom(b, state.scroll_of(id))
                {
                    if x >= bar_x - 4.0
                        && x <= bar_x + 8.0
                        && y >= thumb_y
                        && y <= thumb_y + thumb_h
                    {
                        state.drag = Some(id);
                    }
                }
            }
            if state.drag.is_some() {
                state.pressed = None;
            } else {
                state.pressed = layout::hit_test(&boxes, x, y).map(|(id, _)| id);
            }
        }
        UiEvent::PointerUp(x, y) => {
            if state.drag.is_some() {
                state.drag = None;
                state.pressed = None;
                return;
            }
            let boxes = solve_for(build, state);
            state.clicked = None;
            if let (Some((up_id, role)), Some(pressed)) =
                (layout::hit_test(&boxes, x, y), state.pressed)
            {
                if up_id == pressed {
                    state.clicked = Some(up_id);
                    if role == Role::Toggle {
                        let now = state.toggle_on(up_id);
                        state.toggles.insert(up_id, !now);
                    }
                }
            }
            state.pressed = None;
        }
        UiEvent::Wheel(x, y, delta) => {
            let boxes = solve_for(build, state);
            // Topmost scroll region under the cursor.
            let mut target: Option<(u32, f32, f32)> = None;
            for b in &boxes {
                if b.role == Role::Scroll && rect_contains(b.rect, x, y) {
                    if let Some(id) = b.id {
                        target = Some((id, b.rect.max.y - b.rect.min.y, b.content_len));
                    }
                }
            }
            if let Some((id, view_h, content_len)) = target {
                let max = (content_len - view_h).max(0.0);
                let next = (state.scroll_of(id) + delta).clamp(0.0, max);
                state.scrolls.insert(id, next);
            }
        }
    }
}

/// Render the interactive UI for the current state, including scrollbars.
pub fn render_ui(build: &dyn Fn(&UiState) -> UxNode, state: &UiState, clear: Rgba) -> Framebuffer {
    let mut fb = Framebuffer::new(state.width, state.height, clear);
    let boxes = solve_for(build, state);
    paint_boxes(&mut fb, &boxes);
    draw_scrollbars(&mut fb, &boxes, state);
    fb
}

fn draw_scrollbars(fb: &mut Framebuffer, boxes: &[LaidBox], state: &UiState) {
    for b in boxes {
        let Some(id) = b.id else { continue };
        if let Some((bar_x, track_top, track_h, thumb_y, thumb_h, _max)) =
            scrollbar_geom(b, state.scroll_of(id))
        {
            let thumb_col = if state.drag == Some(id) {
                Rgba::rgb8(150, 160, 185)
            } else {
                Rgba::rgb8(120, 130, 150)
            };
            fill_rect(
                fb,
                bar_x,
                track_top,
                4.0,
                track_h,
                Rgba::rgb8(48, 52, 64),
                2.0,
            );
            fill_rect(fb, bar_x, thumb_y, 4.0, thumb_h, thumb_col, 2.0);
        }
    }
}

fn fill_rect(fb: &mut Framebuffer, x: f32, y: f32, w: f32, h: f32, color: Rgba, radius: f32) {
    let cmd = DrawCmd {
        shape: Shape::RoundedRect {
            half: Vec2::new(w / 2.0, h / 2.0),
            radius,
        },
        paint: Paint::Solid(color),
        transform: Affine::translate(x + w / 2.0, y + h / 2.0),
    };
    raster::scan_convert(&cmd, fb, None);
}
