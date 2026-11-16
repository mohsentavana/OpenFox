//! The shields: tracker classification, ETP levels, fingerprint farbling,
//! consent auto-reject, bounce tracking, HTTPS upgrades. Every decision is
//! made before a request leaves the process.
//!
//! The pipeline in one breath: `classify` names what a request is, `etp`
//! decides whether it flies, `fingerprint` fakes whatever trackers try to
//! measure, `consent` answers their dialogs before the user has to,
//! `bounce` cuts their redirect chains and scrubs the landing URL, and
//! `httpsupgrade` refuses to speak plaintext.
//!
//! The model is first-party by construction, same as the net stack's
//! cookie jar: a site gets to talk to itself, and everything cross-site
//! has to earn its way past the shields. Nothing here phones home — the
//! lists are compiled in and every decision is local.
//!
//! Two invariants hold throughout: decisions are local — no list
//! fetches, no telemetry about what got blocked — and they happen
//! pre-flight. By the time a socket opens, the request has already
//! been classified, allowed or refused, upgraded or scrubbed.

use psl::Psl;

pub mod bounce;
pub mod classify;
pub mod consent;
pub mod etp;
pub mod fingerprint;
pub mod httpsupgrade;

pub use bounce::BounceMitigator;
pub use classify::{classify, ClassifyRequest, Decision, ResourceType, TrackerCategory};
pub use consent::{ConsentGate, ConsentPolicy, ConsentVerdict};
pub use etp::{EtpLevel, EtpPolicy};
pub use fingerprint::Farbler;
pub use httpsupgrade::{HttpsMode, HttpsUpgrader};

/// Everything the shields can fail at. They fail closed: a request that
/// cannot be decided is blocked, not waved through.
#[derive(Debug, thiserror::Error)]
pub enum PrivacyError {
    /// A URL the engine handed over did not parse. The shields never
    /// guess at a malformed URL — they hand it back untouched.
    #[error("unparseable URL: {0}")]
    UnparseableUrl(String),

    /// The public suffix list could not be activated. Without it there
    /// is no such thing as "third-party", so every request is treated
    /// as cross-site until it comes back.
    #[error("public suffix list unavailable")]
    NoSuffixList,
}

/// Whether two hosts sit on different registrable domains — the
/// crate-wide definition of "third-party". Everything that partitions
/// by party (classification, URL scrubbing, and anything downstream)
/// asks here, so the answer never varies between modules.
pub fn is_third_party(top_host: &str, request_host: &str) -> bool {
    registrable(top_host) != registrable(request_host)
}

/// The registrable domain of `host` under the bundled public suffix
/// list, or the host itself when no rule matches.
fn registrable(host: &str) -> String {
    // The bundled list is a zero-sized type in psl 2; `domain` consults it
    // directly, so there is nothing to construct or cache.
    psl::domain(host.as_bytes())
        .map(|d| String::from_utf8_lossy(d.as_bytes()).into_owned())
        .unwrap_or_else(|| host.to_owned())
}
