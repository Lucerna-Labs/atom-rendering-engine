//! The reduced layout core: box-model + block/flex solving, then box → draw commands,
//! plus interaction support (ids carried through to laid boxes, clip rects for scroll
//! regions, and hit-testing). Pure mechanism: deterministic, decision-free.
//! `solve` returns a flat pre-order list (parents before children = paint order).

use crate::geom::{Affine, Vec2};
use crate::paint::{Bounds, DrawCmd, Paint, Rgba, Shape};
use crate::ux::{Align, Dim, Dir, Edges, Justify, Role, Style, UxNode};

/// What a laid-out box paints as.
#[derive(Clone, Debug)]
pub enum Painted {
    Box {
        background: Option<Rgba>,
        radius: f32,
        border: Option<(f32, Rgba)>,
    },
    Text {
        content: String,
        size: f32,
        color: Rgba,
    },
}

/// A node with its solved device-space rectangle and interaction metadata.
#[derive(Clone, Debug)]
pub struct LaidBox {
    pub rect: Bounds,
    pub kind: Painted,
    pub id: Option<u32>,
    pub role: Role,
    /// Clip rectangle this box is confined to (set for descendants of a scroll region).
    pub clip: Option<Bounds>,
    /// For a `Scroll` box: the natural height of its content (for scrollbar + clamping).
    pub content_len: f32,
}

/// A scroll-offset lookup: given a scroll box id, return its current vertical offset.
pub type ScrollFn<'a> = dyn Fn(u32) -> f32 + 'a;

/// Solve layout for `root` inside `viewport`. `scroll` supplies each scroll region's offset.
pub fn solve(root: &UxNode, viewport: Bounds, scroll: &ScrollFn) -> Vec<LaidBox> {
    let mut out = Vec::new();
    layout_node(root, viewport, None, scroll, &mut out);
    out
}

/// Topmost interactive box containing the point (respecting clip), as `(id, role)`.
pub fn hit_test(boxes: &[LaidBox], x: f32, y: f32) -> Option<(u32, Role)> {
    let mut found = None;
    for b in boxes {
        let Some(id) = b.id else { continue };
        if !contains(b.rect, x, y) {
            continue;
        }
        if let Some(clip) = b.clip {
            if !contains(clip, x, y) {
                continue;
            }
        }
        found = Some((id, b.role)); // later in pre-order = drawn on top
    }
    found
}

fn contains(b: Bounds, x: f32, y: f32) -> bool {
    x >= b.min.x && x < b.max.x && y >= b.min.y && y < b.max.y
}

/// Text advance width — single source of truth shared with the glyph rasterizer.
pub fn text_width(content: &str, size: f32) -> f32 {
    crate::text::advance(content, size)
}

fn extent(b: Bounds) -> (f32, f32) {
    (b.max.x - b.min.x, b.max.y - b.min.y)
}

fn inset(rect: Bounds, p: Edges, border: f32) -> Bounds {
    Bounds {
        min: Vec2::new(rect.min.x + p.l + border, rect.min.y + p.t + border),
        max: Vec2::new(rect.max.x - p.r - border, rect.max.y - p.b - border),
    }
}

fn clip_to(parent: Option<Bounds>, inner: Bounds) -> Option<Bounds> {
    match parent {
        None => Some(inner),
        Some(p) => Some(Bounds {
            min: Vec2::new(p.min.x.max(inner.min.x), p.min.y.max(inner.min.y)),
            max: Vec2::new(p.max.x.min(inner.max.x), p.max.y.min(inner.max.y)),
        }),
    }
}

fn node_dim(node: &UxNode, want_width: bool) -> Dim {
    match node {
        UxNode::Box { style, .. } => {
            if want_width {
                style.width
            } else {
                style.height
            }
        }
        UxNode::Text { .. } => Dim::Auto,
    }
}

