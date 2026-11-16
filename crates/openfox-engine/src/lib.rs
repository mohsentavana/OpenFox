pub mod a11y;
pub mod css;
pub mod dom;
pub mod events;
pub mod html;
pub mod iframe;
pub mod js;
pub mod layout;
pub mod paint;
pub mod selection;
pub mod style;

pub use dom::{Dom, NodeId};
pub use html::parse_html;
pub use layout::{layout_tree, LayoutBox, Rect};
pub use paint::{paint, DisplayItem, DisplayList};
pub use style::{cascade, Color, Computed, Display as DisplayKind, Size};

pub const VIEWPORT_WIDTH: f32 = 1280.0;
pub const VIEWPORT_HEIGHT: f32 = 800.0;

/// One fully processed page: DOM, computed style, solved boxes and the page
/// title. The display list is derived on demand (`display_list()`), so
/// re-painting after a resize reuses the cascade.
pub struct Page {
    pub dom: Dom,
    pub style: Vec<Computed>,
    pub boxes: LayoutBox,
    pub title: Option<String>,
}

impl Page {
    pub fn build(html: &str, extra_css: &[&str]) -> Page {
        let dom = parse_html(html);
        let mut css_sources: Vec<String> = extra_css.iter().map(|css| css.to_string()).collect();
        dom.walk(dom.root, &mut |id, node| {
            if let Some(data) = node.element() {
                if data.tag == "style" {
                    css_sources.push(dom.inner_text(id));
                }
            }
        });
        let sheet = css::parse_stylesheet(&css_sources.join("\n"));
        let style = style::cascade(&dom, &sheet);
        let boxes = layout_tree(&dom, &style, VIEWPORT_WIDTH, VIEWPORT_HEIGHT);
        let title = dom.find_first_tag("title").map(|t| dom.inner_text(t).trim().to_owned()).filter(|t| !t.is_empty());
        Page { dom, style, boxes, title }
    }

    pub fn display_list(&self) -> DisplayList {
        paint(&self.dom, &self.style, &self.boxes)
    }

    /// Collects the CSS text of `<link rel="stylesheet">` targets so the
    /// caller can fetch them through the network stack before building.
    pub fn linked_stylesheets(html: &str) -> Vec<String> {
        let dom = parse_html(html);
        let mut urls = Vec::new();
        dom.walk(dom.root, &mut |id, node| {
            if let Some(data) = node.element() {
                if data.tag == "link" && data.attr("rel").is_some_and(|r| r.eq_ignore_ascii_case("stylesheet")) {
                    if let Some(href) = data.attr("href") {
                        urls.push(href.to_owned());
                    }
                }
            }
        });
        urls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_builds_end_to_end() {
        let html = "<html><head><title>Fox</title><style>body { background: white; margin: 8px } h1 { color: orange; font-size: 24px }</style></head><body><h1>Speed</h1><p>Bytes to GPU.</p></body></html>";
        let page = Page::build(html, &[]);
        assert_eq!(page.title.as_deref(), Some("Fox"));
        let list = page.display_list();
        assert!(list.len() >= 2); // body background + two text runs
        assert!(list.items.iter().any(|item| matches!(item, DisplayItem::Rect { .. })));
        assert!(list.items.iter().any(|item| matches!(item, DisplayItem::Text { .. })));
    }

    #[test]
    fn linked_stylesheets_are_found() {
        let html = "<html><head><link rel=\"stylesheet\" href=\"/a.css\"><link rel=\"icon\" href=\"/f.ico\"></head></html>";
        assert_eq!(Page::linked_stylesheets(html), vec!["/a.css".to_owned()]);
    }
}
