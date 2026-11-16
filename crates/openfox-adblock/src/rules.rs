use crate::AdblockError;

/// Which list a rule came from. The shield badge counts ads and trackers
/// separately, and the count is per kind, not per list file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Ads,
    Trackers,
    Custom,
}

/// Resource type as the network layer classifies requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Document,
    Script,
    Image,
    Stylesheet,
    Xhr,
    Media,
    Font,
    Other,
}

impl ResourceType {
    fn bit(self) -> u32 {
        match self {
            ResourceType::Document => 1 << 0,
            ResourceType::Script => 1 << 1,
            ResourceType::Image => 1 << 2,
            ResourceType::Stylesheet => 1 << 3,
            ResourceType::Xhr => 1 << 4,
            ResourceType::Media => 1 << 5,
            ResourceType::Font => 1 << 6,
            ResourceType::Other => 1 << 7,
        }
    }
}

/// One network rule after parsing. `parts` are lowercase literals that must
/// appear in order; `*` and `^` in the source text become boundaries.
#[derive(Debug, Clone)]
pub struct NetworkRule {
    pub source: String,
    pub exception: bool,
    pub kind: ListKind,
    pub host_anchor: Option<String>,
    pub prefix_anchor: bool,
    pub suffix_anchor: bool,
    pub parts: Vec<String>,
    pub resource: u32,
    pub third_party_only: bool,
}

impl NetworkRule {
    /// Parses one adblock-syntax line. Cosmetic lines (`##`) are rejected
    /// here and handled by the cosmetic engine — they never reach this
    /// function's caller mixed in.
    pub fn parse(line: &str, kind: ListKind) -> Result<Option<NetworkRule>, AdblockError> {
        let raw = line.trim();
        if raw.is_empty() || raw.starts_with('!') || raw.starts_with('#') || raw.starts_with('[') {
            return Ok(None);
        }
        if raw.contains("##") || raw.contains("#@#") || raw.contains("#?#") {
            return Ok(None);
        }

        let (exception, body) = match raw.strip_prefix("@@") {
            Some(rest) => (true, rest),
            None => (false, raw),
        };

        let (filter, options) = match body.split_once('$') {
            Some((f, o)) => (f, Some(o)),
            None => (body, None),
        };
        if filter.is_empty() {
            return Err(AdblockError::EmptyRule(raw.to_owned()));
        }

        let mut rule = NetworkRule {
            source: raw.to_owned(),
            exception,
            kind,
            host_anchor: None,
            prefix_anchor: false,
            suffix_anchor: false,
            parts: Vec::new(),
            resource: 0,
            third_party_only: false,
        };

        if let Some(opts) = options {
            for opt in opts.split(',') {
                match opt.trim().to_ascii_lowercase().as_str() {
                    "" => {}
                    "script" => rule.resource |= ResourceType::Script.bit(),
                    "image" => rule.resource |= ResourceType::Image.bit(),
                    "stylesheet" => rule.resource |= ResourceType::Stylesheet.bit(),
                    "xhr" | "xmlhttprequest" => rule.resource |= ResourceType::Xhr.bit(),
                    "media" => rule.resource |= ResourceType::Media.bit(),
                    "font" => rule.resource |= ResourceType::Font.bit(),
                    "document" | "doc" => rule.resource |= ResourceType::Document.bit(),
                    "third-party" => rule.third_party_only = true,
                    other => return Err(AdblockError::UnsupportedOption(other.into(), raw.into())),
                }
            }
        }

        let mut body = filter;
        if let Some(rest) = body.strip_prefix("||") {
            let (host, remainder) = match rest.find(['^', '/']) {
                Some(i) => (rest[..i].to_owned(), &rest[i..]),
                None => (rest.to_owned(), ""),
            };
            if host.is_empty() {
                return Err(AdblockError::EmptyRule(raw.to_owned()));
            }
            rule.host_anchor = Some(host.to_ascii_lowercase());
            body = remainder;
        }
        if body.starts_with('|') {
            rule.prefix_anchor = true;
            body = &body[1..];
        }
        if body.ends_with('|') {
            rule.suffix_anchor = true;
            body = &body[..body.len() - 1];
        }

        for seg in body.split(['*', '^']) {
            let seg = seg.trim_matches('|');
            if !seg.is_empty() {
                rule.parts.push(seg.to_ascii_lowercase());
            }
        }
        if rule.host_anchor.is_none() && rule.parts.is_empty() {
            return Err(AdblockError::EmptyRule(raw.to_owned()));
        }
        Ok(Some(rule))
    }

    pub fn matches(&self, url: &url::Url, resource: ResourceType, source_host: Option<&str>) -> bool {
        if self.resource != 0 && self.resource & resource.bit() == 0 {
            return false;
        }
        if self.third_party_only {
            let Some(host) = url.host_str() else { return false };
            let first_party = source_host.is_some_and(|src| {
                host == src || host.ends_with(&format!(".{src}")) || src.ends_with(&format!(".{host}"))
            });
            if first_party {
                return false;
            }
        }
        if let Some(anchor) = &self.host_anchor {
            let Some(host) = url.host_str() else { return false };
            if !(host.eq_ignore_ascii_case(anchor) || host.ends_with(&format!(".{anchor}"))) {
                return false;
            }
        }
        if self.parts.is_empty() {
            return true;
        }
        let hay = url.as_str().to_ascii_lowercase();
        let mut from = 0;
        for part in &self.parts {
            match hay[from..].find(part.as_str()) {
                Some(i) => from += i + part.len(),
                None => return false,
            }
        }
        if self.prefix_anchor {
            if let Some(first) = self.parts.first() {
                if !hay.starts_with(first.as_str()) {
                    return false;
                }
            }
        }
        if self.suffix_anchor {
            if let Some(last) = self.parts.last() {
                if !hay.ends_with(last.as_str()) {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn host_anchors_by_label() {
        let rule = NetworkRule::parse("||doubleclick.net^", ListKind::Ads).unwrap().unwrap();
        assert!(rule.matches(
            url("https://ad.doubleclick.net/ddm/adj/x"),
            ResourceType::Script,
            None
        ));
        assert!(!rule.matches(url("https://notdoubleclick.net.example.com/x"), ResourceType::Script, None));
        assert!(!rule.matches(url("https://example.com/doubleclick.net"), ResourceType::Script, None));
    }

    #[test]
    fn exceptions_want_lists() {
        let rule = NetworkRule::parse("@@||example.com^$script", ListKind::Ads).unwrap().unwrap();
        assert!(rule.exception);
        assert!(rule.matches(url("https://example.com/a.js"), ResourceType::Script, None));
        assert!(!rule.matches(url("https://example.com/a.js"), ResourceType::Image, None));
    }

    #[test]
    fn third_party_only_respects_source() {
        let rule = NetworkRule::parse("||cdn.io^$third-party", ListKind::Ads).unwrap().unwrap();
        assert!(rule.matches(url("https://cdn.io/x"), ResourceType::Script, Some("news.org")));
        assert!(!rule.matches(url("https://cdn.io/x"), ResourceType::Script, Some("cdn.io")));
    }

    #[test]
    fn cosmetic_lines_are_not_network_rules() {
        assert!(NetworkRule::parse("example.com##.ad", ListKind::Ads).unwrap().is_none());
    }
}
