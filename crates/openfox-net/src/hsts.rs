//! HSTS: the Strict-Transport-Security store. max-age and
//! includeSubDomains per host, a short embedded preload list, and
//! upgrade decisions that answer before any request is built.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Hosts that never deserve a plain-HTTP request. The real preload list
/// ships as a generated blob; this is the shape of it.
pub const PRELOAD: &[&str] = &[
    "github.com",
    "wikipedia.org",
    "mozilla.org",
    "cloudflare.com",
    "arxiv.org",
    "openssl.org",
    "torproject.org",
    "letsencrypt.org",
];

#[derive(Debug, Clone)]
pub struct HstsEntry {
    pub include_subdomains: bool,
    pub expires_at: Instant,
}

pub struct HstsStore {
    entries: HashMap<String, HstsEntry>,
}

impl Default for HstsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HstsStore {
    pub fn new() -> Self {
        let mut entries = HashMap::new();
        for host in PRELOAD {
            entries.insert(
                (*host).to_owned(),
                HstsEntry {
                    include_subdomains: true,
                    expires_at: Instant::now() + Duration::from_secs(10 * 365 * 24 * 3600),
                },
            );
        }
        HstsStore { entries }
    }

    /// Parse a Strict-Transport-Security header into an entry. max-age
    /// is required; zero means delete — the spec's one honest exit.
    pub fn observe(&mut self, host: &str, header_value: &str) {
        let mut max_age: Option<u64> = None;
        let mut include_subdomains = false;
        for directive in header_value.split(';') {
            let directive = directive.trim();
            if let Some(age) = directive.strip_prefix("max-age=").or_else(|| {
                directive
                    .strip_prefix("MAX-AGE=")
            }) {
                max_age = age.trim().parse().ok();
            }
            if directive.eq_ignore_ascii_case("includeSubDomains") {
                include_subdomains = true;
            }
        }
        match max_age {
            Some(0) => {
                self.entries.remove(host);
            }
            Some(seconds) => {
                self.entries.insert(
                    host.to_owned(),
                    HstsEntry {
                        include_subdomains,
                        expires_at: Instant::now() + Duration::from_secs(seconds),
                    },
                );
            }
            None => {} // max-age absent: not an HSTS header, ignore
        }
    }

    /// Should this URL be upgraded before the request is built? Exact
    /// host match or subdomain of an includeSubDomains entry.
    pub fn should_upgrade(&self, url: &url::Url) -> bool {
        if url.scheme() != "http" {
            return false;
        }
        let Some(host) = url.host_str() else { return false };
        if let Some(entry) = self.entries.get(host) {
            if entry.expires_at > Instant::now() {
                return true;
            }
        }
        self.entries.iter().any(|(known, entry)| {
            entry.include_subdomains
                && entry.expires_at > Instant::now()
                && host.ends_with(&format!(".{known}"))
        })
    }

    /// Serialize for persistence. Instants become remaining seconds —
    /// an HSTS entry that outlives a sleep is an entry worth keeping.
    pub fn serialize(&self) -> Vec<(String, bool, u64)> {
        let now = Instant::now();
        self.entries
            .iter()
            .filter_map(|(host, entry)| {
                if entry.expires_at <= now {
                    return None;
                }
                Some((
                    host.clone(),
                    entry.include_subdomains,
                    (entry.expires_at - now).as_secs(),
                ))
            })
            .collect()
    }

    pub fn restore(&mut self, saved: Vec<(String, bool, u64)>) {
        for (host, include_subdomains, seconds) in saved {
            self.entries.insert(
                host,
                HstsEntry {
                    include_subdomains,
                    expires_at: Instant::now() + Duration::from_secs(seconds),
                },
            );
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
