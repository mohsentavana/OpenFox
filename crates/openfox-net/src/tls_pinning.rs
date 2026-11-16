//! TLS public-key pinning: a small, deliberate exception list, never a
//! growth strategy. Firefox and Chrome removed general-purpose pinning
//! because pinned sites that rotate keys brick their own visitors; the
//! lesson is kept, not ignored. What survives here is the shape update
//! systems and internal deployments need — pins expressed as SPKI
//! sha256 hashes, a verify hook in the TLS handshake path, and a verdict
//! the caller turns into a connection abort or a user-visible warning.

use std::collections::HashMap;

/// A pin check outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinVerdict {
    /// The host is pinned and the presented key matches a known pin.
    Pinned,
    /// The host has no pins; ordinary WebPKI validation governs.
    Unknown,
    /// The host is pinned and the presented key matches none of them.
    /// This is a possible interception: the connection must not proceed
    /// silently, whatever the recovery policy turns out to be.
    Mismatch,
}

/// Domain → SPKI sha256 pins. Domain matching is exact-host; no
/// wildcards — a wildcard pin is a promise nobody can keep about keys
/// they have not issued yet.
pub struct PinStore {
    pins: HashMap<String, Vec<[u8; 32]>>,
}

/// Seed pins. The list ships in the binary and changes only with a
/// browser release — that is the point: pins rotate with the software,
/// not with the network. Empty by design; entries arrive when OpenFox
/// runs its own update service over a host it controls end to end.
pub const PINNED_DOMAINS: &[(&str, [[u8; 32]; 1])] = &[];

impl PinStore {
    pub fn bundled() -> PinStore {
        let mut pins = HashMap::new();
        for (host, list) in PINNED_DOMAINS {
            pins.insert((*host).to_owned(), list.to_vec());
        }
        PinStore { pins }
    }

    pub fn add(&mut self, host: &str, spki_sha256: [u8; 32]) {
        self.pins.entry(host.to_owned()).or_default().push(spki_sha256);
    }

    /// Checks a presented SPKI hash. `spki_sha256` is sha256 over the
    /// DER-encoded SubjectPublicKeyInfo — the same bytes an HPKP pin
    /// was, so existing operational tooling keeps working.
    pub fn check(&self, host: &str, spki_sha256: &[u8; 32]) -> PinVerdict {
        match self.pins.get(host) {
            None => PinVerdict::Unknown,
            Some(list) if list.contains(spki_sha256) => PinVerdict::Pinned,
            Some(_) => PinVerdict::Mismatch,
        }
    }

    pub fn is_pinned(&self, host: &str) -> bool {
        self.pins.contains_key(host)
    }
}
