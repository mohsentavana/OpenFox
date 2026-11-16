//! Readability extraction over an abstract tree: score subtrees, keep the
//! winner, prune its children.

use std::collections::HashMap;

/// A DOM-free page tree. The engine converts its DOM into this shape so
/// the reader never depends on any particular parser's node type.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractNode {
    pub tag: String,
    pub text: String,
    pub children: Vec<ExtractNode>,
    pub attrs: HashMap<String, String>,
}

/// What reader mode shows: the winning subtree, pruned, plus the metadata
/// the chrome around it needs.
#[derive(Debug, Clone)]
pub struct Article {
    pub title: String,
    pub byline: Option<String>,
    /// Pruned copy of the winning subtree — children scoring below
    /// threshold are dropped.
    pub content: ExtractNode,
    pub text_chars: usize,
    /// At 200 words per minute, the standard reading-speed estimate.
    pub est_read_minutes: u32,
}

/// Link density above this and the subtree is navigation wearing a content
/// costume — Readability's classic cut line.
const MAX_LINK_DENSITY: f32 = 0.25;
/// Children below this score are pruned from the article copy.
const KEEP_THRESHOLD: f32 = 80.0;
/// Class/id fragments that mark chrome, not content.
const CHROME_HINTS: [&str; 7] = ["nav", "footer", "sidebar", "comment", "promo", "share", "related"];
/// Below this much text the page is not an article, whatever it thinks.
const MIN_CANDIDATE_CHARS: usize = 200;

/// Extracts the article from a page tree, or None when the page has no
/// article-shaped content to offer.
pub fn extract(root: &ExtractNode) -> Option<Article> {
    let title = find_first(root, &["title", "h1"]).unwrap_or_else(|| "Untitled".to_owned());
    let byline = find_byline(root);
    // Best subtree by score. script/style/noscript/template text is
    // invisible, and invisible text must not win a visibility contest.
    let mut best: Option<(&ExtractNode, f32)> = None;
    walk(root, &mut |n| {
        if matches!(n.tag.as_str(), "script" | "style" | "noscript" | "template") {
            return;
        }
        let s = score(n);
        if s > best.map_or(0.0, |(_, b)| b) {
            best = Some((n, s));
        }
    });
    let candidate = best.map(|(n, _)| n)?;
    if text_chars(candidate) < MIN_CANDIDATE_CHARS {
        return None;
    }
    let content = prune(candidate);
    let text_chars = text_chars(&content);
    // ~5 characters per word including the space between words.
    let est_read_minutes = ((((text_chars / 5) as u32).max(1)) / 200).max(1);
    Some(Article { title, byline, content, text_chars, est_read_minutes })
}

/// Scores a subtree for article-ness. Text mass and paragraphs are the
/// signal; link density past the cut line and chrome vocabulary in
/// class/id are the penalties.
fn score(node: &ExtractNode) -> f32 {
    let total = text_chars(node);
    if total == 0 {
        return 0.0;
    }
    let mut s = total as f32 + paragraph_count(node) as f32 * 25.0;
    // Commas proxy for actual prose (clauses); capped so comma farms lose.
    s += node.text.matches(',').count().min(total / 8) as f32 * 2.0;
    let density = link_chars(node) as f32 / total as f32;
    if density > MAX_LINK_DENSITY {
        s *= MAX_LINK_DENSITY / density;
    }
    if chrome(node) {
        s -= 50.0;
    }
    s
}

/// True when class or id carries a chrome marker. Only the node itself is
/// checked — an ancestor's sins are charged when that ancestor is scored.
fn chrome(node: &ExtractNode) -> bool {
    ["class", "id"].iter().any(|k| {
        node.attrs
            .get(*k)
            .map_or(false, |v| CHROME_HINTS.iter().any(|h| v.to_ascii_lowercase().contains(h)))
    })
}

/// Copies the winning subtree, dropping children that do not earn their
/// place. A lone <p> with one good sentence loses the score contest but
/// belongs in the article, so short paragraphs survive on tag alone.
fn prune(node: &ExtractNode) -> ExtractNode {
    let mut copy = node.clone();
    copy.children = node
        .children
        .iter()
        .filter(|c| score(c) >= KEEP_THRESHOLD || (c.tag == "p" && text_chars(c) >= 40))
        .map(prune)
        .collect();
    copy
}

/// First non-empty text under one of `tags`, depth-first: <title> wins
/// because it is authored; <h1> is the fallback when it is not.
fn find_first(node: &ExtractNode, tags: &[&str]) -> Option<String> {
    if tags.contains(&node.tag.as_str()) {
        let t = node.text.trim();
        if !t.is_empty() {
            return Some(t.to_owned());
        }
    }
    node.children.iter().find_map(|c| find_first(c, tags))
}

/// Byline hunting: rel="author" is the explicit signal; class/id hinting
/// "author" or "byline" is the pragmatic one the web actually uses. The
/// 80-char cap keeps a section labeled "authors" from being mistaken for
/// a name.
fn find_byline(node: &ExtractNode) -> Option<String> {
    let explicit = node.attrs.get("rel").map_or(false, |r| r.split_whitespace().any(|t| t == "author"));
    let hinted = ["class", "id"].iter().any(|k| {
        node.attrs.get(*k).map_or(false, |v| {
            let v = v.to_ascii_lowercase();
            v.contains("author") || v.contains("byline")
        })
    });
    if explicit || hinted {
        let t = node.text.trim();
        if !t.is_empty() && t.len() <= 80 {
            return Some(t.to_owned());
        }
    }
    node.children.iter().find_map(find_byline)
}

fn walk<'a>(node: &'a ExtractNode, f: &mut impl FnMut(&'a ExtractNode)) {
    f(node);
    for child in &node.children {
        walk(child, f);
    }
}

fn text_chars(node: &ExtractNode) -> usize {
    node.text.len() + node.children.iter().map(text_chars).sum::<usize>()
}

fn paragraph_count(node: &ExtractNode) -> usize {
    usize::from(node.tag == "p") + node.children.iter().map(paragraph_count).sum::<usize>()
}

/// The link-density numerator: every character under an anchor.
fn link_chars(node: &ExtractNode) -> usize {
    if node.tag == "a" {
        return text_chars(node);
    }
    node.children.iter().map(link_chars).sum()
}
