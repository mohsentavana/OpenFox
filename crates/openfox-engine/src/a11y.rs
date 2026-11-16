//! The accessibility tree: roles from semantics, names computed per the
//! accname algorithm, bounds from layout, focus order with real tab
//! index semantics. This is the tree screen readers walk — and the same
//! tree MCP's fox.a11y.tree serves. One source, two consumers.

use crate::dom::NodeId;

#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    Button,
    Link,
    Heading { level: u8 },
    Image,
    Navigation,
    Main,
    Article,
    TextInput,
    Checkbox,
    Radio,
    Slider,
    List,
    ListItem,
    Table,
    Cell,
    Paragraph,
    HeadingLarge,
    Generic,
    Document,
}

#[derive(Debug, Clone)]
pub struct A11yNode {
    pub node: NodeId,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub role: Role,
    pub name: String,
    pub description: String,
    /// Layout bounds in CSS pixels; the OS layer converts.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub focusable: bool,
    /// Positive tabindex participates in the explicit order; None rides
    /// DOM order.
    pub tab_index: Option<i32>,
    pub focused: bool,
    pub disabled: bool,
    pub checked: Option<bool>,
}

/// Inputs the tree builder takes per DOM node: semantics from the
/// document, layout from the box tree. Pure data, no arena coupling.
#[derive(Debug, Clone)]
pub struct A11yInput {
    pub node: NodeId,
    pub tag: String,
    pub explicit_role: Option<String>,
    pub aria_label: Option<String>,
    pub aria_labelledby: Option<String>,
    pub label_for: Option<String>,
    pub inner_text: String,
    pub alt: Option<String>,
    pub href: Option<String>,
    pub tabindex: Option<i32>,
    pub disabled: bool,
    pub checked: Option<bool>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Map one input to a role: explicit role wins, then tag semantics.
/// This is the accessible-name computation's cheaper sibling — same
/// priority order (aria-label > labelledby > label-for > text > alt).
pub fn build_role(input: &A11yInput) -> Role {
    if let Some(explicit) = &input.explicit_role {
        return match explicit.as_str() {
            "button" => Role::Button,
            "link" => Role::Link,
            "navigation" => Role::Navigation,
            "main" => Role::Main,
            "article" => Role::Article,
            "textbox" => Role::TextInput,
            "checkbox" => Role::Checkbox,
            "radio" => Role::Radio,
            "slider" => Role::Slider,
            "list" => Role::List,
            "table" => Role::Table,
            _ => Role::Generic,
        };
    }
    match input.tag.as_str() {
        "button" => Role::Button,
        "a" if input.href.is_some() => Role::Link,
        "img" => Role::Image,
        "nav" => Role::Navigation,
        "main" => Role::Main,
        "article" => Role::Article,
        "input" => match input.checked {
            Some(_) if input.explicit_role.is_none() => Role::Checkbox,
            _ => Role::TextInput,
        },
        "ul" | "ol" => Role::List,
        "li" => Role::ListItem,
        "table" => Role::Table,
        "td" | "th" => Role::Cell,
        "p" => Role::Paragraph,
        "h1" => Role::Heading { level: 1 },
        "h2" => Role::Heading { level: 2 },
        "h3" => Role::Heading { level: 3 },
        "h4" => Role::Heading { level: 4 },
        "h5" => Role::Heading { level: 5 },
        "h6" => Role::Heading { level: 6 },
        _ => Role::Generic,
    }
}

/// The accessible name, in priority order.
pub fn compute_name(input: &A11yInput) -> String {
    if let Some(label) = &input.aria_label {
        return label.clone();
    }
    if let Some(labelledby) = &input.aria_labelledby {
        // Referenced ids resolve against the document; the caller passes
        // them pre-joined through this field when they exist.
        return labelledby.clone();
    }
    if let Some(label) = &input.label_for {
        return label.clone();
    }
    if let Some(alt) = &input.alt {
        if !alt.is_empty() {
            return alt.clone();
        }
    }
    input.inner_text.trim().chars().take(200).collect()
}

/// Is the node focusable: explicit tabindex first, then native
/// semantics. Negative tabindex: focusable by script, skipped by Tab —
/// expressed as focusable=true, tab_order=None.
pub fn focusable(input: &A11yInput) -> (bool, Option<i32>) {
    match input.tabindex {
        Some(index) if index < 0 => (true, None),
        Some(index) => (true, Some(index)),
        None => {
            let native = matches!(
                input.tag.as_str(),
                "a" | "button" | "input" | "select" | "textarea"
            ) && input.href.is_some() || matches!(input.tag.as_str(), "button" | "input" | "select" | "textarea");
            (native, None)
        }
    }
}

/// Tab order: positive tabindex values in ascending order first, then
/// DOM order for natively focusable / tabindex=0 nodes — the spec's
/// exact rule, because muscle memory depends on it.
pub fn tab_order(nodes: &[A11yNode], dom_order: &[usize]) -> Vec<usize> {
    let mut explicit: Vec<(i32, usize)> = Vec::new();
    let mut natural: Vec<usize> = Vec::new();
    for index in dom_order {
        let node = &nodes[*index];
        if !node.focusable || node.disabled {
            continue;
        }
        match node.tab_index {
            Some(positive) if positive > 0 => explicit.push((positive, *index)),
            _ => natural.push(*index),
        }
    }
    explicit.sort_by_key(|(order, _)| *order);
    let mut ordered: Vec<usize> = explicit.into_iter().map(|(_, index)| index).collect();
    ordered.extend(natural);
    ordered
}

/// The audit checks devtools' a11y panel runs over the built tree. Each
/// returns Some(node) when the check fails.
pub fn audit(nodes: &[A11yNode]) -> Vec<Check> {
    let mut failures = Vec::new();
    for node in nodes {
        match &node.role {
            Role::Image if node.name.is_empty() => failures.push(Check {
                severity: Severity::Error,
                kind: CheckKind::ImageWithoutName,
                node: node.node,
            }),
            Role::Button if node.name.is_empty() => failures.push(Check {
                severity: Severity::Error,
                kind: CheckKind::ButtonWithoutLabel,
                node: node.node,
            }),
            Role::Link if node.name.is_empty() => failures.push(Check {
                severity: Severity::Error,
                kind: CheckKind::LinkWithoutLabel,
                node: node.node,
            }),
            _ => {}
        }
    }
    failures
}

#[derive(Debug, Clone)]
pub struct Check {
    pub severity: Severity,
    pub kind: CheckKind,
    pub node: NodeId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CheckKind {
    ImageWithoutName,
    ButtonWithoutLabel,
    LinkWithoutLabel,
    LowContrast,
    HeadingOrder,
}
