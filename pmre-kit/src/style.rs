//! CSS style value data used by the parser and future computed-style pipeline.
//!
//! This module is mechanism only: it stores values, variables, and small parsing helpers.
//! Selector matching, source ordering, and cascade policy live in crate::css.

use crate::paint::Rgba;

/// A parsed CSS value that keeps unsupported values lossless enough for later phases.
#[derive(Clone, Debug)]
pub enum CssValue {
    Ident(String),
    String(String),
    Number(f32),
    Px(f32),
    Percent(f32),
    Color(Rgba),
    Function { name: String, args: String },
    Raw(String),
}

impl CssValue {
    /// Parse one declaration value into a small typed subset, preserving unknown values.
    pub fn parse(src: &str) -> Self {
        let trimmed = src.trim();
        if trimmed.is_empty() {
            return CssValue::Raw(String::new());
        }
        if let Some(s) = quoted(trimmed) {
            return CssValue::String(s.to_string());
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(px) = lower.strip_suffix("px").and_then(parse_f32) {
            return CssValue::Px(px);
        }
        if let Some(percent) = lower.strip_suffix('%').and_then(parse_f32) {
            return CssValue::Percent(percent);
        }
        if let Ok(n) = lower.parse::<f32>() {
            return CssValue::Number(n);
        }
        if let Some(color) = parse_color(&lower) {
            return CssValue::Color(color);
        }
        if let Some(open) = lower.find('(') {
            if lower.ends_with(')') && open > 0 {
                let name = lower[..open].trim();
                if is_ident(name) {
                    return CssValue::Function {
                        name: name.to_string(),
                        args: trimmed[open + 1..trimmed.len() - 1].trim().to_string(),
                    };
                }
            }
        }
        if is_ident(&lower) {
            CssValue::Ident(lower)
        } else {
            CssValue::Raw(trimmed.to_string())
        }
    }

    pub fn as_ident(&self) -> Option<&str> {
        match self {
            CssValue::Ident(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_px(&self) -> Option<f32> {
        match self {
            CssValue::Px(v) => Some(*v),
            CssValue::Number(v) => Some(*v),
            _ => None,
        }
    }
}

/// Author-specified declarations before layout/style reduction.
#[derive(Clone, Debug, Default)]
pub struct SpecifiedStyle {
    pub declarations: Vec<(String, CssValue, bool)>,
}

impl SpecifiedStyle {
    pub fn set(&mut self, property: impl Into<String>, value: CssValue, important: bool) {
        self.declarations
            .push((normalize_property(property.into()), value, important));
    }
}

/// Minimal computed-style carrier for future HTML integration.
#[derive(Clone, Debug)]
pub struct ComputedStyle {
    pub display: Option<String>,
    pub color: Option<Rgba>,
    pub variables: VariableMap,
}

impl ComputedStyle {
    pub fn inherit(parent: Option<&ComputedStyle>) -> Self {
        Self {
            display: None,
            color: parent.and_then(|p| p.color),
            variables: parent.map(|p| p.variables.clone()).unwrap_or_default(),
        }
    }
}

/// CSS custom properties inherited through the DOM tree.
#[derive(Clone, Debug, Default)]
pub struct VariableMap {
    values: Vec<(String, CssValue)>,
}

impl VariableMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inherit_from(parent: &VariableMap) -> Self {
        parent.clone()
    }

    pub fn set(&mut self, name: impl Into<String>, value: CssValue) {
        let name = normalize_variable(name.into());
        if let Some((_, existing)) = self.values.iter_mut().find(|(key, _)| *key == name) {
            *existing = value;
        } else {
            self.values.push((name, value));
        }
    }

    pub fn get(&self, name: &str) -> Option<&CssValue> {
        let name = normalize_variable(name.to_string());
        self.values
            .iter()
            .rev()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    }

    pub fn resolve_var(&self, value: &CssValue) -> Option<CssValue> {
        let CssValue::Function { name, args } = value else {
            return Some(value.clone());
        };
        if name != "var" {
            return Some(value.clone());
        }
        let mut parts = args.splitn(2, ',');
        let var_name = parts.next().unwrap_or("").trim();
        self.get(var_name)
            .cloned()
            .or_else(|| parts.next().map(CssValue::parse))
    }
}

pub fn normalize_property(property: String) -> String {
    property.trim().to_ascii_lowercase()
}

fn normalize_variable(name: String) -> String {
    let n = name.trim();
    if n.starts_with("--") {
        n.to_string()
    } else {
        format!("--{n}")
    }
}

fn quoted(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\"' && bytes[bytes.len() - 1] == b'\"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_' || first == '-')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_f32(s: &str) -> Option<f32> {
    s.trim().parse().ok()
}

fn parse_color(s: &str) -> Option<Rgba> {
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.is_ascii() {
            return None;
        }
        let dup = |i: usize| u8::from_str_radix(&hex[i..i + 1].repeat(2), 16).ok();
        let two = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return match hex.len() {
            3 => Some(Rgba::rgb8(dup(0)?, dup(1)?, dup(2)?)),
            4 => Some(Rgba::rgb8(dup(0)?, dup(1)?, dup(2)?).with_alpha(dup(3)? as f32 / 255.0)),
            6 => Some(Rgba::rgb8(two(0)?, two(2)?, two(4)?)),
            8 => Some(Rgba::rgb8(two(0)?, two(2)?, two(4)?).with_alpha(two(6)? as f32 / 255.0)),
            _ => None,
        };
    }
    if let Some(parts) = color_function(s, "rgba").or_else(|| color_function(s, "rgb")) {
        if parts.len() >= 3 {
            let r = css_channel(&parts[0])?;
            let g = css_channel(&parts[1])?;
            let b = css_channel(&parts[2])?;
            let a = parts.get(3).and_then(|p| css_alpha(p)).unwrap_or(1.0);
            return Some(Rgba::new(r, g, b, a));
        }
    }
    let (r, g, b, a) = match s {
        "transparent" => (0, 0, 0, 0),
        "black" => (0, 0, 0, 255),
        "white" => (255, 255, 255, 255),
        "red" => (255, 0, 0, 255),
        "green" => (0, 128, 0, 255),
        "blue" => (0, 0, 255, 255),
        "rebeccapurple" => (102, 51, 153, 255),
        _ => return None,
    };
    Some(Rgba::new(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

fn color_function(s: &str, name: &str) -> Option<Vec<String>> {
    let prefix = format!("{name}(");
    s.strip_prefix(&prefix)
        .and_then(|v| v.strip_suffix(')'))
        .map(|inner| {
            inner
                .split([',', '/'])
                .flat_map(str::split_whitespace)
                .map(str::to_string)
                .collect()
        })
}

fn css_channel(s: &str) -> Option<f32> {
    if let Some(p) = s.strip_suffix('%') {
        Some((parse_f32(p)? / 100.0).clamp(0.0, 1.0))
    } else {
        Some((parse_f32(s)? / 255.0).clamp(0.0, 1.0))
    }
}

fn css_alpha(s: &str) -> Option<f32> {
    if let Some(p) = s.strip_suffix('%') {
        Some((parse_f32(p)? / 100.0).clamp(0.0, 1.0))
    } else {
        Some(parse_f32(s)?.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_css_values() {
        assert!(matches!(CssValue::parse("12px"), CssValue::Px(v) if (v - 12.0).abs() < 0.01));
        assert!(matches!(CssValue::parse("75%"), CssValue::Percent(v) if (v - 75.0).abs() < 0.01));
        assert!(matches!(CssValue::parse("block"), CssValue::Ident(v) if v == "block"));
        assert!(
            matches!(CssValue::parse("calc(100% - 24px)"), CssValue::Function { ref name, .. } if name == "calc")
        );
        assert!(matches!(CssValue::parse("#abc"), CssValue::Color(_)));
    }

    #[test]
    fn variables_inherit_and_resolve_var_function() {
        let mut parent = VariableMap::new();
        parent.set("--accent", CssValue::parse("#336699"));
        let mut child = VariableMap::inherit_from(&parent);
        child.set("spacing", CssValue::parse("12px"));

        assert!(matches!(child.get("--accent"), Some(CssValue::Color(_))));
        assert!(
            matches!(child.get("--spacing"), Some(CssValue::Px(v)) if (*v - 12.0).abs() < 0.01)
        );
        assert!(matches!(
            child.resolve_var(&CssValue::parse("var(--spacing)")),
            Some(CssValue::Px(v)) if (v - 12.0).abs() < 0.01
        ));
        assert!(matches!(
            child.resolve_var(&CssValue::parse("var(--missing, 4px)")),
            Some(CssValue::Px(v)) if (v - 4.0).abs() < 0.01
        ));
    }
}
