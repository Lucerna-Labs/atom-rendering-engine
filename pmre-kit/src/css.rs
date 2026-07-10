//! Dependency-free CSS parser, selector matcher, and cascade foundation.
//!
//! This is intentionally parser-first. HTML integration remains in html.rs until the
//! selector/cascade surface is exercised enough to replace the inline-only path safely.

use crate::style::{normalize_property, CssValue};

/// Parsed stylesheet rule list.
#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
}

impl Stylesheet {
    pub fn parse(src: &str) -> Self {
        parse_stylesheet(src)
    }
}

/// One qualified CSS rule.
#[derive(Clone, Debug)]
pub struct CssRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    pub specificity: Specificity,
}

/// One CSS declaration.
#[derive(Clone, Debug)]
pub struct Declaration {
    pub property: String,
    pub value: CssValue,
    pub important: bool,
}

/// Selector specificity: ids, classes/attributes/pseudo-classes, element names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    pub ids: u32,
    pub classes: u32,
    pub elements: u32,
}

impl Specificity {
    pub const ZERO: Self = Self {
        ids: 0,
        classes: 0,
        elements: 0,
    };

    pub const INLINE: Self = Self {
        ids: 1_000,
        classes: 0,
        elements: 0,
    };
}

/// A selector stored left-to-right. Each part after the first carries its relation
/// to the previous part.
#[derive(Clone, Debug)]
pub struct Selector {
    pub parts: Vec<SelectorPart>,
    pub specificity: Specificity,
}

#[derive(Clone, Debug)]
pub struct SelectorPart {
    pub combinator: Option<Combinator>,
    pub simple: SimpleSelector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
}

#[derive(Clone, Debug, Default)]
pub struct SimpleSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub pseudos: Vec<PseudoClass>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PseudoClass {
    Hover,
    Active,
    Focus,
    Unknown(String),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ElementState {
    pub hover: bool,
    pub active: bool,
    pub focus: bool,
}

/// A lightweight element view for selector matching. The last entry in a path is
/// the candidate element; earlier entries are its ancestors from root to parent.
#[derive(Clone, Copy, Debug)]
pub struct ElementRef<'a> {
    pub tag: &'a str,
    pub id: Option<&'a str>,
    pub classes: &'a [&'a str],
    pub state: ElementState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleOrigin {
    UserAgent,
    Author,
    Inline,
}

/// A winning declaration after cascade comparison.
#[derive(Clone, Debug)]
pub struct CascadedProperty {
    pub property: String,
    pub value: CssValue,
    pub important: bool,
    pub specificity: Specificity,
    pub source_order: usize,
    pub origin: StyleOrigin,
}

pub fn parse_stylesheet(src: &str) -> Stylesheet {
    let css = strip_comments(src);
    let mut rules = Vec::new();
    let mut pos = 0usize;
    while pos < css.len() {
        let Some(open_rel) = css[pos..].find('{') else {
            break;
        };
        let open = pos + open_rel;
        let selector_src = css[pos..open].trim();
        let Some(close_rel) = css[open + 1..].find('}') else {
            break;
        };
        let close = open + 1 + close_rel;
        let body = &css[open + 1..close];
        pos = close + 1;

        if selector_src.is_empty() || selector_src.starts_with('@') {
            continue;
        }
        let selectors: Vec<Selector> = split_top_level(selector_src, ',')
            .into_iter()
            .filter_map(|s| parse_selector(s.trim()))
            .collect();
        if selectors.is_empty() {
            continue;
        }
        let declarations = parse_declarations(body);
        if declarations.is_empty() {
            continue;
        }
        let specificity = selectors
            .iter()
            .map(|s| s.specificity)
            .max()
            .unwrap_or(Specificity::ZERO);
        rules.push(CssRule {
            selectors,
            declarations,
            specificity,
        });
    }
    Stylesheet { rules }
}

