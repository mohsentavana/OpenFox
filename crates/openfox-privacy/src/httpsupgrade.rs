//! HTTPS upgrades: the policy layer that rewrites `http://` to `https://`
//! before a request leaves the process. The net stack's HSTS cache
//! answers first; this is what to do when HSTS has no opinion.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long a failed upgrade vetoes further attempts. Try, fail,
/// remember, back off — this is what prevents upgrade loops against
/// hosts whose TLS is broken.
const FAILURE_TTL: Duration = Duration::from_secs(10 * 60);

/// Hosts that upgrade no matter what the failure memory says. A bad TLS
/// day at github.com is not a reason to speak plaintext to github.com.
const PRELOAD: &[&str] = &[
    "github.com",
    "wikipedia.org",
    "mozilla.org",
    "cloudflare.com",
    "arxiv.org",
    "openssl.org",
    "torproject.org",
];

/// Upgrade policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpsMode {
    /// Never rewrite; plain http goes out as it came in.
    Off,
    /// Try https first, fall back to http when it fails.
    #[default]
    First,
    /// https only. No fallback — if https is down, the site is down.
    Only,
}

/// Answers "should this URL leave as https?" — `openfox-net`'s HSTS
/// cache is consulted first; this is the policy layer above it.
#[derive(Debug, Clone)]
pub struct HttpsUpgrader {
    mode: HttpsMode,
    /// Host → when its last https attempt failed. A fresh failure vetoes
    /// upgrades for ten minutes.
    https_failures: HashMap<String, Instant>,
}

impl HttpsUpgrader {
    /// An upgrader running in `mode`.
    pub fn new(mode: HttpsMode) -> Self {
        HttpsUpgrader { mode, https_failures: HashMap::new() }
    }

    /// Whether `scheme://host` should go out as https. `Off` never
    /// upgrades, and an already-https URL never needs to. `First` and
    /// `Only` answer the same here — the difference is what the loader
    /// does after a failure, which is its business, not ours. Preloaded
    /// hosts upgrade regardless of failure memory.
    pub fn should_upgrade(&self, scheme: &str, host: &str) -> bool {
        if self.mode == HttpsMode::Off || scheme != "http" {
            return false;
        }
        if is_preloaded(host) {
            return true;
        }
        match self.https_failures.get(host) {
            Some(failed_at) => failed_at.elapsed() >= FAILURE_TTL,
            None => true,
        }
    }

    /// Record that https failed on `host`; it gets ten minutes of
    /// plaintext peace before we try again.
    pub fn record_failure(&mut self, host: &str) {
        let now = Instant::now();
        self.https_failures.retain(|_, t| t.elapsed() < FAILURE_TTL);
        self.https_failures.insert(host.to_owned(), now);
    }
}

/// Whether `host` is a preloaded domain or one of its subdomains.
fn is_preloaded(host: &str) -> bool {
    PRELOAD
        .iter()
        .any(|p| host == *p || host.ends_with(&format!(".{p}")))
}
