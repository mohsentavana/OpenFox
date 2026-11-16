//! Page metadata: titles, descriptions, og tags, favicon candidates,
//! and the frecency math that ranks top sites. Extraction is a pure
//! function over meta pairs — the engine collects, this file decides.

/// Extracted page metadata, ready for history, top sites, and shares.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PageMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_title: Option<String>,
    pub og_image: Option<String>,
    pub canonical: Option<String>,
    pub lang: Option<String>,
}

/// Parse (name|property, content) pairs into metadata. og:title wins
/// over <title> only when the caller says the title is missing — social
/// preview semantics, not page semantics.
pub fn extract(meta_pairs: &[(String, String)], title: Option<&str>) -> PageMetadata {
    let mut metadata = PageMetadata {
        title: title.map(str::to_owned),
        ..Default::default()
    };
    for (name, content) in meta_pairs {
        let key = name.to_ascii_lowercase();
        let value = content.trim();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "description" => metadata.description = Some(value.to_owned()),
            "og:title" => metadata.og_title = Some(value.to_owned()),
            "og:image" | "og:image:url" => metadata.og_image = Some(value.to_owned()),
            "canonical" => metadata.canonical = Some(value.to_owned()),
            "lang" | "language" => metadata.lang = Some(value.to_owned()),
            _ => {}
        }
    }
    metadata
}

/// Favicon candidates for one page, in priority order: declared icon
/// links (largest first), then the host default.
#[derive(Debug, Clone, PartialEq)]
pub struct FaviconCandidate {
    pub url: String,
    /// Declared pixel size when known; the host default is a guess.
    pub size: Option<u32>,
}

pub fn favicon_candidates(
    icon_links: &[(String, u32, String)], // (url, declared size, rel)
    host: &str,
) -> Vec<FaviconCandidate> {
    let mut candidates: Vec<FaviconCandidate> = icon_links
        .iter()
        .filter(|(_, _, rel)| rel.split_whitespace().any(|r| r.eq_ignore_ascii_case("icon")))
        .map(|(url, size, _)| FaviconCandidate {
            url: url.clone(),
            size: Some(*size),
        })
        .collect();
    candidates.sort_by(|a, b| b.size.cmp(&a.size));
    candidates.push(FaviconCandidate {
        url: format!("https://{host}/favicon.ico"),
        size: None,
    });
    candidates
}

/// The frecency events that feed ranking. Weights follow Firefox's
/// recipe: a bookmark is worth eight visits, a typed url two.
#[derive(Debug, Clone, Copy)]
pub enum FrecencyEvent {
    Visit,
    Bookmark,
    Typed,
    LinkClick,
}

impl FrecencyEvent {
    fn weight(self) -> f32 {
        match self {
            FrecencyEvent::Visit => 1.0,
            FrecencyEvent::Bookmark => 8.0,
            FrecencyEvent::Typed => 2.0,
            FrecencyEvent::LinkClick => 0.25,
        }
    }
}

/// Frecency with decay: points halve as they age through the buckets
/// (today, this week, this month, older). Computed on read — no
/// background jobs maintaining what nobody asked for yet.
pub fn frecency(events: &[(FrecencyEvent, u64)], now_ms: u64) -> f32 {
    const DAY: u64 = 86_400_000;
    let mut score = 0.0f32;
    for (event, at_ms) in events {
        let age_days = now_ms.saturating_sub(*at_ms) / DAY;
        let decay = if age_days < 1 {
            1.0
        } else if age_days < 7 {
            0.5
        } else if age_days < 30 {
            0.25
        } else {
            0.125
        };
        score += event.weight() * decay;
    }
    score
}

/// Top-sites aggregation input: one host's frecency over its URLs.
pub fn host_score(host: &str, per_url_events: &[(String, FrecencyEvent, u64)], now_ms: u64) -> (String, f32) {
    let events: Vec<(FrecencyEvent, u64)> = per_url_events
        .iter()
        .filter(|(url, _, _)| url.contains(host))
        .map(|(_, event, at)| (*event, *at))
        .collect();
    (host.to_owned(), frecency(&events, now_ms))
}
