//! Bounce tracking mitigation: trackers redirect through an intermediate
//! domain to plant a cookie, then forward you along — so the cookie is
//! "first-party" to the tracker and follows you out. We cut the middle
//! hop out and strip the tracking decoration from the landing URL.

use crate::is_third_party;
use std::collections::HashMap;
use url::Url;

/// (host, path prefix) pairs. An empty prefix means every path on the
/// host bounces; stubhub.com only bounces through its /redirect
/// handler, so the rest of that site stays reachable.
const REDIRECTORS: &[(&str, &str)] = &[
    ("l.facebook.com", ""),
    ("l.messenger.com", ""),
    ("app.link", ""),
    ("share.theweb.com", ""),
    ("goto.target.com", ""),
    ("ccmp.eu", ""),
    ("atomurl.com", ""),
    ("stubhub.com", "/redirect"),
];

/// Click identifiers and campaign parameters that exist to follow the
/// user across the hop.
const TRACKING_PARAMS: &[&str] = &["gclid", "fbclid", "msclkid", "mc_eid", "_ga"];

/// How many navigation chains the ledger keeps — a window for the
/// shields panel, not an archive.
const MAX_RECORDED: usize = 256;

/// Collapses redirect chains so bounce trackers never get their cookie
/// planted. One instance lives next to the navigation stack.
#[derive(Debug, Clone, Default)]
pub struct BounceMitigator {
    /// nav_id → the chain as it actually executed.
    history: HashMap<String, Vec<String>>,
}

impl BounceMitigator {
    /// Given every hop of one navigation, return the hops that should
    /// execute: known redirectors in the middle are cut, the final
    /// destination always survives, and honest intermediaries (single
    /// sign-on, payment hand-offs) are left alone. The collapsed chain
    /// is recorded under `nav_id` for the ledger.
    pub fn collapse(&mut self, nav_id: &str, hops: &[String]) -> Vec<String> {
        let last = hops.len().saturating_sub(1);
        let kept: Vec<String> = hops
            .iter()
            .enumerate()
            .filter(|(i, hop)| *i == last || !is_redirector(hop))
            .map(|(_, hop)| hop.clone())
            .collect();
        if self.history.len() >= MAX_RECORDED {
            self.history.clear();
        }
        self.history.insert(nav_id.to_owned(), kept.clone());
        kept
    }

    /// The chain recorded for one navigation, if it went through us.
    pub fn chain(&self, nav_id: &str) -> Option<&[String]> {
        self.history.get(nav_id).map(Vec::as_slice)
    }
}

/// Strip `gclid`, `fbclid`, `msclkid`, `mc_eid`, `_ga` and every `utm_*`
/// parameter when the URL's registrable domain differs from the top
/// frame's. Same-site parameters stay — the user chose that site, and
/// its own analytics are its own business, not ours.
pub fn clean_url(url: &Url, top: &Url) -> Url {
    // Same-site: the user chose this site, its parameters are its own.
    if !is_third_party(url.host_str().unwrap_or(""), top.host_str().unwrap_or("")) {
        return url.clone();
    }
    let mut kept = url::form_urlencoded::Serializer::new(String::new());
    let mut stripped = 0usize;
    for (key, value) in url.query_pairs() {
        if is_tracking_param(&key) {
            stripped += 1;
        } else {
            kept.append_pair(&key, &value);
        }
    }
    if stripped == 0 {
        return url.clone();
    }
    let query = kept.finish();
    let mut cleaned = url.clone();
    if query.is_empty() {
        cleaned.set_query(None);
    } else {
        cleaned.set_query(Some(&query));
    }
    cleaned
}

/// Whether a query parameter is a cross-site tracking decoration.
fn is_tracking_param(key: &str) -> bool {
    TRACKING_PARAMS.contains(&key) || key.starts_with("utm_")
}

/// Whether a hop URL sits on a known redirector host, honoring the
/// path-prefix rule. Unparseable hops are not redirectors — they are
/// somebody else's problem.
fn is_redirector(hop: &str) -> bool {
    let Ok(url) = Url::parse(hop) else {
        return false;
    };
    let Some(host) = url.host_str() else { return false };
    let path = url.path();
    REDIRECTORS.iter().any(|(r_host, r_path)| {
        let host_hit = host == *r_host || host.ends_with(&format!(".{r_host}"));
        host_hit && (r_path.is_empty() || path.starts_with(r_path))
    })
}
