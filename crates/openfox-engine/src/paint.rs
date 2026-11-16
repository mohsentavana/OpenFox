use crate::dom::{Dom, NodeKind};
use crate::layout::{LayoutBox, Rect};
use crate::style::{Color, Computed, Size};

/// The render target of the whole engine: a flat, ordered list of drawing
/// commands in DOM order. This is the WebRender-shaped boundary — everything
/// before it is retained data, everything after it is GPU work.
#[derive(Debug, Clone)]
pub enum DisplayItem {
    Rect { rect: Rect, color: Color, radius: f32 },
    Text { rect: Rect, text: String, size: f32, weight: u16, color: Color },
}

#[derive(Debug, Clone, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
}

impl DisplayList {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Walks solved boxes and emits the list. Backgrounds paint before children
/// (DOM order), text paints on its leaf box, radius rounds rects.
pub fn paint(dom: &Dom, computed: &[Computed], boxes: &LayoutBox) -> DisplayList {
    let mut items = Vec::with_capacity(64);
    emit(dom, computed, boxes, &mut items);
    DisplayList { items }
}

fn emit(dom: &Dom, computed: &[Computed], box_node: &LayoutBox, items: &mut Vec<DisplayItem>) {
    let style = &computed[box_node.node.0 as usize];
    if let Some(background) = style.background {
        let radius = match style.radius {
            Size::Px(px) => px,
            _ => 0.0,
        };
        items.push(DisplayItem::Rect { rect: box_node.rect, color: background, radius });
    }
    if let NodeKind::Text(text) = &dom.node(box_node.node).kind {
        items.push(DisplayItem::Text {
            rect: box_node.rect,
            text: text.clone(),
            size: style.font_size,
            weight: style.font_weight,
            color: style.color,
        });
    }
    for child in &box_node.children {
        emit(dom, computed, child, items);
    }
}
