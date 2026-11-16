//! Redirect resources: the byte-level replacements a blocked request
//! answers with. A blocked ad slot gets a transparent pixel or an empty
//! script — never a substitution, never a "better ad."

/// The 1x1 transparent GIF: 43 bytes, the correct ones.
pub const TRANSPARENT_GIF: &str = "data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==";

/// The 1x1 transparent PNG.
pub const TRANSPARENT_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// Empty resources: nothing is a valid answer to a blocked request.
pub const EMPTY_JS: &str = "data:text/javascript,";
pub const EMPTY_CSS: &str = "data:text/css,";
pub const EMPTY_HTML: &str = "data:text/html,";

/// Resource kinds the engine can substitute for a blocked request,
/// keyed by the rule's resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Image,
    Script,
    Stylesheet,
    Document,
    Media,
}

impl ResourceKind {
    pub fn data_uri(self) -> &'static str {
        match self {
            ResourceKind::Image => TRANSPARENT_GIF,
            ResourceKind::Script => EMPTY_JS,
            ResourceKind::Stylesheet => EMPTY_CSS,
            ResourceKind::Document => EMPTY_HTML,
            ResourceKind::Media => TRANSPARENT_GIF,
        }
    }
}

/// The resource a redirect-style rule (`$redirect=`) resolves to.
/// Unknown names return None — an unknown redirect resource is a
/// rule error, not a passthrough.
pub fn resource_for(name: &str) -> Option<&'static str> {
    match name {
        "1x1.gif" | "1x1-transparent.gif" => Some(TRANSPARENT_GIF),
        "1x1.png" | "1x1-transparent.png" => Some(TRANSPARENT_PNG),
        "noop.js" | "noop.txt" => Some(EMPTY_JS),
        "noop.css" => Some(EMPTY_CSS),
        "noop.html" | "empty.html" => Some(EMPTY_HTML),
        _ => None,
    }
}

/// Extension web_accessible_resources: extensions get a namespaced
/// table (`ext://{extension-id}/{path}`), never raw filesystem access.
/// The table is the entire surface — an extension reading outside its
/// namespace is a bug this file cannot have.
pub fn extension_resource_uri(extension_id: &str, path: &str) -> String {
    format!("ext://{extension_id}/{}", path.trim_start_matches('/'))
}

pub fn parse_extension_resource(uri: &str) -> Option<(String, String)> {
    let rest = uri.strip_prefix("ext://")?;
    let (id, path) = rest.split_once('/')?;
    if id.is_empty() || path.is_empty() {
        return None;
    }
    Some((id.to_owned(), path.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gif_is_the_real_gif() {
        // 43 bytes base64 -> the canonical 1x1 transparent GIF.
        assert!(TRANSPARENT_GIF.starts_with("data:image/gif;base64,R0lGOD"));
    }

    #[test]
    fn extension_resources_stay_namespaced() {
        let uri = extension_resource_uri("ublock", "css/filter.css");
        assert_eq!(
            parse_extension_resource(&uri),
            Some(("ublock".into(), "css/filter.css".into()))
        );
        assert!(parse_extension_resource("file:///etc/passwd").is_none());
    }
}