/// Intrinsic (content) size of a node. When `avail_w` is given, text wraps to it and the
/// returned height reflects the wrapped line count (so a column reserves the right height).
fn measure(node: &UxNode, avail_w: Option<f32>) -> (f32, f32) {
    match node {
        UxNode::Text { content, size, .. } => {
            let single = text_width(content, *size);
            let line_h = size * 1.3;
            match avail_w {
                Some(w) if w > 0.0 && single > w => {
                    let lines = crate::text::wrap(content, *size, w).len().max(1);
                    (w, lines as f32 * line_h)
                }
                _ => (single, line_h),
            }
        }
        UxNode::Box { style, children } => {
            let bw = style.border.map(|(w, _)| w).unwrap_or(0.0);
            let pad_w = style.padding.l + style.padding.r + 2.0 * bw;
            let pad_h = style.padding.t + style.padding.b + 2.0 * bw;
            let mut main = 0.0f32;
            let mut cross = 0.0f32;
            let n = children.len();
            for (i, ch) in children.iter().enumerate() {
                let (cw, chh) = measure(ch, None);
                let (cm, cc) = match style.dir {
                    Dir::Row => (cw, chh),
                    Dir::Column => (chh, cw),
                };
                main += cm;
                if i + 1 < n {
                    main += style.gap;
                }
                cross = cross.max(cc);
            }
            let (iw, ih) = match style.dir {
                Dir::Row => (main, cross),
                Dir::Column => (cross, main),
            };
            let w = match style.width {
                Dim::Px(v) => v,
                _ => iw + pad_w,
            };
            let h = match style.height {
                Dim::Px(v) => v,
                _ => ih + pad_h,
            };
            (w, h)
        }
    }
}

fn layout_node(
    node: &UxNode,
    rect: Bounds,
    clip: Option<Bounds>,
    scroll: &ScrollFn,
    out: &mut Vec<LaidBox>,
) {
    match node {
        UxNode::Text {
            content,
            size,
            color,
        } => {
            out.push(LaidBox {
                rect,
                kind: Painted::Text {
                    content: content.clone(),
                    size: *size,
                    color: *color,
                },
                id: None,
                role: Role::None,
                clip,
                content_len: 0.0,
            });
        }
        UxNode::Box { style, children } => {
            let bw = style.border.map(|(w, _)| w).unwrap_or(0.0);
            let content = inset(rect, style.padding, bw);

            if style.role == Role::Scroll {
                let (cw, _) = extent(content);
                let content_len = scroll_content_height(children, cw, style.gap);
                out.push(LaidBox {
                    rect,
                    kind: Painted::Box {
                        background: style.background,
                        radius: style.radius,
                        border: style.border,
                    },
                    id: style.id,
                    role: style.role,
                    clip,
                    content_len,
                });
                let inner_clip = clip_to(clip, rect);
                let off = style.id.map(scroll).unwrap_or(0.0);
                let mut cursor = content.min.y - off;
                for ch in children {
                    let (_, chh) = measure(ch, Some(cw));
                    let child_rect = Bounds {
                        min: Vec2::new(content.min.x, cursor),
                        max: Vec2::new(content.min.x + cw, cursor + chh),
                    };
                    layout_node(ch, child_rect, inner_clip, scroll, out);
                    cursor += chh + style.gap;
                }
            } else {
                out.push(LaidBox {
                    rect,
                    kind: Painted::Box {
                        background: style.background,
                        radius: style.radius,
                        border: style.border,
                    },
                    id: style.id,
                    role: style.role,
                    clip,
                    content_len: 0.0,
                });
                layout_children(style, children, content, clip, scroll, out);
            }
        }
    }
}

fn scroll_content_height(children: &[UxNode], width: f32, gap: f32) -> f32 {
    let n = children.len();
    let mut h = 0.0;
    for (i, ch) in children.iter().enumerate() {
        h += measure(ch, Some(width)).1;
        if i + 1 < n {
            h += gap;
        }
    }
    h
}

