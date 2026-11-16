//! Tracker classification: the answer to "what is this request, and is it
//! watching the user?" The host lists are compiled in — no remote list
//! fetch, ever. A request that matches nothing is `Content`, because the
//! alternative is blocking the web to save it.

use crate::is_third_party;
use serde::{Deserialize, Serialize};
use std::fmt;

/// What a matched tracker host does for a living (`Content` = not one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackerCategory {
    /// Ad exchanges and beacons.
    Advertising,
    /// Cross-site measurement panels.
    Analytics,
    /// Social platforms' tracking arms.
    Social,
    /// Active fingerprinting scripts.
    Fingerprinting,
    /// In-page cryptomining.
    Cryptomining,
    /// Everything else. Not a tracker by our books.
    Content,
}

impl fmt::Display for TrackerCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TrackerCategory::Advertising => "advertising",
            TrackerCategory::Analytics => "analytics",
            TrackerCategory::Social => "social",
            TrackerCategory::Fingerprinting => "fingerprinting",
            TrackerCategory::Cryptomining => "cryptomining",
            TrackerCategory::Content => "content",
        })
    }
}

const ADVERTISING: &[&str] = &[
    "doubleclick.net",
    "googlesyndication.com",
    "adnxs.com",
    "taboola.com",
    "criteo.com",
];

const ANALYTICS: &[&str] = &[
    "google-analytics.com",
    "scorecardresearch.com",
    "hotjar.com",
    "amplitude.com",
];

/// Social widgets that phone home with a browsing history attached.
const SOCIAL: &[&str] = &[
    "facebook.net",
    "connect.facebook.net",
    "ads-twitter.com",
    "analytics.tiktok.com",
];

/// Fingerprinting vendors.
const FINGERPRINTING: &[&str] = &["fingerprintjs.com", "fpjs.io"];

/// Cycle thieves.
const CRYPTOMINING: &[&str] = &["coinhive.com", "cryptoloot.com"];

/// The kind of resource being fetched — context for the reason string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Document,
    Script,
    Image,
    Stylesheet,
    Xhr,
    Font,
    Media,
    Other,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ResourceType::Document => "document",
            ResourceType::Script => "script",
            ResourceType::Image => "image",
            ResourceType::Stylesheet => "stylesheet",
            ResourceType::Xhr => "xhr",
            ResourceType::Font => "font",
            ResourceType::Media => "media",
            ResourceType::Other => "other",
        })
    }
}

/// Everything the classifier needs, all borrowed: whose page is loading,
/// who is being loaded, and what kind of thing it is.
#[derive(Debug, Clone, Copy)]
pub struct ClassifyRequest<'a> {
    /// Host of the top-level page the user is actually looking at.
    pub top_host: &'a str,
    /// Host the subresource is being fetched from.
    pub request_host: &'a str,
    /// What the engine is fetching.
    pub resource_type: ResourceType,
}

/// The classifier's verdict on one request. `blocked` is the raw
/// recommendation — a known third-party tracker; `crate::etp::EtpPolicy`
/// makes the final call, and this flag feeds the shield badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// Whether the classifier recommends blocking.
    pub blocked: bool,
    /// What the request matched, or `Content` when it did not.
    pub category: TrackerCategory,
    /// Whether the request's registrable domain differs from the page's.
    pub third_party: bool,
    /// Human-readable one-liner for the shields panel.
    pub reason: String,
}

/// Classify one subresource request. Third-partyness is decided by
/// registrable domain, not string equality — `a.example.co.uk` and
/// `b.example.co.uk` are the same party; `example.com` and
/// `example.co.uk` are not. First-party is always `Content`.
pub fn classify(req: ClassifyRequest<'_>) -> Decision {
    let third_party = is_third_party(req.top_host, req.request_host);
    let category = if third_party {
        match_category(req.request_host)
    } else {
        TrackerCategory::Content
    };
    let blocked = third_party && category != TrackerCategory::Content;
    let reason = if blocked {
        format!(
            "third-party {category} {} ({})",
            req.resource_type, req.request_host
        )
    } else if third_party {
        format!("third-party {}, unclassified", req.resource_type)
    } else {
        "first-party".to_owned()
    };
    Decision { blocked, category, third_party, reason }
}

/// The category for a third-party host, or `Content` when nothing
/// matches — fingerprinting and cryptomining are checked first.
fn match_category(host: &str) -> TrackerCategory {
    const LISTS: &[(&[&str], TrackerCategory)] = &[
        (FINGERPRINTING, TrackerCategory::Fingerprinting),
        (CRYPTOMINING, TrackerCategory::Cryptomining),
        (ADVERTISING, TrackerCategory::Advertising),
        (ANALYTICS, TrackerCategory::Analytics),
        (SOCIAL, TrackerCategory::Social),
    ];
    for &(hosts, category) in LISTS {
        if hosts.iter().any(|h| host_matches(host, h)) {
            return category;
        }
    }
    TrackerCategory::Content
}

fn host_matches(host: &str, tracker: &str) -> bool {
    host == tracker || host.ends_with(&format!(".{tracker}"))
}
