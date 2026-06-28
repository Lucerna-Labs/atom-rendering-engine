//! Reduced HTML/CSS front-end. Parses a small HTML subset with inline `style` attributes
//! into the shared UXI box tree (`crate::ux`), which then flows through the same layout +
//! raster path as native UXI. This is the "reduce": the load-bearing core is the box model,
//! block/flex layout, and a CSS property subset; selectors, external stylesheets, and the
//! full cascade are the expansion, not the foundation.

use crate::paint::Rgba;
use crate::ux::{Align, Dim, Dir, Edges, Justify, Style, UxNode};

// ---- DOM ----

enum Dom {
    Elem {
        tag: String,
        style_attr: Option<String>,
        kids: Vec<Dom>,
    },
    Text(String),
}

enum Tok {
    Open {
        tag: String,
        style_attr: Option<String>,
        self_close: bool,
    },
    Close(String),
    Text(String),
}

/// Inherited text properties (a minimal stand-in for CSS inheritance).
#[derive(Clone, Copy)]
struct Inherited {
    color: Rgba,
    font_size: f32,
}

/// Parse an HTML document fragment into a single UXI root node.
pub fn parse(src: &str) -> UxNode {
    let toks = tokenize(src);
    let mut pos = 0usize;
    let roots = parse_nodes(&toks, &mut pos, None);
    let inherited = Inherited {
        color: Rgba::rgb8(228, 232, 240),
        font_size: 14.0,
    };
    let kids: Vec<UxNode> = roots.iter().map(|d| to_ux(d, inherited)).collect();
    if kids.len() == 1 {
        kids.into_iter().next().unwrap()
    } else {
        UxNode::Box {
            style: Style::col(),
            children: kids,
        }
    }
}

fn is_void(tag: &str) -> bool {
    matches!(tag, "br" | "img" | "hr" | "input" | "meta" | "link")
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim_end().to_string()
}

fn tokenize(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < b.len() {
        if b[i] == b'<' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'>' {
                j += 1;
            }
            let inner = src[i + 1..j].trim();
            if let Some(rest) = inner.strip_prefix('/') {
                out.push(Tok::Close(rest.trim().to_ascii_lowercase()));
            } else {
                let self_close = inner.ends_with('/');
                let inner = inner.trim_end_matches('/').trim();
                let (tag, style_attr) = parse_open(inner);
                let self_close = self_close || is_void(&tag);
                out.push(Tok::Open {
                    tag,
                    style_attr,
                    self_close,
                });
            }
            i = j + 1;
        } else {
            let start = i;
            while i < b.len() && b[i] != b'<' {
                i += 1;
            }
            let text = collapse_ws(&src[start..i]);
            if !text.is_empty() {
                out.push(Tok::Text(text));
            }
        }
    }
    out
}

/// Pull the tag name and the `style="..."` value out of an opening-tag body.
fn parse_open(inner: &str) -> (String, Option<String>) {
    let mut it = inner.splitn(2, char::is_whitespace);
    let tag = it.next().unwrap_or("").to_ascii_lowercase();
    let attrs = it.next().unwrap_or("");
    let style_attr = extract_attr(attrs, "style");
    (tag, style_attr)
}

fn extract_attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let key = format!("{name}=");
    let idx = lower.find(&key)?;
    let after = &attrs[idx + key.len()..];
    let bytes = after.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let quote = bytes[0];
    if quote == b'"' || quote == b'\'' {
        let rest = &after[1..];
        let end = rest.find(quote as char)?;
        Some(rest[..end].to_string())
    } else {
        Some(after.split_whitespace().next().unwrap_or("").to_string())
    }
}

fn parse_nodes(toks: &[Tok], pos: &mut usize, stop: Option<&str>) -> Vec<Dom> {
    let mut nodes = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Tok::Close(name) => {
                if Some(name.as_str()) == stop {
                    *pos += 1;
                    return nodes;
                }
                *pos += 1; // stray close: skip
            }
            Tok::Text(t) => {
                nodes.push(Dom::Text(t.clone()));
                *pos += 1;
            }
            Tok::Open {
                tag,
                style_attr,
                self_close,
            } => {
                let tag = tag.clone();
                let style_attr = style_attr.clone();
                let self_close = *self_close;
                *pos += 1;
                let kids = if self_close {
                    Vec::new()
                } else {
                    parse_nodes(toks, pos, Some(&tag))
                };
                nodes.push(Dom::Elem {
                    tag,
                    style_attr,
                    kids,
                });
            }
        }
    }
    nodes
}

