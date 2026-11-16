use crate::css::{Combinator, Rule, SelectorPart, SimpleSelector, Stylesheet};
use crate::dom::{Dom, NodeId, NodeKind};

pub const INITIAL_FONT_SIZE: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Display {
    None,
    Block,
    Inline,
    Flex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    Auto,
    Px(f32),
    Percent(f32),
}

impl Default for Size {
    fn default() -> Self {
        Size::Auto
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Edges {
    pub top: Size,
    pub right: Size,
    pub bottom: Size,
    pub left: Size,
}

impl Default for Edges {
    fn default() -> Self {
        Edges {
            top: Size::Px(0.0),
            right: Size::Px(0.0),
            bottom: Size::Px(0.0),
            left: Size::Px(0.0),
        }
    }
}

/// The single source of truth for layout and paint. Immutable once computed,
/// one per node — the Stylo-shaped core: cascade is data, not side effects.
#[derive(Debug, Clone)]
pub struct Computed {
    pub display: Display,
    pub color: Color,
    pub background: Option<Color>,
    pub font_size: f32,
    pub font_weight: u16,
    pub margin: Edges,
    pub padding: Edges,
    pub radius: Size,
    pub width: Size,
    pub height: Size,
}

impl Default for Computed {
    fn default() -> Self {
        Computed {
            display: Display::Inline,
            color: Color::BLACK,
            background: None,
            font_size: INITIAL_FONT_SIZE,
            font_weight: 400,
            margin: Edges::default(),
            padding: Edges::default(),
            radius: Size::Px(0.0),
            width: Size::Auto,
            height: Size::Auto,
        }
    }
}

impl Computed {
    fn inherited(parent: &Computed) -> Computed {
        Computed { color: parent.color, font_size: parent.font_size, font_weight: parent.font_weight, ..Default::default() }
    }

    fn apply(&mut self, declaration: &crate::css::Declaration) {
        let value = declaration.value.trim();
        match declaration.property.as_str() {
            "display" => {
                self.display = match value.to_ascii_lowercase().as_str() {
                    "none" => Display::None,
                    "block" => Display::Block,
                    "inline" | "inline-block" => Display::Inline,
                    "flex" | "inline-flex" => Display::Flex,
                    _ => self.display,
                };
            }
            "color" => {
                if let Some(color) = parse_color(value) {
                    self.color = color;
                }
            }
            "background-color" => {
                if let Some(color) = parse_color(value) {
                    self.background = Some(color);
                }
            }
            "background" => {
                // Shorthand: only the color term applies in phase 1; anything
                // else (image, gradient) is left untouched, not faked.
                if let Some(color) = parse_color(value) {
                    self.background = Some(color);
                }
            }
            "font-size" => {
                if let Some(px) = parse_font_size(value, self.font_size) {
                    self.font_size = px;
                }
            }
            "font-weight" => {
                if value.eq_ignore_ascii_case("bold") {
                    self.font_weight = 700;
                } else if value.eq_ignore_ascii_case("normal") {
                    self.font_weight = 400;
                } else if let Ok(weight) = value.parse::<u16>() {
                    self.font_weight = weight.clamp(100, 900);
                }
            }
            "margin" => self.margin = parse_edges(value),
            "margin-top" => self.margin.top = parse_size(value),
            "margin-right" => self.margin.right = parse_size(value),
            "margin-bottom" => self.margin.bottom = parse_size(value),
            "margin-left" => self.margin.left = parse_size(value),
            "padding" => self.padding = parse_edges(value),
            "padding-top" => self.padding.top = parse_size(value),
            "padding-right" => self.padding.right = parse_size(value),
            "padding-bottom" => self.padding.bottom = parse_size(value),
            "padding-left" => self.padding.left = parse_size(value),
            "border-radius" => self.radius = parse_size(value),
            "width" => self.width = parse_size(value),
            "height" => self.height = parse_size(value),
            _ => {}
        }
    }
}

/// Runs the cascade: for every node, the inherited base plus every matching
/// declaration, ordered by specificity then document order. Inline `style`
/// attributes win as specificity u32::MAX.
pub fn cascade(dom: &Dom, sheet: &Stylesheet) -> Vec<Computed> {
    let mut computed: Vec<Computed> = vec![Computed::default(); dom.nodes.len()];

    fn resolve(
        dom: &Dom,
        sheet: &Stylesheet,
        node_id: NodeId,
        parent: Option<&Computed>,
        computed: &mut Vec<Computed>,
    ) {
        let base = match parent {
            Some(parent) => Computed::inherited(parent),
            None => Computed::default(),
        };
        let node = dom.node(node_id);
        let mut current = match &node.kind {
            NodeKind::Text(_) => base, // text nodes carry nothing but inheritance
            _ => base,
        };

        if let Some(element) = node.element() {
            let mut matches: Vec<(u32, usize, &Rule)> = Vec::new();
            for rule in &sheet.rules {
                for selector in &rule.selectors {
                    if selector_matches(dom, node_id, &selector.parts) {
                        matches.push((specificity(&selector.parts), matches.len(), rule));
                        break;
                    }
                }
            }
            matches.sort_by_key(|(spec, order, _)| (*spec, *order));
            for (_, _, rule) in &matches {
                for declaration in &rule.declarations {
                    current.apply(declaration);
                }
            }
            if let Some(inline) = element.attr("style") {
                for declaration in split_inline(inline) {
                    current.apply(&declaration);
                }
            }
        }

        computed[node_id.0 as usize] = current.clone();
        let parent_value = computed[node_id.0 as usize].clone();
        for child in &node.children {
            resolve(dom, sheet, *child, Some(&parent_value), computed);
        }
    }

    resolve(dom, sheet, dom.root, None, &mut computed);
    computed
}

/// Splits a `style="a:b; c:d"` attribute into declarations. Reuses the sheet
/// parser's semantics without a second grammar.
fn split_inline(inline: &str) -> Vec<crate::css::Declaration> {
    inline
        .split(';')
        .filter_map(|chunk| {
            let (property, value) = chunk.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if property.is_empty() || value.is_empty() {
                return None;
            }
            Some(crate::css::Declaration { property, value })
        })
        .collect()
}

fn specificity(parts: &[SelectorPart]) -> u32 {
    parts
        .iter()
        .map(|part| {
            let simple = &part.simple;
            simple.id.is_some() as u32 * 100
                + simple.classes.len() as u32 * 10
                + simple.tag.is_some() as u32
        })
        .sum()
}

/// Matches a selector chain against a node. Public because the browser's
/// query surface (MCP `fox.dom.query`, devtools DOM) is the same matcher
/// the cascade uses — one selector grammar, one matching implementation.
pub fn selector_matches(dom: &Dom, node_id: NodeId, parts: &[SelectorPart]) -> bool {
    let Some((last, rest)) = parts.split_last() else {
        return true;
    };
    if !simple_matches(dom, node_id, &last.simple) {
        return false;
    }
    match last.combinator {
        Combinator::Child => match dom.node(node_id).parent {
            Some(parent) => selector_matches(dom, parent, rest),
            None => false,
        },
        Combinator::Descendant => {
            let mut ancestor = dom.node(node_id).parent;
            while let Some(id) = ancestor {
                if selector_matches(dom, id, rest) {
                    return true;
                }
                ancestor = dom.node(id).parent;
            }
            false
        }
    }
}

fn simple_matches(dom: &Dom, node_id: NodeId, simple: &SimpleSelector) -> bool {
    let Some(element) = dom.node(node_id).element() else {
        return false;
    };
    if let Some(tag) = &simple.tag {
        if element.tag != *tag {
            return false;
        }
    }
    if let Some(id) = &simple.id {
        if element.attr("id") != Some(id.as_str()) {
            return false;
        }
    }
    let node_classes = element.classes();
    for class in &simple.classes {
        if !node_classes.contains(&class.as_str()) {
            return false;
        }
    }
    true
}

pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return match hex.len() {
            3 => u8::from_str_radix(&hex[..1], 16).ok().zip(u8::from_str_radix(&hex[1..2], 16).ok()).zip(u8::from_str_radix(&hex[2..3], 16).ok()).map(|((r, g), b)| Color::rgba(r, g, b, 255)),
            4 => {
                let r = u8::from_str_radix(&hex[..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
                Some(Color::rgba(r * 17, g * 17, b * 17, a * 17))
            }
            6 => {
                let r = u8::from_str_radix(&hex[..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::rgba(r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Color::rgba(r, g, b, a))
            }
            _ => None,
        };
    }
    let lower = value.to_ascii_lowercase();
    let (r, g, b): (u8, u8, u8) = match lower.as_str() {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (220, 38, 38),
        "green" => (34, 139, 58),
        "blue" => (29, 78, 116),
        "orange" => (249, 115, 22),
        "amber" => (245, 158, 11),
        "yellow" => (250, 204, 21),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "teal" => (18, 94, 94),
        "navy" => (11, 31, 51),
        "maroon" => (128, 0, 0),
        "olive" => (109, 94, 26),
        "lime" => (132, 204, 22),
        "purple" => (107, 33, 168),
        _ => return None,
    };
    if lower == "transparent" {
        return Some(Color::rgba(0, 0, 0, 0));
    }
    Some(Color::rgba(r, g, b, 255))
}

fn parse_size(value: &str) -> Size {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Size::Auto;
    }
    if let Some(percent) = value.strip_suffix('%') {
        if let Ok(p) = percent.trim().parse::<f32>() {
            return Size::Percent(p / 100.0);
        }
    }
    match parse_px(value) {
        Some(px) => Size::Px(px),
        None => Size::Auto,
    }
}

fn parse_px(value: &str) -> Option<f32> {
    let value = value.trim();
    let num_len = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').count();
    if num_len == 0 {
        return None;
    }
    let number: f32 = value[..num_len].trim().parse().ok()?;
    let unit = value[num_len..].trim();
    match unit {
        "" | "px" => Some(number),
        "pt" => Some(number * 4.0 / 3.0),
        _ => None,
    }
}

fn parse_font_size(value: &str, parent: f32) -> Option<f32> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f32>().ok().map(|p| parent * p / 100.0);
    }
    if let Some(em) = value.strip_suffix("em") {
        return em.trim().parse::<f32>().ok().map(|e| parent * e);
    }
    if let Some(rem) = value.strip_suffix("rem") {
        return rem.trim().parse::<f32>().ok().map(|r| INITIAL_FONT_SIZE * r);
    }
    match value.to_ascii_lowercase().as_str() {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(INITIAL_FONT_SIZE),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        _ => parse_px(value),
    }
}

/// 1..4-value shorthand: `m`, `m v`, `m v m`, `t r b l`.
fn parse_edges(value: &str) -> Edges {
    let values: Vec<Size> = value.split_ascii_whitespace().map(parse_size).collect();
    let (t, r, b, l) = match values.as_slice() {
        [t] => (*t, *t, *t, *t),
        [v, h] => (*v, *h, *v, *h),
        [t, h, b] => (*t, *h, *b, *h),
        [t, r, b, l] => (*t, *r, *b, *l),
        _ => return Edges::default(),
    };
    Edges { top: t, right: r, bottom: b, left: l }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::parse_html;

    #[test]
    fn cascade_specificity_and_inheritance() {
        let html = "<html><body><p class='lead'>Hi</p><em>there</em></body></html>";
        let dom = parse_html(html);
        let sheet = crate::css::parse_stylesheet(
            "p { color: red; } .lead { color: orange; font-size: 2em; margin: 8px 16px; }",
        );
        let computed = cascade(&dom, &sheet);
        let p = dom.find_first_tag("p").unwrap();
        let style = &computed[p.0 as usize];
        assert_eq!(style.color, parse_color("orange").unwrap());
        assert_eq!(style.font_size, 32.0); // 2em of inherited 16
        assert_eq!(style.margin.top, Size::Px(8.0));
        assert_eq!(style.margin.left, Size::Px(16.0));
        let em = dom.find_first_tag("em").unwrap();
        assert_eq!(computed[em.0 as usize].color, parse_color("orange").unwrap());
    }

    #[test]
    fn inline_wins_over_sheet() {
        let dom = parse_html("<div style='display:none'>x</div>");
        let sheet = crate::css::parse_stylesheet("div { display: block; }");
        let computed = cascade(&dom, &sheet);
        let div = dom.find_first_tag("div").unwrap();
        assert_eq!(computed[div.0 as usize].display, Display::None);
    }
}