fn layout_children(
    style: &Style,
    children: &[UxNode],
    content: Bounds,
    clip: Option<Bounds>,
    scroll: &ScrollFn,
    out: &mut Vec<LaidBox>,
) {
    let n = children.len();
    if n == 0 {
        return;
    }
    let (cw, chh) = extent(content);
    let main_is_width = matches!(style.dir, Dir::Row);
    let (main_extent, cross_extent) = if main_is_width { (cw, chh) } else { (chh, cw) };
    let (main_start, cross_start) = if main_is_width {
        (content.min.x, content.min.y)
    } else {
        (content.min.y, content.min.x)
    };
    let avail_for_child = if main_is_width {
        None
    } else {
        Some(cross_extent)
    };

    let mut bases = Vec::with_capacity(n);
    let mut weights = Vec::with_capacity(n);
    let mut sum_base = 0.0f32;
    let mut sum_flex = 0.0f32;
    for ch in children {
        let (mw, mh) = measure(ch, avail_for_child);
        let measured_main = if main_is_width { mw } else { mh };
        let (base, weight) = match node_dim(ch, main_is_width) {
            Dim::Px(v) => (v, 0.0),
            Dim::Auto => (measured_main, 0.0),
            // `flex:N` is `flex-basis: 0` — size by grow share alone, so items fit (and shrink).
            Dim::Flex(w) => (0.0, w),
        };
        bases.push(base);
        weights.push(weight);
        sum_base += base;
        sum_flex += weight;
    }
    let gaps = style.gap * (n as f32 - 1.0);
    let free = (main_extent - sum_base - gaps).max(0.0);
    let mains: Vec<f32> = (0..n)
        .map(|i| {
            if sum_flex > 0.0 {
                bases[i] + free * (weights[i] / sum_flex)
            } else {
                bases[i]
            }
        })
        .collect();

    let leftover = if sum_flex > 0.0 { 0.0 } else { free };
    let (lead, between_extra) = justify_offsets(style.justify, leftover, n);

    let mut cursor = main_start + lead;
    for (i, ch) in children.iter().enumerate() {
        let cm = mains[i];
        let (mw, mh) = measure(ch, avail_for_child);
        let measured_cross = if main_is_width { mh } else { mw };
        let cc = match node_dim(ch, !main_is_width) {
            Dim::Px(v) => v,
            _ => {
                if matches!(style.align, Align::Stretch) {
                    cross_extent
                } else {
                    measured_cross
                }
            }
        };
        let cross_pos = align_pos(style.align, cross_start, cross_extent, cc);
        let rect = if main_is_width {
            Bounds {
                min: Vec2::new(cursor, cross_pos),
                max: Vec2::new(cursor + cm, cross_pos + cc),
            }
        } else {
            Bounds {
                min: Vec2::new(cross_pos, cursor),
                max: Vec2::new(cross_pos + cc, cursor + cm),
            }
        };
        layout_node(ch, rect, clip, scroll, out);
        cursor += cm + style.gap + between_extra;
    }
}

fn justify_offsets(j: Justify, free: f32, n: usize) -> (f32, f32) {
    match j {
        Justify::Start => (0.0, 0.0),
        Justify::Center => (free / 2.0, 0.0),
        Justify::End => (free, 0.0),
        Justify::SpaceBetween => {
            if n > 1 {
                (0.0, free / (n as f32 - 1.0))
            } else {
                (0.0, 0.0)
            }
        }
    }
}

fn align_pos(a: Align, cross_start: f32, cross_extent: f32, item_cross: f32) -> f32 {
    match a {
        Align::Start | Align::Stretch => cross_start,
        Align::Center => cross_start + (cross_extent - item_cross) / 2.0,
        Align::End => cross_start + (cross_extent - item_cross),
    }
}

fn center_half(r: Bounds) -> (Vec2, Vec2) {
    (
        Vec2::new((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0),
        Vec2::new((r.max.x - r.min.x) / 2.0, (r.max.y - r.min.y) / 2.0),
    )
}

/// Emit the draw commands for one laid-out box (background + border), appending to `out`.
pub fn cmds_for(b: &LaidBox, out: &mut Vec<DrawCmd>) {
    let (center, half) = center_half(b.rect);
    if half.x <= 0.0 || half.y <= 0.0 {
        return;
    }
    let at = Affine::translate(center.x, center.y);
    if let Painted::Box {
        background,
        radius,
        border,
    } = &b.kind
    {
        let r = radius.min(half.x).min(half.y).max(0.0);
        match border {
            Some((bw, bc)) => {
                out.push(DrawCmd {
                    shape: Shape::RoundedRect { half, radius: r },
                    paint: Paint::Solid(*bc),
                    transform: at,
                });
                if let Some(bg) = background {
                    let inner = Vec2::new((half.x - bw).max(0.0), (half.y - bw).max(0.0));
                    out.push(DrawCmd {
                        shape: Shape::RoundedRect {
                            half: inner,
                            radius: (r - bw).max(0.0),
                        },
                        paint: Paint::Solid(*bg),
                        transform: at,
                    });
                }
            }
            None => {
                if let Some(bg) = background {
                    out.push(DrawCmd {
                        shape: Shape::RoundedRect { half, radius: r },
                        paint: Paint::Solid(*bg),
                        transform: at,
                    });
                }
            }
        }
    }
}
