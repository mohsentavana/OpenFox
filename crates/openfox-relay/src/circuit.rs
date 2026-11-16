//! Circuit specs: the ordered hops a private window's traffic walks and
//! the policy for how long it walks them. Pure data — establishment
//! lives in `pool`, byte-level tunneling in `socks5` and `http_connect`
//! — because a spec is configuration, and configuration should be
//! buildable, inspectable and serializable without a network stack in
//! the room.
//!
//! The shape follows the onion-service intuition with honest
//! substitutions: hop 1 is the guard (dialed directly), every later hop
//! is reached through the tunnel built so far, and the final target is
//! reached through all of them. No relay list ships in the binary —
//! the user's own proxies fill the slots, which is the whole privacy
//! argument: OpenFox routes through infrastructure its user chose, not
//! infrastructure it shipped.

use crate::RelayError;

/// How one hop is tunneled to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HopScheme {
    /// RFC 1928 SOCKS5 CONNECT — the protocol Tor's own SOCKS layer
    /// speaks, and the one most self-hosted relays offer.
    Socks5,
    /// Plain HTTP CONNECT — what corporate-style proxies speak, kept
    /// because it is also what cheap VPS forwarders speak.
    HttpConnect,
}

impl HopScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            HopScheme::Socks5 => "socks5",
            HopScheme::HttpConnect => "http-connect",
        }
    }
}

/// Proxy credentials. `Clone` without `Debug`: a debug print of proxy
/// credentials lands in crash logs, and crash logs travel further than
/// most credentials deserve.
#[derive(Clone)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

/// One hop: protocol, endpoint, optional credentials.
#[derive(Clone)]
pub struct Hop {
    pub scheme: HopScheme,
    pub host: String,
    pub port: u16,
    pub auth: Option<ProxyAuth>,
}

impl std::fmt::Debug for Hop {
    /// Debug prints the endpoint, never the credentials — a hop in a
    /// crash log should identify the machine, not hand over its login.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let auth = if self.auth.is_some() { " (auth)" } else { "" };
        write!(
            f,
            "Hop({}{}:{}{auth})",
            self.scheme.as_str(),
            self.host,
            self.port
        )
    }
}

impl Hop {
    /// Endpoint identity without credentials, safe to log and correct
    /// to pool on.
    pub fn endpoint(&self) -> String {
        format!("{}://{}:{}", self.scheme.as_str(), self.host, self.port)
    }
}

/// When to abandon a circuit and build the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationPolicy {
    /// One circuit per private session: everything the session fetches
    /// walks the same chain, and the chain dies with the session. The
    /// default, because it matches how long a session is linkable
    /// anyway (it shares cookies, cache, timing).
    PerSession,
    /// Rotate after N requests through the circuit. Trades latency for
    /// unlinkability between requests — the right knob for fetch-heavy
    /// sessions on hostile networks.
    EveryNRequests(u32),
}

impl RotationPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            RotationPolicy::PerSession => "per-session",
            RotationPolicy::EveryNRequests(_) => "every-n-requests",
        }
    }
}

/// A complete, buildable circuit description.
#[derive(Clone)]
pub struct CircuitSpec {
    hops: Vec<Hop>,
    rotation: RotationPolicy,
}

impl CircuitSpec {
    pub fn hops(&self) -> &[Hop] {
        &self.hops
    }

    pub fn rotation(&self) -> RotationPolicy {
        self.rotation
    }

