//! The relay: tor-style private-window routing without shipping Tor.
//! A private window's traffic walks a chain of SOCKS5 and HTTP CONNECT
//! proxies the user chose — guard, middle, exit — with per-tab-stream
//! isolation, a bounded LRU circuit pool, and rotation policy as data.
//!
//! The division of labor: `circuit` is the pure spec a private window
//! is configured with; `socks5` and `http_connect` are hand-rolled
//! tunnel protocols over tokio TCP; `pool` builds chains inside-out and
//! keeps the live ones bounded; `isolation` is the key that guarantees
//! two tabs never share a machine that could correlate them. Nothing
//! here dials anything until a caller asks for a target through a spec,
//! and no relay list ships in the binary — the user's infrastructure,
//! or none.

pub mod circuit;
pub mod http_connect;
pub mod isolation;
pub mod pool;
pub mod socks5;

pub use circuit::{CircuitBuilder, CircuitSpec, Hop, HopScheme, ProxyAuth, RotationPolicy};
pub use isolation::IsolationKey;
pub use pool::{Circuit, CircuitPool};

use std::time::Duration;

/// Everything routing through someone else's machine can fail at.
/// Network and tunnel failures carry the hop that failed; policy
/// failures (dead circuits, incomplete specs) are the pool keeping its
/// own counsel.
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("connect {host}:{port}: {source}")]
    Connect { host: String, port: u16, source: std::io::Error },

    /// A SOCKS5 or HTTP proxy answered, but not in a language this
    /// client speaks.
    #[error("tunnel handshake failed: {0}")]
    Handshake(String),

    /// The proxy offered no auth method we can use (or rejected the
    /// credentials it demanded).
    #[error("no acceptable proxy authentication")]
    UnsupportedAuth,

    /// SOCKS5 CONNECT came back with a failure code.
    #[error("socks5 proxy refused the connection (code {code})")]
    SocksRefused { code: u8 },

    /// HTTP CONNECT came back with a non-2xx status.
    #[error("http proxy refused the tunnel (status {status})")]
    HttpRefused { status: u16 },

    /// The response head never parsed into anything actionable.
    #[error("malformed proxy response")]
    MalformedHead,

    /// The response head exceeded the cap a proxy response deserves.
    #[error("proxy response head exceeded {0} bytes")]
    HeadOverflow(usize),

    /// A builder produced a spec that cannot route: no hops, or a hop
    /// without an endpoint.
    #[error("circuit spec is incomplete: every hop needs a scheme, host and port")]
    IncompleteCircuit,

    /// This (isolation, circuit, target) failed recently and is in its
    /// cooldown; retry when `retry_in` has elapsed.
    #[error("circuit marked dead; retry in {retry_in:?}")]
    DeadCircuit { retry_in: Duration },

    /// Chain establishment did not finish inside its budget.
    #[error("circuit establishment timed out after {after:?}")]
    Timeout { after: Duration },
}
