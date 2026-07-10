//! Atom Display List / Render IR.
//!
//! The IR is pure scene data: no rasterization, no ordering policy, and no host
//! decisions. Front ends and layout code can reduce to this vocabulary before an
//! orchestrator decides how to paint, inspect, export, or replay it.

use crate::{
    geom::{Affine, Vec2},
    paint::{Bounds, Paint, Rgba, Shape},
    path::PathCmd,
    ux::Role,
};

/// Stable scene node identity used by display lists and future caches.
pub type NodeId = u64;

/// Stable interaction identity used by event targets.
pub type EventId = u32;

/// One node in Atom's renderer-neutral display list.
#[derive(Clone, Debug)]
pub enum AtomNode {
    /// Structural container. Children are already in source/display-list order.
    Group {
        id: Option<NodeId>,
        children: Vec<AtomNode>,
    },
    /// Isolated compositing layer. The orchestrator decides when to allocate it.
    Layer {
        id: Option<NodeId>,
        opacity: f32,
        blend: BlendMode,
        children: Vec<AtomNode>,
    },
    /// Clip subtree to a geometric shape.
    Clip {
        shape: ClipShape,
        children: Vec<AtomNode>,
    },
    /// Transform subtree coordinates.
    Transform {
        transform: Affine,
        children: Vec<AtomNode>,
    },
    /// Filled local-space shape placed by an affine transform.
    Shape {
        id: Option<NodeId>,
        shape: Shape,
        paint: Paint,
        transform: Affine,
        soft: f32,
    },
    /// Filled or stroked device-space path.
    Path {
        id: Option<NodeId>,
        path: Vec<PathCmd>,
        paint: Paint,
        stroke: Option<StrokeStyle>,
    },
    /// One positioned text run.
    Text { id: Option<NodeId>, run: TextRun },
    /// Referenced raster image.
    Image {
        id: Option<NodeId>,
        image: ImageRef,
        rect: Bounds,
        fit: ImageFit,
    },
    /// Semantic interaction wrapper. It does not choose behavior.
    EventTarget {
        event_id: EventId,
        role: Role,
        children: Vec<AtomNode>,
    },
}

impl AtomNode {
    /// Create an anonymous group.
    pub fn group(children: Vec<AtomNode>) -> Self {
        Self::Group { id: None, children }
    }

    /// Create a keyed group.
    pub fn keyed_group(id: NodeId, children: Vec<AtomNode>) -> Self {
        Self::Group {
            id: Some(id),
            children,
        }
    }

    /// Create an anonymous compositing layer.
    pub fn layer(opacity: f32, blend: BlendMode, children: Vec<AtomNode>) -> Self {
        Self::Layer {
            id: None,
            opacity,
            blend,
            children,
        }
    }

    /// Create a clipping subtree.
    pub fn clipped(shape: ClipShape, children: Vec<AtomNode>) -> Self {
        Self::Clip { shape, children }
    }

    /// Create a rectangular clipping subtree.
    pub fn clip_rect(bounds: Bounds, children: Vec<AtomNode>) -> Self {
        Self::clipped(ClipShape::Rect(bounds), children)
    }

    /// Create a transformed subtree.
    pub fn transformed(transform: Affine, children: Vec<AtomNode>) -> Self {
        Self::Transform {
            transform,
            children,
        }
    }

    /// Create an unkeyed, crisp shape node.
    pub fn shape(shape: Shape, paint: Paint, transform: Affine) -> Self {
        Self::Shape {
            id: None,
            shape,
            paint,
            transform,
            soft: 0.0,
        }
    }

    /// Create an unkeyed path node.
    pub fn path(path: Vec<PathCmd>, paint: Paint, stroke: Option<StrokeStyle>) -> Self {
        Self::Path {
            id: None,
            path,
            paint,
            stroke,
        }
    }

    /// Create an unkeyed text node.
    pub fn text(text: impl Into<String>, origin: Vec2, size: f32, color: Rgba) -> Self {
        Self::Text {
            id: None,
            run: TextRun::new(text, origin, size, color),
        }
    }

    /// Create an unkeyed image node.
    pub fn image(image: ImageRef, rect: Bounds, fit: ImageFit) -> Self {
        Self::Image {
            id: None,
            image,
            rect,
            fit,
        }
    }

    /// Wrap children in an event target.
    pub fn event_target(event_id: EventId, role: Role, children: Vec<AtomNode>) -> Self {
        Self::EventTarget {
            event_id,
            role,
            children,
        }
    }
}

