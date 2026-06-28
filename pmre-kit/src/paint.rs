//! Drawing vocabulary (renderer-neutral) and color. Pure data; makes no rendering decisions.

use crate::geom::{Affine, Vec2};

/// Straight-alpha RGBA in [0, 1]. `scale` keeps channels normalized.
#[derive(Clone, Copy, Debug)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    pub fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
    }
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

/// Axis-aligned bounding box in some coordinate space.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub fn pad(self, p: f32) -> Bounds {
        Bounds {
            min: Vec2::new(self.min.x - p, self.min.y - p),
            max: Vec2::new(self.max.x + p, self.max.y + p),
        }
    }
}

/// Local-space shapes (centered at the origin where natural); a `DrawCmd.transform` places them.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Rect { half: Vec2 },
    RoundedRect { half: Vec2, radius: f32 },
    Circle { radius: f32 },
    Line { a: Vec2, b: Vec2, width: f32 },
}

impl Shape {
    /// Local-space bounding box, before the command transform is applied.
    pub fn local_bounds(&self) -> Bounds {
        match *self {
            Shape::Rect { half } | Shape::RoundedRect { half, .. } => Bounds {
                min: Vec2::new(-half.x, -half.y),
                max: Vec2::new(half.x, half.y),
            },
            Shape::Circle { radius } => Bounds {
                min: Vec2::new(-radius, -radius),
                max: Vec2::new(radius, radius),
            },
            Shape::Line { a, b, width } => {
                let hw = width * 0.5;
                Bounds {
                    min: Vec2::new(a.x.min(b.x) - hw, a.y.min(b.y) - hw),
                    max: Vec2::new(a.x.max(b.x) + hw, a.y.max(b.y) + hw),
                }
            }
        }
    }

    /// True when the shape has no area to rasterize (an unfilled generator slot).
    pub fn is_degenerate(&self) -> bool {
        match *self {
            Shape::Rect { half } | Shape::RoundedRect { half, .. } => {
                half.x <= 0.0 || half.y <= 0.0
            }
            Shape::Circle { radius } => radius <= 0.0,
            Shape::Line { a, b, width } => width <= 0.0 || (a.x == b.x && a.y == b.y),
        }
    }
}

/// How a shape is filled. Two-stop gradients are sampled per pixel by the rasterizer.
/// Coordinates are in the shape's local space (for SDF shapes) or device space (for paths).
#[derive(Clone, Copy, Debug)]
pub enum Paint {
    Solid(Rgba),
    /// Linear gradient: `c0` at `from`, `c1` at `to`, clamped past the ends.
    Linear {
        from: Vec2,
        to: Vec2,
        c0: Rgba,
        c1: Rgba,
    },
    /// Radial gradient: `c0` at `center`, `c1` at `radius`.
    Radial {
        center: Vec2,
        radius: f32,
        c0: Rgba,
        c1: Rgba,
    },
}

impl Paint {
    /// The color of this paint at point `p`.
    pub fn sample(&self, p: Vec2) -> Rgba {
        match *self {
            Paint::Solid(c) => c,
            Paint::Linear { from, to, c0, c1 } => {
                let d = to - from;
                let t = ((p - from).dot(d) / d.dot(d).max(1e-6)).clamp(0.0, 1.0);
                lerp_rgba(c0, c1, t)
            }
            Paint::Radial {
                center,
                radius,
                c0,
                c1,
            } => {
                let t = ((p - center).length() / radius.max(1e-6)).clamp(0.0, 1.0);
                lerp_rgba(c0, c1, t)
            }
        }
    }
}

fn lerp_rgba(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// One drawing instruction: a shape, how to fill it, and where to place it.
#[derive(Clone, Copy, Debug)]
pub struct DrawCmd {
    pub shape: Shape,
    pub paint: Paint,
    pub transform: Affine,
}