// ---- DOM -> UXI ----

fn tag_font(tag: &str, base: f32) -> f32 {
    match tag {
        "h1" => 30.0,
        "h2" => 24.0,
        "h3" => 18.0,
        "small" => 11.0,
        _ => base,
    }
}

fn to_ux(dom: &Dom, inh: Inherited) -> UxNode {
    match dom {
        Dom::Text(t) => UxNode::Text {
            content: t.clone(),
            size: inh.font_size,
            color: inh.color,
        },
        Dom::Elem {
            tag,
            style_attr,
            kids,
        } => {
            let mut style = Style::col();
            let mut color = inh.color;
            let mut font = tag_font(tag, inh.font_size);
            if let Some(css) = style_attr {
                apply_css(&mut style, &mut color, &mut font, css);
            }
            let child_inh = Inherited {
                color,
                font_size: font,
            };
            let children = kids.iter().map(|k| to_ux(k, child_inh)).collect();
            UxNode::Box { style, children }
        }
    }
}

fn apply_css(style: &mut Style, color: &mut Rgba, font: &mut f32, css: &str) {
    for decl in css.split(';') {
        let mut kv = decl.splitn(2, ':');
        let key = kv.next().unwrap_or("").trim().to_ascii_lowercase();
        let val = match kv.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        match key.as_str() {
            "display" => {
                style.dir = if val == "flex" { Dir::Row } else { Dir::Column };
            }
            "flex-direction" => {
                style.dir = if val == "row" { Dir::Row } else { Dir::Column };
            }
            "flex" | "flex-grow" => {
                let n = val
                    .split_whitespace()
                    .next()
                    .and_then(parse_f32)
                    .unwrap_or(1.0);
                style.width = Dim::Flex(n);
                style.height = Dim::Flex(n);
            }
            "width" => style.width = parse_dim(val),
            "height" => style.height = parse_dim(val),
            "padding" => {
                if let Some(p) = parse_px(val) {
                    style.padding = Edges::all(p);
                }
            }
            "gap" => {
                if let Some(p) = parse_px(val) {
                    style.gap = p;
                }
            }
            "background" | "background-color" => {
                if let Some(c) = parse_color(val) {
                    style.background = Some(c);
                }
            }
            "border-radius" => {
                if let Some(p) = parse_px(val) {
                    style.radius = p;
                }
            }
            "border" => {
                let parts: Vec<&str> = val.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let (Some(w), Some(c)) = (parse_px(parts[0]), parse_color(parts[2])) {
                        style.border = Some((w, c));
                    }
                }
            }
            "color" => {
                if let Some(c) = parse_color(val) {
                    *color = c;
                }
            }
            "font-size" => {
                if let Some(p) = parse_px(val) {
                    *font = p;
                }
            }
            "align-items" => {
                style.align = match val {
                    "center" => Align::Center,
                    "flex-end" => Align::End,
                    "flex-start" => Align::Start,
                    _ => Align::Stretch,
                };
            }
            "justify-content" => {
                style.justify = match val {
                    "center" => Justify::Center,
                    "flex-end" => Justify::End,
                    "space-between" => Justify::SpaceBetween,
                    _ => Justify::Start,
                };
            }
            _ => {}
        }
    }
}

fn parse_f32(s: &str) -> Option<f32> {
    s.trim().parse().ok()
}

fn parse_px(s: &str) -> Option<f32> {
    let s = s.trim().trim_end_matches("px").trim();
    s.parse().ok()
}

fn parse_dim(s: &str) -> Dim {
    let s = s.trim();
    if s == "auto" {
        Dim::Auto
    } else if let Some(p) = parse_px(s) {
        Dim::Px(p)
    } else {
        Dim::Auto
    }
}

fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let (r, g, b) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                (r, g, b)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b)
            }
            _ => return None,
        };
        return Some(Rgba::rgb8(r, g, b));
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            return Some(Rgba::rgb8(r, g, b));
        }
    }
    match s.to_ascii_lowercase().as_str() {
        "white" => Some(Rgba::rgb8(255, 255, 255)),
        "black" => Some(Rgba::rgb8(0, 0, 0)),
        "gray" | "grey" => Some(Rgba::rgb8(128, 128, 128)),
        "red" => Some(Rgba::rgb8(220, 70, 70)),
        "green" => Some(Rgba::rgb8(60, 190, 120)),
        "blue" => Some(Rgba::rgb8(80, 140, 230)),
        _ => None,
    }
}
