//! URL upgrades that happen before a socket opens. HTTPS-Only rewrites
//! plain http to https when the port is the default one; de-AMP lifts a
//! page out of the publisher's AMP cache copy onto the canonical article.
//! Both are policy, not parsing: the browser decides where it goes, not
//! the page that linked it. Failures fall back — an upgrade that cannot
//! connect falls back to plain http only when the user's mode says
//! "upgrade", never in "only" mode, which is the caller's decision to
//! make with the retry budget it owns.

/// An HTTPS upgrade verdict: the rewritten URL, or why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upgrade {
    /// The URL can be fetched as-is: already https, or not http at all.
    Unchanged,
    /// The http URL was rewritten to https.
    ToHttps(url::Url),
    /// Rewriting would be a guess we refuse to make: non-default port,
    /// credentials in the URL, or a scheme that is neither http nor
    /// https (file:, data:, about: — the caller's domain, not ours).
    Refused(&'static str),
}

/// HTTPS-Only mode, the Firefox shape. `Upgrade` is best-effort with
/// fallback; `Only` is hard-fail — a site that cannot speak https is a
/// site the user has decided not to visit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpsMode {
    /// Plain http allowed; no rewriting.
    #[default]
    Off,
    /// http:// tried as https:// first, falling back once per navigation.
    Upgrade,
    /// http:// refused outright.
    Only,
}

/// Rewrites `http://host/path` to `https://host/path` when the port is
/// the default one. Non-default ports are refused: an http port is a
/// deliberate choice by the site operator, and guessing an https port
/// would connect somewhere they never offered.
pub fn https_upgrade(url: &url::Url) -> Upgrade {
    if url.scheme() != "http" {
        return Upgrade::Unchanged;
    }
    let default_port = url.port().is_none() || url.port() == Some(80);
    if !default_port {
        return Upgrade::Refused("non-default port");
    }
    if url.has_authority() && !url.username().is_empty() {
        return Upgrade::Refused("credentials in url");
    }
    let mut https = url.clone();
    https.set_scheme("https").ok();
    if https.scheme() != "https" {
        // url refuses scheme changes that would break the URL's shape;
        // treat that as a refusal rather than a panic path.
        return Upgrade::Refused("scheme rewrite rejected");
    }
    https.set_port(None).ok();
    Upgrade::ToHttps(https)
}

/// Extracts the `<link rel="canonical">` target from a document head.
/// AMP pages carry one; the canonical is the publisher's own URL for the
/// article the cache copied. Returns `None` when the document does not
/// declare one — no guesswork, no heuristics beyond the declared link.
pub fn canonical_link(html_head: &str) -> Option<String> {
    // Case-insensitive scan for link tags without a full HTML parse: the
    // head is small, tags are flat, and a full parser here would make the
    // net stack depend on the engine. The engine re-parses for real when
    // the page loads; this is only the redirect decision.
    let lower = html_head.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(open) = lower[cursor..].find("<link") {
        let tag_start = cursor + open;
        let tag_end = lower[tag_start..].find('>').map(|i| tag_start + i)?;
        let tag = &html_head[tag_start..tag_end];
        let tag_lower = &lower[tag_start..tag_end];
        let rel_is_canonical = tag_lower
            .split(&[' ', '\t', '\n'][..])
            .any(|attr| attr.trim_start_matches("rel").trim_start_matches(['=', '"', '\''])
                .trim_end_matches(['"', '\'']).eq_ignore_ascii_case("canonical"));
        if rel_is_canonical {
            if let Some(href) = extract_href(tag) {
                return Some(href);
            }
        }
        cursor = tag_end + 1;
    }
    None
}

/// The href attribute value of one tag, quotes stripped.
fn extract_href(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let href_at = lower.find("href")?;
    let after = &tag[href_at + 4..];
    let after = after.trim_start();
    let after = after.strip_prefix('=')?;
    let after = after.trim_start();
    let (value, _) = if let Some(stripped) = after.strip_prefix('"') {
        (stripped, '"')
    } else if let Some(stripped) = after.strip_prefix('\'') {
        (stripped, '\'')
    } else {
        (after, ' ')
    };
    let end = value.find(|c: char| c == '"' || c == '\'' || c.is_whitespace()).unwrap_or(value.len());
    Some(value[..end].to_owned())
}

/// Decides whether `current` is an AMP copy that should be lifted to its
/// canonical article. True when the path smells of AMP (a leading `/amp/`
/// segment, an `amp.` subdomain, or the `.amp.html` suffix) AND a
/// canonical link resolves against the current URL.
pub fn deamp_url(html_head: &str, current: &url::Url) -> Option<url::Url> {
    let host = current.host_str().unwrap_or_default();
    let path = current.path();
    let looks_amp = path.starts_with("/amp/")
        || path.starts_with("/amp.")
        || path.ends_with(".amp.html")
        || path.ends_with("/amp")
        || host.starts_with("amp.")
        || host.starts_with("cdn.ampproject.org");
    if !looks_amp {
        return None;
    }
    let canonical = canonical_link(html_head)?;
    let resolved = current.join(&canonical).ok()?;
    // A canonical pointing back at the AMP page itself is a publisher
    // bug; treating it as a target would loop. Only leave when the
    // resolved URL actually differs.
    if resolved == *current {
        return None;
    }
    Some(resolved)
}
