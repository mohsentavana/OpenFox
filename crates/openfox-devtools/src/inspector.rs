//! The DOM inspector: flat node rows, breadcrumbs, the box-model overlay
//! and the edit intents the engine applies.

use serde_json::{json, Value};
use std::collections::HashMap;

/// Axis-aligned rect in page space, shared by every devtools overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One node as the tree list shows it: identity, geometry and a computed
/// style summary — the properties that explain why it looks like that,
/// not the whole cascade.
#[derive(Debug, Clone)]
pub struct DomNodeRow {
    pub node: u64,
    pub parent: Option<u64>,
    pub depth: u32,
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub dims: (f32, f32),
    pub computed: Vec<(String, String)>,
}

impl DomNodeRow {
    /// The row's label: `div#nav.menu.sidebar`, empty parts dropped.
    pub fn label(&self) -> String {
        let mut label = self.tag.clone();
        if let Some(id) = &self.id {
            label.push('#');
            label.push_str(id);
        }
        for class in &self.classes {
            label.push('.');
            label.push_str(class);
        }
        label
    }
}

/// The four nested rects the highlight overlay draws, outermost first.
#[derive(Debug, Clone, Copy)]
pub struct BoxModel {
    pub margin: Rect,
    pub border: Rect,
    pub padding: Rect,
    pub content: Rect,
}

/// A DOM edit, phrased as intent. The engine applies it to the live tree
/// and re-emits rows — the inspector never mutates the tree it inspects.
#[derive(Debug, Clone)]
pub enum EditIntent {
    SetAttribute { node: u64, name: String, value: String },
    RemoveNode { node: u64 },
    SetTextContent { node: u64, text: String },
}

/// Inspector state: the flattened tree, the selection, the hover, and the
/// outbox of edits waiting for the engine.
pub struct DomInspector {
    pub rows: Vec<DomNodeRow>,
    pub boxes: HashMap<u64, BoxModel>,
    pub selected: Option<u64>,
    pub hovered: Option<u64>,
    outbox: Vec<EditIntent>,
}

impl DomInspector {
    pub fn new() -> DomInspector {
        DomInspector { rows: Vec::new(), boxes: HashMap::new(), selected: None, hovered: None, outbox: Vec::new() }
    }

    /// Replaces the tree from an engine snapshot. Selection and hover are
    /// dropped if their nodes no longer exist — a dead highlight is worse
    /// than none.
    pub fn ingest(&mut self, rows: Vec<DomNodeRow>, boxes: HashMap<u64, BoxModel>) {
        self.selected = self.selected.filter(|node| rows.iter().any(|row| row.node == *node));
        self.hovered = self.hovered.filter(|node| rows.iter().any(|row| row.node == *node));
        self.rows = rows;
        self.boxes = boxes;
    }

    pub fn select(&mut self, node: Option<u64>) {
        self.selected = node;
    }

    /// Hover-to-highlight: the overlay follows the mouse within the tree
    /// list, and the page overlay follows the overlay.
    pub fn hover(&mut self, node: Option<u64>) {
        self.hovered = node;
    }

    /// The box model the overlay should draw: hover wins over selection,
    /// because the eye is on the pointer.
    pub fn highlight(&self) -> Option<BoxModel> {
        let node = self.hovered.or(self.selected)?;
        self.boxes.get(&node).copied()
    }

    /// Breadcrumb path from root to a node: `html > body > div#nav`.
    pub fn breadcrumb(&self, node: u64) -> Vec<String> {
        let mut path = Vec::new();
        let mut cursor = self.rows.iter().find(|row| row.node == node);
        while let Some(row) = cursor {
            path.push(row.label());
            cursor = row.parent.and_then(|parent| self.rows.iter().find(|r| r.node == parent));
        }
        path.reverse();
        path
    }

    /// Queues an edit and mirrors it into the local rows so the panel
    /// reacts instantly; the engine's next snapshot is the confirmation.
    pub fn edit(&mut self, intent: EditIntent) {
        match &intent {
            EditIntent::SetAttribute { node, name, value } => {
                if let Some(row) = self.rows.iter_mut().find(|row| row.node == *node) {
                    if name == "id" {
                        row.id = Some(value.clone());
                    } else if name == "class" {
                        row.classes = value.split_whitespace().map(str::to_owned).collect();
                    }
                }
            }
            EditIntent::RemoveNode { node } => {
                let mut gone = vec![*node];
                // A removed node takes its subtree with it.
                loop {
                    let children: Vec<u64> = self
                        .rows
                        .iter()
                        .filter(|row| row.parent.is_some_and(|parent| gone.contains(&parent)))
                        .map(|row| row.node)
                        .collect();
                    if children.is_empty() {
                        break;
                    }
                    gone.extend(children);
                }
                self.rows.retain(|row| !gone.contains(&row.node));
                if self.selected.is_some_and(|selected| gone.contains(&selected)) {
                    self.selected = None;
                }
                if self.hovered.is_some_and(|hovered| gone.contains(&hovered)) {
                    self.hovered = None;
                }
            }
            EditIntent::SetTextContent { .. } => {}
        }
        self.outbox.push(intent);
    }

    /// Takes the edits the engine owes a pass over.
    pub fn drain_edits(&mut self) -> Vec<EditIntent> {
        std::mem::take(&mut self.outbox)
    }

    pub fn to_json(&self) -> Value {
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|row| {
                json!({
                    "node": row.node,
                    "label": row.label(),
                    "depth": row.depth,
                    "dims": [row.dims.0, row.dims.1],
                    "computed": row.computed.iter()
                        .map(|(name, value)| json!({ "name": name, "value": value }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({ "rows": rows, "selected": self.selected, "breadcrumb": self.breadcrumb(self.selected.unwrap_or(0)) })
    }
}

impl Default for DomInspector {
    fn default() -> Self {
        DomInspector::new()
    }
}
