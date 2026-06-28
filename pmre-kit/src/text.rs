//! Glyph rasterization: blit bitmap text crisply onto the framebuffer.
//! A text run is `scan` (over glyph cells) + `combine` (alpha-over) — its own coverage
//! generator, distinct from the SDF generator used for shapes. Mechanism only.

use crate::font;
use crate::framebuffer::Framebuffer;
use crate::geom::Vec2;
use crate::paint::{Bounds, Rgba};

/// Width in device pixels that `draw` will advance for `content` at cap-height `size`.
pub fn advance(content: &str, size: f32) -> f32 {
    let cs = (size / 7.0).max(1.0);
    content.chars().count() as f32 * 6.0 * cs
}

/// Greedy word-wrap of `content` into lines that each fit within `max_width` pixels.
pub fn wrap(content: &str, size: f32, max_width: f32) -> Vec<String> {
    if max_width <= 0.0 {
        return vec![content.to_string()];
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in content.split_whitespace() {
        let trial = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || advance(&trial, size) <= max_width {
            cur = trial;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Render `content` with its top-left at `origin`; the 7-row glyph spans `size` pixels tall.
/// Pixels outside `clip` (when given) are skipped.
pub fn draw(fb: &mut Framebuffer, content: &str, origin: Vec2, size: f32, color: Rgba, clip: Option<Bounds>) {
    let cs = (size / 7.0).max(1.0);
    let mut pen_x = origin.x;
    for ch in content.chars() {
        let g = font::glyph(ch);
        for (r, &row) in g.iter().enumerate() {
            for col in 0..5u32 {
                if row & (1 << (4 - col)) != 0 {
                    fill_cell(fb, pen_x + col as f32 * cs, origin.y + r as f32 * cs, cs, color, clip);
                }
            }
        }
        pen_x += 6.0 * cs;
    }
}

fn fill_cell(fb: &mut Framebuffer, x: f32, y: f32, cs: f32, color: Rgba, clip: Option<Bounds>) {
    let x0 = x.round() as i32;
    let y0 = y.round() as i32;
    let x1 = (x + cs).round().max((x0 + 1) as f32) as i32;
    let y1 = (y + cs).round().max((y0 + 1) as f32) as i32;
    for py in y0..y1 {
        for px in x0..x1 {
            if px < 0 || py < 0 {
                continue;
            }
            if let Some(c) = clip {
                let (fx, fy) = (px as f32, py as f32);
                if fx < c.min.x || fx >= c.max.x || fy < c.min.y || fy >= c.max.y {
                    continue;
                }
            }
            fb.blend_over(px as u32, py as u32, color);
        }
    }
}