/// Blend mode requested for a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

/// Clip geometry for an IR subtree.
#[derive(Clone, Debug)]
pub enum ClipShape {
    Rect(Bounds),
    RoundedRect { bounds: Bounds, radius: f32 },
    Path(Vec<PathCmd>),
}

/// A single laid-out text run. Origin uses the same top-left text box convention
/// as `crate::text::draw`.
#[derive(Clone, Debug)]
pub struct TextRun {
    pub text: String,
    pub origin: Vec2,
    pub size: f32,
    pub color: Rgba,
    pub bold: bool,
    pub underline: bool,
}

impl TextRun {
    /// Create a plain text run.
    pub fn new(text: impl Into<String>, origin: Vec2, size: f32, color: Rgba) -> Self {
        Self {
            text: text.into(),
            origin,
            size,
            color,
            bold: false,
            underline: false,
        }
    }

    /// Mark this run as bold.
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Mark this run as underlined.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
}

/// Stroke style for path nodes.
#[derive(Clone, Debug)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub dash: Vec<f32>,
}

impl StrokeStyle {
    /// Create a solid stroke with butt caps and miter joins.
    pub fn new(width: f32) -> Self {
        Self {
            width,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            dash: Vec::new(),
        }
    }
}

/// Line cap style for path strokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// Line join style for path strokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// Reference to an image owned by a future image store.
#[derive(Clone, Debug)]
pub struct ImageRef {
    pub id: u64,
}

impl ImageRef {
    /// Create an image reference by stable id.
    pub const fn new(id: u64) -> Self {
        Self { id }
    }
}

/// How an image should fit inside its destination rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    Fill,
    Contain,
    Cover,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> Rgba {
        Rgba::rgb8(255, 0, 0)
    }

    #[test]
    fn constructs_shape_node() {
        let node = AtomNode::shape(
            Shape::Rect {
                half: Vec2::new(10.0, 5.0),
            },
            Paint::Solid(red()),
            Affine::IDENTITY,
        );

        match node {
            AtomNode::Shape {
                id,
                soft,
                shape: Shape::Rect { half },
                ..
            } => {
                assert_eq!(id, None);
                assert_eq!(soft, 0.0);
                assert_eq!(half, Vec2::new(10.0, 5.0));
            }
            other => panic!("expected shape node, got {other:?}"),
        }
    }

    #[test]
    fn constructs_text_node() {
        let node = AtomNode::text("Atom", Vec2::new(3.0, 4.0), 16.0, red());

        match node {
            AtomNode::Text { run, .. } => {
                assert_eq!(run.text, "Atom");
                assert_eq!(run.origin, Vec2::new(3.0, 4.0));
                assert_eq!(run.size, 16.0);
                assert!(!run.bold);
                assert!(!run.underline);
            }
            other => panic!("expected text node, got {other:?}"),
        }
    }

    #[test]
    fn constructs_clipped_group() {
        let clip = Bounds {
            min: Vec2::new(0.0, 0.0),
            max: Vec2::new(20.0, 20.0),
        };
        let node = AtomNode::clip_rect(clip, vec![AtomNode::group(Vec::new())]);

        match node {
            AtomNode::Clip {
                shape: ClipShape::Rect(bounds),
                children,
            } => {
                assert_eq!(bounds.min, clip.min);
                assert_eq!(bounds.max, clip.max);
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected clip node, got {other:?}"),
        }
    }

    #[test]
    fn constructs_transformed_group() {
        let tx = Affine::translate(8.0, 9.0);
        let node = AtomNode::transformed(tx, vec![AtomNode::group(Vec::new())]);

        match node {
            AtomNode::Transform {
                transform,
                children,
            } => {
                assert_eq!(transform.e, 8.0);
                assert_eq!(transform.f, 9.0);
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected transform node, got {other:?}"),
        }
    }

    #[test]
    fn constructs_event_target_wrapping_child() {
        let node = AtomNode::event_target(
            7,
            Role::Button,
            vec![AtomNode::shape(
                Shape::Circle { radius: 4.0 },
                Paint::Solid(red()),
                Affine::IDENTITY,
            )],
        );

        match node {
            AtomNode::EventTarget {
                event_id,
                role,
                children,
            } => {
                assert_eq!(event_id, 7);
                assert_eq!(role, Role::Button);
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected event target, got {other:?}"),
        }
    }
}
