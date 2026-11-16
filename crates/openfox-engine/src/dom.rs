/// Arena DOM. Nodes live in one Vec and refer to each other by index; no
/// Rc webs, no cycles, no leak hunting. `NodeId` is a plain u32 with a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone)]
pub struct ElementData {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
}

impl ElementData {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn classes(&self) -> Vec<&str> {
        match self.attr("class") {
            Some(value) => value.split_ascii_whitespace().collect(),
            None => Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    Document,
    Element(ElementData),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

impl Node {
    pub fn element(&self) -> Option<&ElementData> {
        match &self.kind {
            NodeKind::Element(data) => Some(data),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Text(t) => Some(t.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dom {
    pub nodes: Vec<Node>,
    pub root: NodeId,
}

impl Dom {
    pub fn with_document() -> Dom {
        let mut dom = Dom { nodes: Vec::with_capacity(256), root: NodeId(0) };
        dom.nodes.push(Node { kind: NodeKind::Document, parent: None, children: Vec::new() });
        dom
    }

    pub fn create_element(&mut self, tag: String, attrs: Vec<(String, String)>) -> NodeId {
        self.alloc(NodeKind::Element(ElementData { tag: tag.to_ascii_lowercase(), attrs }))
    }

    pub fn create_text(&mut self, text: String) -> NodeId {
        self.alloc(NodeKind::Text(text))
    }

    fn alloc(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node { kind, parent: None, children: Vec::new() });
        id
    }

    pub fn append(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[child.0 as usize].parent = Some(parent);
        self.nodes[parent.0 as usize].children.push(child);
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn tag(&self, id: NodeId) -> Option<&str> {
        self.node(id).element().map(|e| e.tag.as_str())
    }

    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        self.node(id).element().and_then(|e| e.attr(name))
    }

    pub fn classes(&self, id: NodeId) -> Vec<&str> {
        self.node(id).element().map(|e| e.classes()).unwrap_or_default()
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.node(id).children
    }

    /// Pre-order walk over every node below `from`.
    pub fn walk<F: FnMut(NodeId, &Node)>(&self, from: NodeId, visit: &mut F) {
        let mut stack = vec![from];
        while let Some(id) = stack.pop() {
            let node = self.node(id);
            visit(id, node);
            for child in node.children.iter().rev() {
                stack.push(*child);
            }
        }
    }

    pub fn find_first_tag(&self, tag: &str) -> Option<NodeId> {
        let mut found = None;
        self.walk(self.root, &mut |id, node| {
            if found.is_none() {
                if let Some(data) = node.element() {
                    if data.tag == tag {
                        found = Some(id);
                    }
                }
            }
        });
        found
    }

    /// Concatenated text below `id`, deepest-first order preserved.
    pub fn inner_text(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.walk(id, &mut |_, node| {
            if let Some(text) = node.text() {
                out.push_str(text);
            }
        });
        out
    }

    /// Element children of `id`.
    pub fn element_children(&self, id: NodeId) -> Vec<NodeId> {
        self.children(id).iter().copied().filter(|&c| self.node(c).element().is_some()).collect()
    }
}
