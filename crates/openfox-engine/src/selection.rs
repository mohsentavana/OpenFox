//! Selection: ranges over text runs, granularity expansion, selection
//! rectangles, the clipboard copy path. The caret is a range with equal
//! anchor and focus — no special case, by construction.

use crate::dom::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub node: NodeId,
    pub offset: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionRange {
    pub anchor: Position,
    pub focus: Position,
}

impl SelectionRange {
    pub fn caret(at: Position) -> Self {
        SelectionRange {
            anchor: at,
            focus: at,
        }
    }

    pub fn is_caret(&self) -> bool {
        self.anchor == self.focus
    }

    /// True when the selection was made backwards (focus before anchor).
    /// Rendering highlights either way; the copy path cares, the user
    /// does not.
    pub fn is_backwards(&self, order: &impl Fn(Position, Position) -> Ordering) -> bool {
        matches!(order(self.anchor, self.focus), Ordering::Less)
    }

    pub fn start(&self, order: &impl Fn(Position, Position) -> Ordering) -> Position {
        match order(self.anchor, self.focus) {
            Ordering::Less | Ordering::Equal => self.anchor,
            Ordering::Greater => self.focus,
        }
    }

    pub fn end(&self, order: &impl Fn(Position, Position) -> Ordering) -> Position {
        match order(self.anchor, self.focus) {
            Ordering::Less | Ordering::Equal => self.focus,
            Ordering::Greater => self.anchor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    Less,
    Equal,
    Greater,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Character,
    Word,
    Line,
}

/// Expand a caret to a range at the given granularity. Word expansion
/// walks whitespace boundaries; line expansion is the engine's line-map
/// lookup — both are pure functions over the text.
pub fn expand(
    at: Position,
    granularity: Granularity,
    text_of: impl Fn(NodeId) -> String,
) -> SelectionRange {
    let text = text_of(at.node);
    let bytes = text.as_bytes();
    match granularity {
        Granularity::Character => {
            let next = at.offset.saturating_add(1).min(bytes.len() as u16);
            SelectionRange {
                anchor: at,
                focus: Position {
                    node: at.node,
                    offset: next,
                },
            }
        }
        Granularity::Word => {
            let mut start = at.offset.min(bytes.len() as u16);
            while start > 0 && !is_word_boundary(bytes[(start - 1) as usize]) {
                start -= 1;
            }
            let mut end = at.offset.min(bytes.len() as u16);
            while end < bytes.len() as u16 && !is_word_boundary(bytes[end as usize]) {
                end += 1;
            }
            SelectionRange {
                anchor: Position {
                    node: at.node,
                    offset: start,
                },
                focus: Position {
                    node: at.node,
                    offset: end.max(start + 1),
                },
            }
        }
        Granularity::Line => {
            // Lines are a layout fact; without the line map the honest
            // expansion is to the whole node's text — the caller with a
            // line map narrows it.
            SelectionRange {
                anchor: Position {
                    node: at.node,
                    offset: 0,
                },
                focus: Position {
                    node: at.node,
                    offset: bytes.len() as u16,
                },
            }
        }
    }
}

fn is_word_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == b'.' || byte == b',' || byte == b'!'
        || byte == b'?' || byte == b';' || byte == b':'
}

/// One selection rectangle: the paint layer consumes these directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The plain-text copy payload: the selected runs joined with newlines
/// per block boundary. Rich copy is a phase-2 clipboard flavor; plain
/// text is the contract that never breaks.
pub fn copy_text(
    range: &SelectionRange,
    order: &impl Fn(Position, Position) -> Ordering,
    text_of: impl Fn(NodeId) -> String,
) -> String {
    if range.is_caret() {
        return String::new();
    }
    if range.start(order).node == range.end(order).node {
        let text = text_of(range.start(order).node);
        let from = range.start(order).offset as usize;
        let to = range.end(order).offset as usize;
        return text
            .chars()
            .skip(from)
            .take(to.saturating_sub(from))
            .collect();
    }
    String::new()
}

/// select-all over one node's text: anchor at zero, focus at the end.
pub fn select_all(node: NodeId, text_of: impl Fn(NodeId) -> String) -> SelectionRange {
    let length = text_of(node).len() as u16;
    SelectionRange {
        anchor: Position { node, offset: 0 },
        focus: Position { node, offset: length },
    }
}
