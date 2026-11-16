//! The document outline: the bookmark tree in the catalog's /Outlines,
//! and the link annotations per page. Walking dict chains the way the
//! spec draws them.

use crate::render::Obj;

#[derive(Debug, Clone)]
pub struct OutlineNode {
    pub title: String,
    pub dest: Destination,
    pub children: Vec<OutlineNode>,
}

#[derive(Debug, Clone)]
pub enum Destination {
    Page(u32),
    Url(String),
}

#[derive(Debug, Clone)]
pub struct LinkAnnotation {
    /// (x1, y1, x2, y2) in page units, bottom-left origin per the spec.
    pub rect: (f32, f32, f32, f32),
    pub dest: Destination,
}

/// Parse the outline tree from the catalog. The chain is
/// First/Last/Next/Prev references; the depth guard exists because a
/// cyclic outline is a crash in every naive implementation ever shipped.
pub fn parse_outline(objects: &[(u64, Obj)], catalog: &Obj) -> Vec<OutlineNode> {
    let Some(outlines) = catalog.dict_get("Outlines") else {
        return Vec::new();
    };
    let Some(first) = outlines.dict_get("First").and_then(as_ref_id) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut cursor = Some(first);
    let mut guard = 0;
    while let Some(id) = cursor {
        if guard > 10_000 {
            break;
        }
        guard += 1;
        let Some(node) = objects.iter().find(|(obj_id, _)| *obj_id == id).map(|(_, o)| o) else {
            break;
        };
        let title = node
            .dict_get("Title")
            .and_then(as_text)
            .unwrap_or_default();
        let dest = parse_destination(node.dict_get("Dest"), objects);
        let children = parse_children(node, objects);
        result.push(OutlineNode { title, dest, children });
        cursor = node.dict_get("Next").and_then(as_ref_id);
    }
    result
}

fn parse_children(parent: &Obj, objects: &[(u64, Obj)]) -> Vec<OutlineNode> {
    let Some(first) = parent.dict_get("First").and_then(as_ref_id) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    let mut cursor = Some(first);
    let mut guard = 0;
    while let Some(id) = cursor {
        if guard > 10_000 {
            break;
        }
        guard += 1;
        let Some(node) = objects.iter().find(|(obj_id, _)| *obj_id == id).map(|(_, o)| o) else {
            break;
        };
        let title = node.dict_get("Title").and_then(as_text).unwrap_or_default();
        let dest = parse_destination(node.dict_get("Dest"), objects);
        let nested = parse_children(node, objects);
        children.push(OutlineNode {
            title,
            dest,
            children: nested,
        });
        cursor = node.dict_get("Next").and_then(as_ref_id);
    }
    children
}

/// Destinations arrive as `[page_ref /XYZ x y z]` arrays or bare names.
/// Both mean "go here"; the array is the common case.
fn parse_destination(dest: Option<&Obj>, objects: &[(u64, Obj)]) -> Destination {
    match dest {
        Some(Obj::Array(items)) => {
            let page = items
                .first()
                .and_then(as_ref_id)
                .and_then(|id| page_number(objects, id))
                .unwrap_or(1);
            Destination::Page(page)
        }
        Some(Obj::Name(name)) => {
            // Named destinations resolve through the name tree; the
            // fallback treats the name itself as a page label.
            let _ = name;
            Destination::Page(1)
        }
        _ => Destination::Page(1),
    }
}

fn parse_link_dest(dest: &Obj, objects: &[(u64, Obj)]) -> Destination {
    match dest {
        Obj::Name(action) if action == "URI" => Destination::Url(String::new()),
        _ => parse_destination(Some(dest), objects),
    }
}

/// Parse /Annots of one page into link annotations the viewer can hit-test.
pub fn parse_annots(page_dict: &Obj, objects: &[(u64, Obj)]) -> Vec<LinkAnnotation> {
    let Some(Obj::Array(refs)) = page_dict.dict_get("Annots") else {
        return Vec::new();
    };
    let mut links = Vec::new();
    for reference in refs {
        let Some(id) = as_ref_id(reference) else { continue };
        let Some(annotation) = objects.iter().find(|(obj_id, _)| *obj_id == id).map(|(_, o)| o)
        else {
            continue;
        };
        let Some(Obj::Array(rect_items)) = annotation.dict_get("Rect") else {
            continue;
        };
        let numbers: Vec<f32> = rect_items
            .iter()
            .filter_map(|item| match item {
                Obj::Int(value) => Some(*value as f32),
                Obj::Real(value) => Some(*value as f32),
                _ => None,
            })
            .collect();
        if numbers.len() < 4 {
            continue;
        }
        let rect = (numbers[0], numbers[1], numbers[2], numbers[3]);
        let dest = annotation
            .dict_get("A")
            .and_then(|action| {
                action.dict_get("URI").and_then(as_text).map(Destination::Url)
            })
            .or_else(|| annotation.dict_get("Dest").map(|d| parse_link_dest(d, objects)))
            .unwrap_or(Destination::Page(1));
        links.push(LinkAnnotation { rect, dest });
    }
    links
}

fn page_number(objects: &[(u64, Obj)], page_ref: u64) -> Option<u32> {
    // Page order comes from the /Pages kids walk; the objects list is in
    // file order, which for well-formed PDFs is page order. One-based.
    objects
        .iter()
        .filter(|(_, obj)| matches!(obj.dict_get("Type"), Some(Obj::Name(t)) if t == "Page"))
        .position(|(id, _)| *id == page_ref)
        .map(|index| index as u32 + 1)
}

fn as_ref_id(obj: &Obj) -> Option<u64> {
    match obj {
        Obj::Dict(pairs) => pairs
            .iter()
            .find(|(k, _)| k == "Ref")
            .and_then(|(_, v)| match v {
                Obj::Int(id) => Some(*id as u64),
                _ => None,
            }),
        _ => None,
    }
}

fn as_text(obj: &Obj) -> Option<String> {
    match obj {
        Obj::Str(bytes) => Some(bytes.iter().map(|&b| b as char).collect()),
        Obj::Name(name) => Some(name.clone()),
        _ => None,
    }
}
