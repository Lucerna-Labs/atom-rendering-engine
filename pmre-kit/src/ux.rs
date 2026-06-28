//! UXI intent vocabulary: a tree of styled boxes and text, with NO coordinates.
//! Position is derived by the layout solver (`crate::layout`), never authored here.
//! HTML/CSS reduces onto this same vocabulary (a box tree + a property subset).
//!
//! Interaction is carried as data too: a box may declare an `id` and a `Role`
//! (Button / Toggle / Scroll). The kit provides hit-testing and scroll/clip mechanism;
//! all widget *policy* (hover/press visuals, toggle flips, scroll offsets) lives in the
//! orchestrator and the app's state-driven `build` function.

use crate::paint::Rgba;

/// Main-axis direction of a box's children (the flex axis).
#[derive(Clone, Copy, Debug)]
pub enum Dir {
    Row,
    Column,
}

/// A size along one axis. `Flex` grows to share leftover main-axis space by weight.
#[derive(Clone, Copy, Debug)]
pub enum Dim {
    Auto,
    Px(f32),
    Flex(f32),
}

/// Cross-axis alignment of children.
#[derive(Clone, Copy, Debug)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

/// Main-axis distribution of children.
#[derive(Clone, Copy, Debug)]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Interactive role of a box. `None` is inert; the others are hit-testable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    None,
    Button,
    Toggle,
    Scroll,
}

/// Per-side lengths (padding / border insets).
#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Edges {
    pub fn all(v: f32) -> Self {
        Self { l: v, t: v, r: v, b: v }
    }
    pub fn xy(x: f32, y: f32) -> Self {
        Self { l: x, t: y, r: x, b: y }
    }
}

/// The reduced style subset shared by UXI and HTML/CSS, plus interaction metadata.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub dir: Dir,
    pub width: Dim,
    pub height: Dim,
    pub padding: Edges,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub background: Option<Rgba>,
    pub radius: f32,
    pub border: Option<(f32, Rgba)>,
    pub id: Option<u32>,
    pub role: Role,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            dir: Dir::Column,
            width: Dim::Auto,
            height: Dim::Auto,
            padding: Edges::default(),
            gap: 0.0,
            align: Align::Stretch, // matches CSS flexbox `align-items: stretch`
            justify: Justify::Start,
            background: None,
            radius: 0.0,
            border: None,
            id: None,
            role: Role::None,
        }
    }
}

impl Style {
    pub fn row() -> Self {
        Self { dir: Dir::Row, ..Self::default() }
    }
    pub fn col() -> Self {
        Self { dir: Dir::Column, ..Self::default() }
    }
    pub fn w(mut self, d: Dim) -> Self {
        self.width = d;
        self
    }
    pub fn h(mut self, d: Dim) -> Self {
        self.height = d;
        self
    }
    pub fn pad(mut self, e: Edges) -> Self {
        self.padding = e;
        self
    }
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn justify(mut self, j: Justify) -> Self {
        self.justify = j;
        self
    }
    pub fn bg(mut self, c: Rgba) -> Self {
        self.background = Some(c);
        self
    }
    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r;
        self
    }
    pub fn border(mut self, w: f32, c: Rgba) -> Self {
        self.border = Some((w, c));
        self
    }
    /// Make this box hit-testable with the given id and role.
    pub fn interactive(mut self, id: u32, role: Role) -> Self {
        self.id = Some(id);
        self.role = role;
        self
    }
    pub fn button(self, id: u32) -> Self {
        self.interactive(id, Role::Button)
    }
    pub fn toggle(self, id: u32) -> Self {
        self.interactive(id, Role::Toggle)
    }
    pub fn scroll(self, id: u32) -> Self {
        self.interactive(id, Role::Scroll)
    }
}

/// A UXI node: either a styled box with children, or a run of text.
#[derive(Clone, Debug)]
pub enum UxNode {
    Box { style: Style, children: Vec<UxNode> },
    Text { content: String, size: f32, color: Rgba },
}

impl UxNode {
    pub fn boxed(style: Style, children: Vec<UxNode>) -> UxNode {
        UxNode::Box { style, children }
    }
    pub fn text(content: impl Into<String>, size: f32, color: Rgba) -> UxNode {
        UxNode::Text { content: content.into(), size, color }
    }
}
