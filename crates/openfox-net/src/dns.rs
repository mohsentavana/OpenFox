//! DNS resolution with an in-process TTL cache over hickory's tokio
//! resolver. Positive and negative answers are both cached — a dead host
//! should not be re-asked for every subresource of a failing page — and the
//! system resolver (getaddrinfo on a blocking thread) is the fallback, never
//! an error path alone.

use hickory_resolver::TokioAsyncResolver;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const POSITIVE_TTL: Duration = Duration::from_secs(300);
const NEGATIVE_TTL: Duration = Duration::from_secs(30);

struct Entry {
    ips: Vec<IpAddr>,
    expires: Instant,
}

/// Resolves hosts through hickory with an in-process TTL cache. Negative
/// answers are cached too — a dead host should not be re-asked for every
/// subresource of a failing page. If no resolver answers (air-gapped box),
/// the system resolver is used as the fallback, never an error path alone.
pub struct DnsCache {
    resolver: TokioAsyncResolver,
    entries: Mutex<HashMap<String, Entry>>,
}

impl DnsCache {
    pub fn new() -> Self {
        // hickory 0.24: the resolver is built through the tokio constructor,
        // config and options in one call. Construction is lazy — nothing is
        // dialed until the first lookup.
        let resolver = TokioAsyncResolver::tokio(
            hickory_resolver::config::ResolverConfig::default(),
            hickory_resolver::config::ResolverOpts::default(),
        );
        DnsCache { resolver, entries: Mutex::new(HashMap::new()) }
    }

    pub async fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, String> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        if let Some(ips) = self.cached(host) {
            return Ok(ips);
        }
        let queried = host.to_owned();
        let lookup = self.resolver.lookup_ip(queried.clone()).await;
        let ips = match lookup {
            Ok(answers) => {
                let found: Vec<IpAddr> = answers.iter().collect();
                if found.is_empty() {
                    Vec::new()
                } else {
                    found
                }
            }
            Err(_) => self
                .resolve_via_system(&queried)
                .await
                .map_err(|e| format!("system resolver: {e}"))?,
        };
        let ttl = if ips.is_empty() { NEGATIVE_TTL } else { POSITIVE_TTL };
        let mut entries = self.entries.lock().expect("dns cache lock");
        entries.insert(host.to_owned(), Entry { ips: ips.clone(), expires: Instant::now() + ttl });
        Ok(ips)
    }

    fn cached(&self, host: &str) -> Option<Vec<IpAddr>> {
        let mut entries = self.entries.lock().expect("dns cache lock");
        match entries.get(host) {
            Some(entry) => {
                if entry.expires > Instant::now() {
                    Some(entry.ips.clone())
                } else {
                    entries.remove(host);
                    None
                }
            }
            None => None,
        }
    }

    async fn resolve_via_system(&self, host: &str) -> std::io::Result<Vec<IpAddr>> {
        // Owned copy: the blocking task must be 'static, and getaddrinfo
        // gets its own hostname to outlive the borrow.
        let host = host.to_owned();
        tokio::task::spawn_blocking(move || {
            use std::net::ToSocketAddrs;
            // Port is irrelevant to getaddrinfo; 0 keeps it honest.
            Ok((host.as_str(), 0u16)
                .to_socket_addrs()?
                .map(|addr| addr.ip())
                .collect::<Vec<IpAddr>>())
        })
        .await
        .map_err(std::io::Error::other)?
    }
}