    /// Stable identity for pooling: the hop chain, credentials excluded.
    /// Two specs with the same key route through the same machines and
    /// may share a circuit — never two specs with different keys.
    pub fn key(&self) -> String {
        self.hops
            .iter()
            .map(Hop::endpoint)
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// Builds a [`CircuitSpec`]. The canonical private-window circuit is
/// [`CircuitBuilder::three_hop_default`] — guard, middle, exit — but
/// one- and two-hop chains are legal and sometimes the honest choice
/// (a single trusted proxy beats a three-hop chain of unknowns).
#[derive(Debug, Clone)]
pub struct CircuitBuilder {
    hops: Vec<Hop>,
    rotation: RotationPolicy,
}

impl CircuitBuilder {
    /// An empty builder, per-session rotation, no hops yet.
    pub fn new() -> Self {
        CircuitBuilder { hops: Vec::new(), rotation: RotationPolicy::PerSession }
    }

    /// The canonical three-hop shape: a SOCKS5 guard, a SOCKS5 middle,
    /// and an HTTP CONNECT exit. Hosts are left empty on purpose —
    /// this builder lays out the *shape*; the user's relay list fills
    /// it in via [`CircuitBuilder::guard`], [`CircuitBuilder::middle`]
    /// and [`CircuitBuilder::exit`].
    pub fn three_hop_default() -> Self {
        CircuitBuilder {
            hops: vec![
                Hop { scheme: HopScheme::Socks5, host: String::new(), port: 0, auth: None },
                Hop { scheme: HopScheme::Socks5, host: String::new(), port: 0, auth: None },
                Hop { scheme: HopScheme::HttpConnect, host: String::new(), port: 0, auth: None },
            ],
            rotation: RotationPolicy::PerSession,
        }
    }

    /// Fill the guard slot (hop 1) of a three-hop builder.
    pub fn guard(mut self, host: impl Into<String>, port: u16) -> Self {
        self.fill(0, host.into(), port);
        self
    }

    /// Fill the middle slot (hop 2) of a three-hop builder.
    pub fn middle(mut self, host: impl Into<String>, port: u16) -> Self {
        self.fill(1, host.into(), port);
        self
    }

    /// Fill the exit slot (hop 3) of a three-hop builder.
    pub fn exit(mut self, host: impl Into<String>, port: u16) -> Self {
        self.fill(2, host.into(), port);
        self
    }

    fn fill(&mut self, index: usize, host: String, port: u16) {
        if let Some(hop) = self.hops.get_mut(index) {
            hop.host = host;
            hop.port = port;
        }
    }

    /// Append a fully specified hop to any builder.
    pub fn push_hop(
        mut self,
        scheme: HopScheme,
        host: impl Into<String>,
        port: u16,
        auth: Option<ProxyAuth>,
    ) -> Self {
        self.hops.push(Hop { scheme, host: host.into(), port, auth });
        self
    }

    /// Attach credentials to a hop that already exists (indexes from
    /// zero; the guard is 0).
    pub fn with_auth(mut self, hop_index: usize, auth: ProxyAuth) -> Self {
        if let Some(hop) = self.hops.get_mut(hop_index) {
            hop.auth = Some(auth);
        }
        self
    }

    pub fn rotate_per_session(mut self) -> Self {
        self.rotation = RotationPolicy::PerSession;
        self
    }

    /// Rotate after every N requests through the circuit.
    pub fn rotate_every(mut self, requests: u32) -> Self {
        self.rotation = RotationPolicy::EveryNRequests(requests.max(1));
        self
    }

    /// The hop schemes laid out so far, for descriptive surfaces that
    /// want to show the shape without building it.
    pub fn hop_schemes(&self) -> Vec<&'static str> {
        self.hops.iter().map(|hop| hop.scheme.as_str()).collect()
    }

    pub fn rotation(&self) -> RotationPolicy {
        self.rotation
    }

    /// Validate and finish. Every hop needs a host and a port; a
    /// circuit with no hops is a direct connection wearing a costume.
    pub fn build(self) -> Result<CircuitSpec, RelayError> {
        if self.hops.is_empty() {
            return Err(RelayError::IncompleteCircuit);
        }
        if self.hops.iter().any(|hop| hop.host.is_empty() || hop.port == 0) {
            return Err(RelayError::IncompleteCircuit);
        }
        Ok(CircuitSpec { hops: self.hops, rotation: self.rotation })
    }
}

impl Default for CircuitBuilder {
    fn default() -> Self {
        CircuitBuilder::new()
    }
}
