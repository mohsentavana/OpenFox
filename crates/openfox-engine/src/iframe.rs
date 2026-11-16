//! The frame tree: iframes own frames, frames own documents. The
//! sandbox attribute parses to flags here; cross-origin frames get
//! their own arena and style scope, same-origin frames share the
//! parent's — the boundary is a policy, not an afterthought.

use crate::dom::NodeId;

/// The sandbox attribute, parsed. An absent sandbox attribute means no
/// sandbox — full permissions, the web's default.
pub const SANDBOX_SCRIPTS: u16 = 1 << 0;
pub const SANDBOX_SAME_ORIGIN: u16 = 1 << 1;
pub const SANDBOX_FORMS: u16 = 1 << 2;
pub const SANDBOX_POPUPS: u16 = 1 << 3;
pub const SANDBOX_TOP_NAVIGATION: u16 = 1 << 4;
pub const SANDBOX_POINTER_LOCK: u16 = 1 << 5;

/// Parse the sandbox attribute value: comma-separated tokens, unknown
/// tokens ignored (the spec's advice — fail open to the tokens you know).
pub fn parse_sandbox(value: &str) -> u16 {
    let mut flags = 0u16;
    for token in value.split(',') {
        match token.trim() {
            "allow-scripts" => flags |= SANDBOX_SCRIPTS,
            "allow-same-origin" => flags |= SANDBOX_SAME_ORIGIN,
            "allow-forms" => flags |= SANDBOX_FORMS,
            "allow-popups" => flags |= SANDBOX_POPUPS,
            "allow-top-navigation" => flags |= SANDBOX_TOP_NAVIGATION,
            "allow-pointer-lock" => flags |= SANDBOX_POINTER_LOCK,
            _ => {}
        }
    }
    flags
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub id: u64,
    pub parent: Option<u64>,
    pub url: String,
    /// The document root in the owning arena. Same-origin children share
    /// the parent arena; cross-origin children get their own.
    pub document: Option<NodeId>,
    pub sandbox: u16,
    /// srcdoc frames: the document came inline, not over the network.
    pub srcdoc: Option<String>,
}

#[derive(Debug, Default)]
pub struct FrameTree {
    frames: Vec<Frame>,
    next_id: u64,
}

impl FrameTree {
    /// Attach the main frame. Every tree has exactly one parentless
    /// frame; attaching a second replaces the first (navigation).
    pub fn attach_main(&mut self, url: &str) -> u64 {
        self.frames.retain(|frame| frame.parent.is_some());
        let id = self.next_id;
        self.next_id += 1;
        self.frames.push(Frame {
            id,
            parent: None,
            url: url.to_owned(),
            document: None,
            sandbox: 0,
            srcdoc: None,
        });
        id
    }

    /// Attach a child frame for an <iframe>. Returns its id.
    pub fn attach_child(
        &mut self,
        parent: u64,
        url: &str,
        sandbox: u16,
        srcdoc: Option<String>,
    ) -> Option<u64> {
        if !self.frames.iter().any(|frame| frame.id == parent) {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.frames.push(Frame {
            id,
            parent: Some(parent),
            url: url.to_owned(),
            document: None,
            sandbox,
            srcdoc,
        });
        Some(id)
    }

    pub fn detach(&mut self, id: u64) {
        // Detaching a parent detaches the whole subtree — an iframe full
        // of iframes goes as one unit.
        let mut doomed = vec![id];
        loop {
            let mut grew = false;
            for frame in &self.frames {
                if frame.parent.is_some_and(|p| doomed.contains(&p)) && !doomed.contains(&frame.id)
                {
                    doomed.push(frame.id);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        self.frames.retain(|frame| !doomed.contains(&frame.id));
    }

    pub fn get(&self, id: u64) -> Option<&Frame> {
        self.frames.iter().find(|frame| frame.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Frame> {
        self.frames.iter_mut().find(|frame| frame.id == id)
    }

    pub fn main(&self) -> Option<&Frame> {
        self.frames.iter().find(|frame| frame.parent.is_none())
    }

    pub fn children_of(&self, id: u64) -> Vec<u64> {
        self.frames
            .iter()
            .filter(|frame| frame.parent == Some(id))
            .map(|frame| frame.id)
            .collect()
    }

    pub fn depth(&self, id: u64) -> usize {
        let mut depth = 0;
        let mut cursor = self.get(id);
        while let Some(frame) = cursor {
            cursor = frame.parent.and_then(|parent| self.get(parent));
            depth += 1;
        }
        depth - 1
    }

    /// Load ordering: depth-first, parents complete after children — a
    /// parent's load event fires when every descendant is done, which is
    /// the contract scripts actually rely on.
    pub fn load_order(&self) -> Vec<u64> {
        let mut order = Vec::with_capacity(self.frames.len());
        fn walk(tree: &FrameTree, id: u64, order: &mut Vec<u64>) {
            for child in tree.children_of(id) {
                walk(tree, child, order);
            }
            order.push(id);
        }
        if let Some(main) = self.main() {
            walk(self, main.id, &mut order);
        }
        order
    }
}
