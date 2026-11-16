//! TLS policy: the rustls config, version floors, ALPN order, and the
//! pinning hooks. Certificate transparency lands in phase 2 as a hook,
//! not a promise.

use std::sync::Arc;

/// The TLS floor. TLS 1.0/1.1 are dead; 1.2 is the floor, 1.3 is the
/// default offer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TlsFloor {
    Tls12,
    Tls13,
}

/// ALPN preference, in order: HTTP/2 first when the server speaks it,
/// HTTP/1.1 always.
pub const ALPN_ORDER: [&[u8]; 2] = [b"h2", b"http/1.1"];

pub struct TlsPolicy {
    floor: TlsFloor,
    roots: Arc<rustls::RootCertStore>,
}

impl TlsPolicy {
    pub fn new(floor: TlsFloor) -> Self {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        TlsPolicy { floor, roots: Arc::new(roots) }
    }

    /// Build the client config the connection pool dials with. One
    /// config, shared by every origin — per-origin configs would
    /// multiply handshakes for zero benefit. The floor is the offer: at
    /// TLS 1.2 both versions are offered and rustls negotiates the
    /// highest mutual one; a 1.3-only policy drops 1.2 from the offer.
    pub fn client_config(&self) -> Arc<rustls::ClientConfig> {
        let builder = match self.floor {
            TlsFloor::Tls12 => rustls::ClientConfig::builder(),
            TlsFloor::Tls13 => {
                rustls::ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            }
        };
        let mut config = builder
            .with_root_certificates(self.roots.clone())
            .with_no_client_auth();
        config.alpn_protocols = ALPN_ORDER.iter().map(|p| p.to_vec()).collect();
        Arc::new(config)
    }

    /// The CT hook: phase 2 wires an embedded log list and requires
    /// SCTs for high-value origins. The trait is the seam.
    pub fn with_ct_logs(&self, _logs: &dyn CtLog) -> Self {
        self.clone_shallow()
    }

    fn clone_shallow(&self) -> Self {
        TlsPolicy {
            floor: self.floor,
            roots: self.roots.clone(),
        }
    }
}

impl Default for TlsPolicy {
    fn default() -> Self {
        TlsPolicy::new(TlsFloor::Tls12)
    }
}

/// Certificate transparency log list seam. An implementer verifies SCTs
/// against real logs; nothing here pretends they exist yet.
pub trait CtLog {
    fn log_id(&self) -> [u8; 32];
    fn verify_sct(&self, sct: &[u8], chain: &[Vec<u8>]) -> bool;
}

/// Session resumption: rustls keeps a ticket store per config by
/// default — this exists to say so out loud instead of leaving it as a
/// mystery knob. rustls 0.23 seals `Resumption` (private store, no
/// comparison), so there is nothing to inspect; every config built
/// here leaves the default in-memory ticket store in place.
pub fn session_resumption_enabled(_config: &rustls::ClientConfig) -> bool {
    true
}
