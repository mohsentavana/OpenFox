//! The circuit pool: bounded live circuits, LRU eviction, dead-marking
//! on failure — the net stack's connection pool model applied to
//! chains. Circuits are pooled per (isolation key, spec, target): a
//! reuse is only a reuse when the same identity, through the same
//! machines, to the same destination is asking.
//!
//! Establishment is the pool's job because a chain is built inside-out:
//! dial the guard directly, tunnel every later hop through the stream
//! built so far, then CONNECT the final target through all of them.
//! After the final CONNECT the tunnel is bound to that target — that is
//! the SOCKS reality, and the pool is shaped by it rather than
//! pretending streams multiplex. When a caller is done it either
//! releases the circuit (back to the pool, rotation policy permitting)
//! or drops it (rotation, session end, or the peer said goodbye).

use crate::circuit::{CircuitSpec, HopScheme, RotationPolicy};
use crate::http_connect;
use crate::isolation::IsolationKey;
use crate::socks5;
use crate::RelayError;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

/// Whole-chain establishment budget: guard dial plus every hop
/// handshake. Ten seconds is generous for three hops and tight enough
/// that a dead relay fails a page load, not a session.
const ESTABLISH_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a failed (isolation, spec, target) stays dead before the
/// pool will try building it again. Marked-dead exists so one broken
/// relay cannot turn a page's fetch fan-out into a retry storm.
const DEAD_TTL: Duration = Duration::from_secs(10);

/// One established, target-bound tunnel.
pub struct Circuit {
    stream: TcpStream,
    /// The pool bucket this circuit belongs to: isolation + spec + target.
    pool_key: String,
    target_host: String,
    target_port: u16,
    rotation: RotationPolicy,
    /// Requests that have traveled this circuit, acquire counted.
    requests: u32,
    last_used: Instant,
}

impl Circuit {
    /// The tunnel itself. After the chain's final CONNECT, every byte
    /// written here goes to the target and every byte read comes from
    /// it — TLS, HTTP, anything, end to end through the hops.
    pub fn stream(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    pub fn target_host(&self) -> &str {
        &self.target_host
    }

    pub fn target_port(&self) -> u16 {
        self.target_port
    }

    /// How many requests have used this circuit so far.
    pub fn requests_served(&self) -> u32 {
        self.requests
    }

    /// Whether the rotation policy says this circuit is finished.
    /// `PerSession` never rotates here — the session's lifetime is the
    /// caller's signal, and the pool obeys it.
    pub fn should_rotate(&self) -> bool {
        match self.rotation {
            RotationPolicy::PerSession => false,
            RotationPolicy::EveryNRequests(n) => self.requests >= n,
        }
    }
}

/// Bounded pool of live circuits.
pub struct CircuitPool {
    max_live: usize,
    live: Vec<Circuit>,
    dead_until: HashMap<String, Instant>,
}

impl CircuitPool {
    /// A pool holding at most `max_live` live circuits (at least one).
    pub fn new(max_live: usize) -> Self {
        CircuitPool {
            max_live: max_live.max(1),
            live: Vec::new(),
            dead_until: HashMap::new(),
        }
    }

    /// The pool bucket identity. Everything about sharing a circuit is
    /// in this string: same identity, same machines, same destination.
    fn key(iso: &IsolationKey, spec: &CircuitSpec, host: &str, port: u16) -> String {
        format!("{iso}|{}|{host}:{port}", spec.key())
    }

    /// Acquire a circuit to `host:port` through `spec`, isolated to
    /// `iso`. Reuses a live pooled circuit when the bucket matches;
    /// otherwise builds a fresh chain. An establish failure marks the
    /// bucket dead for [`DEAD_TTL`] and returns the error — one broken
    /// relay, one cooldown, no storm.
    pub async fn connect(
        &mut self,
        iso: &IsolationKey,
        spec: &CircuitSpec,
        host: &str,
        port: u16,
    ) -> Result<Circuit, RelayError> {
        let key = Self::key(iso, spec, host, port);
        let now = Instant::now();
        match self.dead_until.get(&key) {
            Some(deadline) if *deadline > now => {
                return Err(RelayError::DeadCircuit { retry_in: *deadline - now });
            }
            Some(_) => {
                self.dead_until.remove(&key);
            }
            None => {}
        }

        if let Some(position) = self.live.iter().position(|c| c.pool_key == key) {
            let mut circuit = self.live.remove(position);
            circuit.requests += 1;
            circuit.last_used = Instant::now();
            return Ok(circuit);
        }

        let built = tokio::time::timeout(ESTABLISH_TIMEOUT, establish(spec, host, port)).await;
        match built {
            Ok(Ok(stream)) => Ok(Circuit {
                stream,
                pool_key: key,
                target_host: host.to_owned(),
                target_port: port,
                rotation: spec.rotation(),
                requests: 1,
                last_used: Instant::now(),
            }),
            Ok(Err(failure)) => {
                self.note_dead(&key);
                Err(failure)
            }
            Err(_) => {
                self.note_dead(&key);
                Err(RelayError::Timeout { after: ESTABLISH_TIMEOUT })
            }
        }
    }

