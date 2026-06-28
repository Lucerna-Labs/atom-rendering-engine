//! Scanline path rasterizer — the exact-coverage generator beside the SDF generator.
//!
//! Fills arbitrary closed polygons and flattened Bézier curves that have no closed-form
//! distance (the shapes glyph outlines and vector art are made of). Coverage is analytic
//! across X and supersampled across Y, with nonzero-winding fill so opposite-wound subpaths
//! cut holes. Mechanism only: it fills the points it is given; the orchestrator decides what
//! and where.

use crate::framebuffer::Framebuffer;
use crate::geom::Vec2;
use crate::paint::{Bounds, Rgba};

/// A path command in device space (absolute coordinates).
#[derive(Clone, Copy, Debug)]
pub enum PathCmd {
    MoveTo(Vec2),
    LineTo(Vec2),
    Quad(Vec2, Vec2),        // control, end
    Cubic(Vec2, Vec2, Vec2), // control1, control2, end
    Close,
}

const CURVE_STEPS: usize = 32;
const SUBSCANLINES: usize = 5;

/// Flatten a command list into closed polylines (one `Vec` per subpath).
pub fn flatten(cmds: &[PathCmd]) -> Vec<Vec<Vec2>> {
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    let mut cur: Vec<Vec2> = Vec::new();
    let mut start = Vec2::new(0.0, 0.0);
    let mut last = start;
    for &cmd in cmds {
        match cmd {
            PathCmd::MoveTo(p) => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                cur.push(p);
                start = p;
                last = p;
            }
            PathCmd::LineTo(p) => {
                cur.push(p);
                last = p;
            }
            PathCmd::Quad(c, p) => {
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    cur.push(last.scale(u * u) + c.scale(2.0 * u * t) + p.scale(t * t));
                }
                last = p;
            }
            PathCmd::Cubic(c1, c2, p) => {
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    cur.push(
                        last.scale(u * u * u)
                            + c1.scale(3.0 * u * u * t)
                            + c2.scale(3.0 * u * t * t)
                            + p.scale(t * t * t),
                    );
                }
                last = p;
            }
            PathCmd::Close => {
                if !cur.is_empty() {
                    cur.push(start);
                    last = start;
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// Flatten and fill a command list in one call.
pub fn fill_cmds(fb: &mut Framebuffer, cmds: &[PathCmd], color: Rgba, clip: Option<Bounds>) {
    fill(fb, &flatten(cmds), color, clip);
}

/// Fill closed subpaths (device-space points) with `color`, clipped to `clip`.
pub fn fill(fb: &mut Framebuffer, subpaths: &[Vec<Vec2>], color: Rgba, clip: Option<Bounds>) {
    let mut minx = f32::INFINITY;
    let mut miny = f32::INFINITY;
    let mut maxx = f32::NEG_INFINITY;
    let mut maxy = f32::NEG_INFINITY;
    for sp in subpaths {
        for p in sp {
            minx = minx.min(p.x);
            miny = miny.min(p.y);
            maxx = maxx.max(p.x);
            maxy = maxy.max(p.y);
        }
    }
    if !minx.is_finite() {
        return;
    }

    let (mut cx0, mut cy0, mut cx1, mut cy1) = (0.0f32, 0.0f32, fb.width as f32, fb.height as f32);
    if let Some(c) = clip {
        cx0 = cx0.max(c.min.x);
        cy0 = cy0.max(c.min.y);
        cx1 = cx1.min(c.max.x);
        cy1 = cy1.min(c.max.y);
    }
    let x0 = minx.floor().max(cx0).max(0.0) as i32;
    let y0 = miny.floor().max(cy0).max(0.0) as i32;
    let x1 = maxx.ceil().min(cx1).min(fb.width as f32) as i32;
    let y1 = maxy.ceil().min(cy1).min(fb.height as f32) as i32;
    if x1 <= x0 || y1 <= y0 {
        return;
    }

    let row_w = (x1 - x0) as usize;
    let mut cov = vec![0.0f32; row_w];
    let mut xs: Vec<(f32, i32)> = Vec::new();
    let inv_ss = 1.0 / SUBSCANLINES as f32;

    for y in y0..y1 {
        cov.fill(0.0);
        for k in 0..SUBSCANLINES {
            let sy = y as f32 + (k as f32 + 0.5) * inv_ss;
            xs.clear();
            for sp in subpaths {
                let n = sp.len();
                if n < 2 {
                    continue;
                }
                for (i, &a) in sp.iter().enumerate() {
                    let b = sp[(i + 1) % n];
                    let (lo, hi, dir) = if a.y <= b.y { (a, b, 1) } else { (b, a, -1) };
                    if sy >= lo.y && sy < hi.y {
                        let t = (sy - lo.y) / (hi.y - lo.y);
                        xs.push((lo.x + t * (hi.x - lo.x), dir));
                    }
                }
            }
            if xs.len() < 2 {
                continue;
            }
            xs.sort_by(|p, q| p.0.total_cmp(&q.0));
            let mut wind = 0;
            for j in 0..xs.len() - 1 {
                wind += xs[j].1;
                if wind != 0 {
                    add_span(&mut cov, x0, x1, xs[j].0, xs[j + 1].0, inv_ss);
                }
            }
        }
        for (i, &c) in cov.iter().enumerate() {
            if c > 0.0 {
                let px = (x0 + i as i32) as u32;
                fb.blend_over(px, y as u32, color.with_alpha(color.a * c.min(1.0)));
            }
        }
    }
}

/// Accumulate analytic horizontal coverage for the span `[xa, xb]` at `weight`.
fn add_span(cov: &mut [f32], x0: i32, x1: i32, xa: f32, xb: f32, weight: f32) {
    let xa = xa.max(x0 as f32);
    let xb = xb.min(x1 as f32);
    if xb <= xa {
        return;
    }
    let ia = xa.floor() as i32;
    let ib = xb.ceil() as i32;
    for px in ia..ib {
        let cell_a = px as f32;
        let overlap = (xb.min(cell_a + 1.0) - xa.max(cell_a)).max(0.0);
        let idx = (px - x0) as usize;
        if idx < cov.len() {
            cov[idx] += weight * overlap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::Framebuffer;

    #[test]
    fn flatten_polygon_keeps_one_subpath() {
        let tri = [
            PathCmd::MoveTo(Vec2::new(0.0, 0.0)),
            PathCmd::LineTo(Vec2::new(10.0, 0.0)),
            PathCmd::LineTo(Vec2::new(0.0, 10.0)),
            PathCmd::Close,
        ];
        let sp = flatten(&tri);
        assert_eq!(sp.len(), 1);
        assert!(sp[0].len() >= 3);
    }

    #[test]
    fn flatten_cubic_emits_curve_samples() {
        let p = [
            PathCmd::MoveTo(Vec2::new(0.0, 0.0)),
            PathCmd::Cubic(Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)),
        ];
        let sp = flatten(&p);
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].len(), 1 + CURVE_STEPS); // start point + flattened samples
    }

    #[test]
    fn fill_covers_interior_leaves_exterior() {
        let bg = Rgba::new(0.0, 0.0, 0.0, 1.0);
        let mut fb = Framebuffer::new(40, 40, bg);
        let square = [
            PathCmd::MoveTo(Vec2::new(10.0, 10.0)),
            PathCmd::LineTo(Vec2::new(30.0, 10.0)),
            PathCmd::LineTo(Vec2::new(30.0, 30.0)),
            PathCmd::LineTo(Vec2::new(10.0, 30.0)),
            PathCmd::Close,
        ];
        fill_cmds(&mut fb, &square, Rgba::new(1.0, 0.0, 0.0, 1.0), None);
        let center = fb.pixel(20, 20);
        assert!(center.r > 0.95 && center.g < 0.05, "interior should be solid fill, got {center:?}");
        let outside = fb.pixel(2, 2);
        assert!(outside.r < 0.05, "exterior should be untouched, got {outside:?}");
    }

    #[test]
    fn opposite_winding_cuts_a_hole() {
        let bg = Rgba::new(0.0, 0.0, 0.0, 1.0);
        let mut fb = Framebuffer::new(60, 60, bg);
        let ring = [
            // outer contour
            PathCmd::MoveTo(Vec2::new(10.0, 10.0)),
            PathCmd::LineTo(Vec2::new(50.0, 10.0)),
            PathCmd::LineTo(Vec2::new(50.0, 50.0)),
            PathCmd::LineTo(Vec2::new(10.0, 50.0)),
            PathCmd::Close,
            // inner contour, wound the opposite way
            PathCmd::MoveTo(Vec2::new(24.0, 24.0)),
            PathCmd::LineTo(Vec2::new(24.0, 36.0)),
            PathCmd::LineTo(Vec2::new(36.0, 36.0)),
            PathCmd::LineTo(Vec2::new(36.0, 24.0)),
            PathCmd::Close,
        ];
        fill_cmds(&mut fb, &ring, Rgba::new(0.2, 0.4, 1.0, 1.0), None);
        assert!(fb.pixel(12, 30).b > 0.5, "ring band should be filled");
        assert!(fb.pixel(30, 30).b < 0.2, "centre should be a hole");
    }
}

