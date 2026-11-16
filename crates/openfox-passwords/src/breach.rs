//! Breach checking by the HIBP range protocol: the client SHA-1s the
//! password, sends only the first five hex characters, and matches the
//! remaining 35 locally against the suffixes the server returns for that
//! prefix. That is k-anonymity — the server sees a bucket shared by
//! millions of passwords and cannot narrow it further.

use crate::PasswordError;
use sha1::{Digest, Sha1};
use std::collections::HashMap;

/// The answer shape of `GET /range/{prefix}`: every leaked-password suffix
/// under the prefix, with how often each appears in the corpus.
#[derive(Debug, Clone)]
pub struct RangeResponse {
    pub prefix: String,
    pub suffixes: Vec<String>,
    pub counts: Vec<u32>,
}

/// Canned range rows standing in for api.pwnedpasswords.com so the
/// pipeline runs without network. Suffixes are 35 hex chars (SHA-1 minus
/// the 5-char prefix); counts are HIBP-scale.
const DEMO_CORPUS: [(&str, u32); 12] = [
    ("A3F7C0B92ED5148FA6C371E0D9C4B2E85A1", 3_766_313),
    ("1C9E0F47B36A2D8E0F5CB371948D6E02A9F", 1_243_589),
    ("7E2B59A0F8C4D631E7A0F59B2830CE6B1D4", 764_372),
    ("D08C153F9AE6B470A1D2C93E8F560B174CA", 622_107),
    ("2F5E8A0C73B6914E7D0238FA5C1B9E640D7", 232_518),
    ("9A3D671F0BC5E2840A9DF3617B84C0E2D95", 152_088),
    ("4B7E0D2A9386C5F109BAE4D36730C15F8A2", 94_522),
    ("C1F4806E9B3A7D5920C4B5E137D0A8F6C29", 51_770),
    ("50D9E2C74A8F1B6A09E3D65C71B3F8E4A20", 37_904),
    ("E74B198A0DC3F650D2B981E7A45C03B69D2", 11_539),
    ("3A8C4F159E70B2DC46F109E5AD37B812F60", 8_492),
    ("B5F026E913A7C48D0B6F31E8594A0C7C2D1", 2_107),
];

impl RangeResponse {
    /// The demo response for a prefix. The corpus is prefix-blind — every
    /// prefix gets the same rows — fine for exercising the matching logic,
    /// useless for real risk; the network stack replaces this whole
    /// constructor with one HTTPS call to `api.pwnedpasswords.com`.
    pub fn demo(prefix: &str) -> RangeResponse {
        RangeResponse {
            prefix: prefix.to_owned(),
            suffixes: DEMO_CORPUS.iter().map(|(s, _)| (*s).to_owned()).collect(),
            counts: DEMO_CORPUS.iter().map(|(_, c)| *c).collect(),
        }
    }
}

/// The verdict for one password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreachStatus {
    /// No suffix match under this prefix.
    Safe,
    /// The exact SHA-1 appears in the corpus this many times.
    Breached { count: u32 },
}

/// Remembers verdicts by SHA-1 so a password is never re-queried. The keys
/// are hashes — the same transform HIBP itself uses — plaintext never
/// lands in this map.
#[derive(Default)]
pub struct BreachMonitor {
    seen: HashMap<String, BreachStatus>,
}

impl BreachMonitor {
    pub fn new() -> Self {
        BreachMonitor { seen: HashMap::new() }
    }

    /// The network leg. Real builds issue `GET
    /// https://api.pwnedpasswords.com/range/{prefix}` over HTTPS and hand
    /// back a [`RangeResponse`]; everything that can go wrong there —
    /// transport, non-200, junk rows — folds into `BreachLookup`. A
    /// malformed prefix is refused here rather than sent to the wire.
    pub fn fetch_range(&mut self, prefix: &str) -> Result<RangeResponse, PasswordError> {
        if prefix.len() != 5 || !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PasswordError::BreachLookup(format!("bad range prefix {prefix:?}")));
        }
        Ok(RangeResponse::demo(prefix))
    }

    /// Match a full SHA-1 against a range response, beyond the shared
    /// prefix. A repeat check answers from the cache and asks nothing.
    pub fn check(&mut self, sha1_hex: &str, response: &RangeResponse) -> BreachStatus {
        if let Some(status) = self.seen.get(sha1_hex) {
            return *status;
        }
        let prefix = range_prefix(sha1_hex);
        let suffix = &sha1_hex[prefix.len()..];
        let status = response
            .suffixes
            .iter()
            .zip(response.counts.iter())
            .find(|(s, _)| s.eq_ignore_ascii_case(suffix))
            .map(|(_, &count)| BreachStatus::Breached { count })
            .unwrap_or(BreachStatus::Safe);
        self.seen.insert(sha1_hex.to_owned(), status);
        status
    }
}

/// The first five hex characters of the SHA-1 — the only fragment that
/// ever crosses the network. Five hex chars carve the hash space into a
/// million buckets, each holding millions of leaked passwords; the server
/// learns nothing that narrows which one is yours.
pub fn range_prefix(sha1_hex: &str) -> &str {
    &sha1_hex[..sha1_hex.len().min(5)]
}

/// SHA-1 hex of a secret. Yes, SHA-1: HIBP indexes by SHA-1, so we ask in
/// the server's dialect, and its collision weakness is irrelevant to a
/// range query — the server never sees our input at all.
pub fn sha1_hex(secret: &[u8]) -> String {
    hex(&Sha1::digest(secret))
}

/// Hex-encode by hand: a dozen lines beat a dependency.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}