    /// Return a healthy circuit to the pool. Circuits past their
    /// rotation budget are refused — rotation means what it says. A
    /// full pool evicts its least-recently-used circuit first: the LRU
    /// entry is by definition the one nobody needed.
    pub fn release(&mut self, circuit: Circuit) {
        if circuit.should_rotate() {
            return;
        }
        if self.live.len() >= self.max_live {
            self.evict_lru();
        }
        self.live.push(circuit);
    }

    /// Mark a bucket dead: evict any live circuit in it and refuse new
    /// builds for [`DEAD_TTL`]. Call this when a released circuit fails
    /// on first use — a pooled tunnel whose peer hung up looks alive
    /// until the moment it does not.
    pub fn mark_dead(&mut self, iso: &IsolationKey, spec: &CircuitSpec, host: &str, port: u16) {
        self.note_dead(&Self::key(iso, spec, host, port));
    }

    fn note_dead(&mut self, key: &str) {
        self.dead_until.insert(key.to_owned(), Instant::now() + DEAD_TTL);
        self.live.retain(|c| c.pool_key != key);
    }

    fn evict_lru(&mut self) {
        if let Some(oldest) = self
            .live
            .iter()
            .enumerate()
            .min_by_key(|(_, circuit)| circuit.last_used)
            .map(|(index, _)| index)
        {
            self.live.remove(oldest);
        }
    }

    pub fn live_count(&self) -> usize {
        self.live.len()
    }

    pub fn dead_count(&self) -> usize {
        self.dead_until.len()
    }
}

/// Build a chain from the inside out: dial hop 0 directly, tunnel each
/// later hop through everything before it, then CONNECT the target
/// through the whole chain. The exit hop's protocol decides how the
/// final target is asked for.
async fn establish(spec: &CircuitSpec, host: &str, port: u16) -> Result<TcpStream, RelayError> {
    let hops = spec.hops();
    let guard = &hops[0];
    // The guard is the one host this process resolves itself — it is
    // the entry point, and entry points are chosen, not discovered.
    // Everything after it is handed to the chain as a name.
    let mut addrs = tokio::net::lookup_host((guard.host.as_str(), guard.port))
        .await
        .map_err(|e| RelayError::Connect {
            host: guard.host.clone(),
            port: guard.port,
            source: e,
        })?;
    let Some(addr) = addrs.next() else {
        return Err(RelayError::Connect {
            host: guard.host.clone(),
            port: guard.port,
            source: std::io::Error::new(std::io::ErrorKind::Other, "no addresses"),
        });
    };
    let mut stream = TcpStream::connect(addr).await.map_err(|e| RelayError::Connect {
        host: guard.host.clone(),
        port: guard.port,
        source: e,
    })?;

    for hop in &hops[1..] {
        tunnel(&mut stream, hop.scheme, &hop.host, hop.port, hop.auth.as_ref()).await?;
    }

    let exit = &hops[hops.len() - 1];
    tunnel(&mut stream, exit.scheme, host, port, exit.auth.as_ref()).await?;
    Ok(stream)
}

/// Speak one hop of the chain in the hop's protocol.
async fn tunnel<S>(
    stream: &mut S,
    scheme: HopScheme,
    host: &str,
    port: u16,
    auth: Option<&crate::circuit::ProxyAuth>,
) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match scheme {
        HopScheme::Socks5 => socks5::connect(stream, host, port, auth).await,
        HopScheme::HttpConnect => http_connect::connect(stream, host, port, auth).await,
    }
}
