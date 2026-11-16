//! Page context: turning an `openfox-engine` `Page` into the one text
//! block an assistant may be told about. The DOM knows where the text
//! is; this module decides what counts as text — script, style, and
//! template bodies are invisible by definition, and invisible text must
//! not become context (a rule the reader's extractor taught this
//! codebase).
//!
//! The output is plain prose with block boundaries as newlines, plus
//! the page title as the first line. It is capped at
//! [`MAX_CONTEXT_CHARS`]: context windows are budgets, and a page that
//! fills one alone leaves no room for the question.

use crate::safety::wrap_page_context;
use openfox_engine::dom::{Node, NodeId, NodeKind};
use openfox_engine::Page;

/// The context block never exceeds this many characters. ~12k characters
/// is ~3k estimated tokens — most of a small model's window, all of the
/// local summarizer's needs.
pub const MAX_CONTEXT_CHARS: usize = 12_000;

/// Elements whose entire subtree carries no visible text. The `head` is
/// listed even though its text nodes are metadata: titles are taken
/// from `Page::title`, and a `<style>` block's contents are CSS, not
/// prose.
const INVISIBLE_TAGS: &[&str] = &["script", "style", "noscript", "template", "head"];

/// Elements that start a new line — the sentence splitter in
/// `summarize` reads newlines as block boundaries.
const BLOCK_TAGS: &[&str] = &[
    "p", "div", "section", "article", "li", "blockquote", "h1", "h2", "h3", "h4", "h5", "h6",
    "tr", "br", "hr", "figure", "figcaption",
];

/// A little headroom past the cap before the walk stops early: cutting
/// the tree walk mid-branch is fine, cutting it mid-line is not.
const WALK_HEADROOM: usize = 1024;

/// The extracted, capped page text with its title.
#[derive(Debug, Clone, Default)]
pub struct PageContext {
    pub title: String,
    pub text: String,
}

/// Extract the assistant-visible text of `page`.
pub fn page_context(page: &Page) -> PageContext {
    let mut out = String::with_capacity(2048);
    collect(page, page.dom.root, &mut out);
    // Collapse the whitespace the DOM leaves behind into prose: runs of
    // spaces become one, blank lines become one newline. The text stays
    // verbatim otherwise — context must be quotable.
    let mut normalized = String::with_capacity(out.len());
    let mut pending_break = false;
    for line in out.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            continue;
        }
        if pending_break && !normalized.is_empty() {
            normalized.push('\n');
        }
        normalized.push_str(&collapsed);
        pending_break = true;
    }
    if normalized.len() > MAX_CONTEXT_CHARS {
        // Truncate at the last whitespace boundary so the cap never cuts
        // a word or a surrogate pair in half.
        let mut end = MAX_CONTEXT_CHARS;
        while end > 0 && !normalized.is_char_boundary(end) {
            end -= 1;
        }
        while end > 0 && !normalized.as_bytes()[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        normalized.truncate(end);
        normalized.push_str(" …");
    }
    PageContext { title: page.title.clone().unwrap_or_default(), text: normalized }
}

/// The ready-to-insert envelope: rule + delimiters + title + text, one
/// system message worth of page reality.
pub fn wrapped_context(page: &Page) -> String {
    let context = page_context(page);
    if context.title.is_empty() {
        wrap_page_context(&context.text)
    } else {
        wrap_page_context(&format!("{}\n{}", context.title, context.text))
    }
}

/// Depth-first text collection over the arena. Recursion depth equals
/// DOM nesting depth; pages nest deep but not stack-overflow deep, and
/// the reader walks the same trees recursively.
fn collect(page: &Page, id: NodeId, out: &mut String) {
    if out.len() > MAX_CONTEXT_CHARS + WALK_HEADROOM {
        return; // over budget: stop walking, the cap trims the rest
    }
    let node: &Node = page.dom.node(id);
    match &node.kind {
        NodeKind::Text(text) => {
            if !text.trim().is_empty() {
                out.push_str(text);
                out.push(' ');
            }
        }
        NodeKind::Element(element) => {
            if INVISIBLE_TAGS.contains(&element.tag.as_str()) {
                return;
            }
            if BLOCK_TAGS.contains(&element.tag.as_str()) {
                out.push('\n');
            }
            for child in node.children.iter().copied() {
                collect(page, child, out);
            }
            if BLOCK_TAGS.contains(&element.tag.as_str()) {
                out.push('\n');
            }
        }
        NodeKind::Document => {
            for child in node.children.iter().copied() {
                collect(page, child, out);
            }
        }
    }
}
