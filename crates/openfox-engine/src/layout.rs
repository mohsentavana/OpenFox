use crate::dom::{Dom, NodeId, NodeKind};
use crate::style::{Computed, Display, Size};
use std::collections::HashMap;

/// Estimated advance widths for latin text before the shaper exists. Used
/// only to size text leaves; the renderer draws real glyphs.
const GLYPH_ADVANCE: f32 = 0.55;
const LINE_HEIGHT: f32 = 1.3;

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub node: NodeId,
    pub rect: Rect,
    pub children: Vec<LayoutBox>,
}

/// Solves the box tree with taffy. Phase 1 lays out block flow: `display:
/// inline` participates as a block (own line), `flex` becomes a taffy
/// flexbox, `none` is not laid out at all. Text leaves are sized from the
/// advance estimate — inline wrapping arrives with the shaper.
pub fn layout_tree(dom: &Dom, computed: &[Computed], width: f32, height: f32) -> LayoutBox {
    let mut tree: taffy::TaffyTree = taffy::TaffyTree::new();
    let mut map: HashMap<NodeId, taffy::NodeId> = HashMap::new();

    let root = build_box(dom, computed, &mut tree, &mut map, dom.root);
    let root = match root {
        Some(node) => node,
        None => {
            // An empty or fully hidden document still needs a root box.
            tree.new_leaf(taffy::Style::default()).expect("taffy leaf")
        }
    };

    let full = taffy::Style {
        size: taffy::Size { width: taffy::Dimension::length(width), height: taffy::Dimension::length(height) },
        ..taffy::Style::default()
    };
    // Re-style the root to the viewport; the document element stretches to it.
    tree.set_style(root, full).expect("taffy root style");
    tree.compute_layout(
        root,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(width),
            height: taffy::AvailableSpace::Definite(height),
        },
    );

    collect_boxes(dom, &tree, &map, dom.root)
}

fn build_box(
    dom: &Dom,
    computed: &[Computed],
    tree: &mut taffy::TaffyTree,
    map: &mut HashMap<NodeId, taffy::NodeId>,
    node_id: NodeId,
) -> Option<taffy::NodeId> {
    let style = &computed[node_id.0 as usize];
    if style.display == Display::None {
        return None;
    }
    let node = dom.node(node_id);

    if let NodeKind::Text(text) = &node.kind {
        let w = text.chars().count() as f32 * GLYPH_ADVANCE * style.font_size;
        let h = style.font_size * LINE_HEIGHT;
        let leaf = taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::length(w.max(1.0)),
                height: taffy::Dimension::length(h.max(style.font_size)),
            },
            ..taffy::Style::default()
        };
        let id = tree.new_leaf(leaf).expect("taffy leaf");
        map.insert(node_id, id);
        return Some(id);
    }

    let children: Vec<taffy::NodeId> = node
        .children
        .iter()
        .filter_map(|child| build_box(dom, computed, tree, map, *child))
        .collect();

    let taffy_style = box_style(style);
    let id = if children.is_empty() {
        tree.new_leaf(taffy_style).expect("taffy leaf")
    } else {
        tree.new_with_children(taffy_style, &children).expect("taffy container")
    };
    map.insert(node_id, id);
    Some(id)
}

fn box_style(style: &Computed) -> taffy::Style {
    let display = match style.display {
        Display::Flex => taffy::Display::Flex,
        // Block flow: inline content participates as its own block in phase 1.
        Display::Block | Display::Inline => taffy::Display::Block,
        Display::None => taffy::Display::None,
    };
    taffy::Style {
        display,
        margin: taffy::Rect {
            top: length_auto(style.margin.top),
            right: length_auto(style.margin.right),
            bottom: length_auto(style.margin.bottom),
            left: length_auto(style.margin.left),
        },
        padding: taffy::Rect {
            top: length(style.padding.top),
            right: length(style.padding.right),
            bottom: length(style.padding.bottom),
            left: length(style.padding.left),
        },
        size: taffy::Size { width: dimension(style.width), height: dimension(style.height) },
        // Corner radii are a paint concern: they ride on `Computed` and the
        // display list, not on taffy's layout styles.
        ..taffy::Style::default()
    }
}

fn dimension(size: Size) -> taffy::Dimension {
    match size {
        Size::Auto => taffy::Dimension::auto(),
        Size::Px(px) => taffy::Dimension::length(px),
        Size::Percent(p) => taffy::Dimension::percent(p / 100.0),
    }
}

fn length(size: Size) -> taffy::LengthPercentage {
    match size {
        Size::Auto => taffy::LengthPercentage::length(0.0),
        Size::Px(px) => taffy::LengthPercentage::length(px),
        Size::Percent(p) => taffy::LengthPercentage::percent(p / 100.0),
    }
}

fn length_auto(size: Size) -> taffy::LengthPercentageAuto {
    match size {
        Size::Auto => taffy::LengthPercentageAuto::auto(),
        Size::Px(px) => taffy::LengthPercentageAuto::length(px),
        Size::Percent(p) => taffy::LengthPercentageAuto::percent(p / 100.0),
    }
}

fn collect_boxes(dom: &Dom, tree: &taffy::TaffyTree, map: &HashMap<NodeId, taffy::NodeId>, node_id: NodeId) -> LayoutBox {
    let rect = match map.get(&node_id) {
        Some(&tnode) => {
            let layout = tree.layout(tnode).expect("layout solved");
            Rect { x: layout.location.x, y: layout.location.y, w: layout.size.width, h: layout.size.height }
        }
        None => Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
    };
    let children = dom
        .children(node_id)
        .iter()
        .filter(|child| map.contains_key(child))
        .map(|child| collect_boxes(dom, tree, map, *child))
        .collect();
    LayoutBox { node: node_id, rect, children }
}