pub fn parse_declarations(src: &str) -> Vec<Declaration> {
    let mut out = Vec::new();
    for raw in split_top_level(src, ';') {
        let Some(colon) = find_top_level(raw, ':') else {
            continue;
        };
        let property = normalize_property(raw[..colon].to_string());
        if property.is_empty() {
            continue;
        }
        let mut value_src = raw[colon + 1..].trim();
        let mut important = false;
        if let Some(stripped) = strip_important(value_src) {
            value_src = stripped;
            important = true;
        }
        out.push(Declaration {
            property,
            value: CssValue::parse(value_src),
            important,
        });
    }
    out
}

pub fn parse_selector(src: &str) -> Option<Selector> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut parts = Vec::new();
    let mut pending: Option<Combinator> = None;

    while i < chars.len() {
        let mut saw_space = false;
        while i < chars.len() && chars[i].is_whitespace() {
            saw_space = true;
            i += 1;
        }
        if saw_space && !parts.is_empty() && pending.is_none() {
            pending = Some(Combinator::Descendant);
        }
        if i < chars.len() && chars[i] == '>' {
            pending = Some(Combinator::Child);
            i += 1;
            continue;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '>' {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        let simple = parse_simple_selector(&token)?;
        let combinator = if parts.is_empty() {
            None
        } else {
            Some(pending.take().unwrap_or(Combinator::Descendant))
        };
        parts.push(SelectorPart { combinator, simple });
    }

    if parts.is_empty() {
        None
    } else {
        let specificity = parts.iter().fold(Specificity::ZERO, |acc, part| {
            add_specificity(acc, &part.simple)
        });
        Some(Selector { parts, specificity })
    }
}

pub fn selector_matches(selector: &Selector, path: &[ElementRef<'_>]) -> bool {
    if selector.parts.is_empty() || path.is_empty() {
        return false;
    }
    matches_from(selector, selector.parts.len() - 1, path, path.len() - 1)
}

pub fn cascade(
    stylesheet: &Stylesheet,
    path: &[ElementRef<'_>],
    inline: &[Declaration],
) -> Vec<CascadedProperty> {
    let mut winners = Vec::new();
    let mut source_order = 0usize;

    for rule in &stylesheet.rules {
        for selector in &rule.selectors {
            if !selector_matches(selector, path) {
                continue;
            }
            for declaration in &rule.declarations {
                let candidate = CascadedProperty {
                    property: declaration.property.clone(),
                    value: declaration.value.clone(),
                    important: declaration.important,
                    specificity: selector.specificity,
                    source_order,
                    origin: StyleOrigin::Author,
                };
                insert_candidate(&mut winners, candidate);
                source_order += 1;
            }
        }
    }

    for declaration in inline {
        let candidate = CascadedProperty {
            property: declaration.property.clone(),
            value: declaration.value.clone(),
            important: declaration.important,
            specificity: Specificity::INLINE,
            source_order,
            origin: StyleOrigin::Inline,
        };
        insert_candidate(&mut winners, candidate);
        source_order += 1;
    }

    winners
}

fn insert_candidate(winners: &mut Vec<CascadedProperty>, candidate: CascadedProperty) {
    if let Some(existing) = winners
        .iter_mut()
        .find(|winner| winner.property == candidate.property)
    {
        if candidate_wins(&candidate, existing) {
            *existing = candidate;
        }
    } else {
        winners.push(candidate);
    }
}

fn candidate_wins(next: &CascadedProperty, old: &CascadedProperty) -> bool {
    cascade_rank(next) > cascade_rank(old)
}

fn cascade_rank(p: &CascadedProperty) -> (u8, u8, Specificity, usize) {
    let important = u8::from(p.important);
    let origin = match p.origin {
        StyleOrigin::UserAgent => 0,
        StyleOrigin::Author => 1,
        StyleOrigin::Inline => 2,
    };
    (important, origin, p.specificity, p.source_order)
}

fn matches_from(
    selector: &Selector,
    part_idx: usize,
    path: &[ElementRef<'_>],
    elem_idx: usize,
) -> bool {
    let part = &selector.parts[part_idx];
    if !simple_matches(&part.simple, &path[elem_idx]) {
        return false;
    }
    if part_idx == 0 {
        return true;
    }
    match part.combinator.unwrap_or(Combinator::Descendant) {
        Combinator::Child => {
            elem_idx > 0 && matches_from(selector, part_idx - 1, path, elem_idx - 1)
        }
        Combinator::Descendant => {
            if elem_idx == 0 {
                return false;
            }
            (0..elem_idx)
                .rev()
                .any(|ancestor_idx| matches_from(selector, part_idx - 1, path, ancestor_idx))
        }
    }
}

fn simple_matches(selector: &SimpleSelector, element: &ElementRef<'_>) -> bool {
    if let Some(tag) = &selector.tag {
        if tag != "*" && !tag.eq_ignore_ascii_case(element.tag) {
            return false;
        }
    }
    if let Some(id) = &selector.id {
        if element.id != Some(id.as_str()) {
            return false;
        }
    }
    for class in &selector.classes {
        if !element.classes.iter().any(|c| *c == class) {
            return false;
        }
    }
    for pseudo in &selector.pseudos {
        let ok = match pseudo {
            PseudoClass::Hover => element.state.hover,
            PseudoClass::Active => element.state.active,
            PseudoClass::Focus => element.state.focus,
            PseudoClass::Unknown(_) => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn parse_simple_selector(token: &str) -> Option<SimpleSelector> {
    let chars: Vec<char> = token.chars().collect();
    let mut i = 0usize;
    let mut selector = SimpleSelector::default();

    if i < chars.len() && (is_ident_start(chars[i]) || chars[i] == '*') {
        let start = i;
        i += 1;
        while i < chars.len() && is_ident_continue(chars[i]) {
            i += 1;
        }
        let tag: String = chars[start..i].iter().collect();
        if tag != "*" {
            selector.tag = Some(tag.to_ascii_lowercase());
        } else {
            selector.tag = Some(tag);
        }
    }

    while i < chars.len() {
        let marker = chars[i];
        if marker != '.' && marker != '#' && marker != ':' {
            return None;
        }
        i += 1;
        let start = i;
        while i < chars.len() && is_ident_continue(chars[i]) {
            i += 1;
        }
        if start == i {
            return None;
        }
        let value: String = chars[start..i].iter().collect();
        match marker {
            '.' => selector.classes.push(value),
            '#' => selector.id = Some(value),
            ':' => selector.pseudos.push(parse_pseudo(&value)),
            _ => {}
        }
    }

    if selector.tag.is_none()
        && selector.id.is_none()
        && selector.classes.is_empty()
        && selector.pseudos.is_empty()
    {
        None
    } else {
        Some(selector)
    }
}

fn parse_pseudo(value: &str) -> PseudoClass {
    match value {
        "hover" => PseudoClass::Hover,
        "active" => PseudoClass::Active,
        "focus" => PseudoClass::Focus,
        other => PseudoClass::Unknown(other.to_string()),
    }
}

fn add_specificity(mut specificity: Specificity, selector: &SimpleSelector) -> Specificity {
    if selector.id.is_some() {
        specificity.ids += 1;
    }
    specificity.classes += selector.classes.len() as u32 + selector.pseudos.len() as u32;
    if selector.tag.is_some() {
        specificity.elements += 1;
    }
    specificity
}

fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn split_top_level(src: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    for (idx, ch) in src.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\"' | '\'' => quote = Some(ch),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if ch == separator && depth == 0 => {
                parts.push(&src[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&src[start..]);
    parts
}

fn find_top_level(src: &str, needle: char) -> Option<usize> {
    let mut depth = 0u32;
    let mut quote: Option<char> = None;
    for (idx, ch) in src.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '\"' | '\'' => quote = Some(ch),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if ch == needle && depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
}

fn strip_important(src: &str) -> Option<&str> {
    let trimmed = src.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("!important") {
        let cut = trimmed.len() - "!important".len();
        Some(trimmed[..cut].trim_end())
    } else {
        None
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == '-'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current<'a>(
        tag: &'a str,
        id: Option<&'a str>,
        classes: &'a [&'a str],
    ) -> Vec<ElementRef<'a>> {
        vec![ElementRef {
            tag,
            id,
            classes,
            state: ElementState::default(),
        }]
    }

    #[test]
    fn parses_class_id_tag_and_tag_class_selectors() {
        let sheet = Stylesheet::parse(".card, #main, button.primary { color: red; }");
        assert_eq!(sheet.rules.len(), 1);
        let selectors = &sheet.rules[0].selectors;
        assert_eq!(selectors.len(), 3);
        assert_eq!(selectors[0].parts[0].simple.classes, vec!["card"]);
        assert_eq!(selectors[1].parts[0].simple.id.as_deref(), Some("main"));
        assert_eq!(selectors[2].parts[0].simple.tag.as_deref(), Some("button"));
        assert_eq!(selectors[2].parts[0].simple.classes, vec!["primary"]);
    }

    #[test]
    fn parses_descendant_child_and_state_pseudos() {
        let selector = parse_selector("section.card > button.primary:hover").unwrap();
        assert_eq!(selector.parts.len(), 2);
        assert_eq!(selector.parts[1].combinator, Some(Combinator::Child));
        assert!(matches!(
            selector.parts[1].simple.pseudos.as_slice(),
            [PseudoClass::Hover]
        ));

        let path = vec![
            ElementRef {
                tag: "section",
                id: None,
                classes: &["card"],
                state: ElementState::default(),
            },
            ElementRef {
                tag: "button",
                id: None,
                classes: &["primary"],
                state: ElementState {
                    hover: true,
                    ..ElementState::default()
                },
            },
        ];
        assert!(selector_matches(&selector, &path));
    }

    #[test]
    fn specificity_orders_id_class_and_tag() {
        let tag = parse_selector("button").unwrap().specificity;
        let class = parse_selector("button.primary:hover").unwrap().specificity;
        let id = parse_selector("#submit").unwrap().specificity;
        assert!(class > tag);
        assert!(id > class);
    }

    #[test]
    fn selector_matching_supports_descendant() {
        let selector = parse_selector("main .card button").unwrap();
        let path = vec![
            ElementRef {
                tag: "main",
                id: None,
                classes: &[],
                state: ElementState::default(),
            },
            ElementRef {
                tag: "div",
                id: None,
                classes: &["card"],
                state: ElementState::default(),
            },
            ElementRef {
                tag: "button",
                id: None,
                classes: &[],
                state: ElementState::default(),
            },
        ];
        assert!(selector_matches(&selector, &path));
    }

    #[test]
    fn cascade_inline_beats_stylesheet_normal() {
        let sheet = Stylesheet::parse("button { display: block; }");
        let inline = parse_declarations("display: flex");
        let out = cascade(&sheet, &current("button", None, &[]), &inline);
        let display = out.iter().find(|p| p.property == "display").unwrap();
        assert!(matches!(display.value, CssValue::Ident(ref v) if v == "flex"));
    }

    #[test]
    fn cascade_important_beats_inline_normal() {
        let sheet = Stylesheet::parse("button { display: block !important; }");
        let inline = parse_declarations("display: flex");
        let out = cascade(&sheet, &current("button", None, &[]), &inline);
        let display = out.iter().find(|p| p.property == "display").unwrap();
        assert!(display.important);
        assert!(matches!(display.value, CssValue::Ident(ref v) if v == "block"));
    }

    #[test]
    fn malformed_css_degrades_without_panic() {
        let sheet = Stylesheet::parse(".a { color red; width: calc(100% - 2px); } broken { color");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations.len(), 1);
        assert_eq!(sheet.rules[0].declarations[0].property, "width");
    }
}
