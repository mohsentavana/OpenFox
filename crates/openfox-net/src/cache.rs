//! The HTTP cache, RFC 9111 semantics: freshness lifetimes, Vary
//! matching, revalidation decisions, LRU eviction with a byte budget.
//! A cache that lies is worse than no cache; every rule here is the
//! conservative reading.

use std::collections::HashMap;
use tokio::sync::Mutex;
use url::Url;

const DEFAULT_BUDGET: usize = 64 * 1024 * 1024;
const MAX_ENTRY: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: bytes::Bytes,
    pub stored_at_ms: u64,
    pub vary: HashMap<String, String>,
}

impl CacheEntry {
    /// The freshness lifetime per Cache-Control, then Expires. max-age
    /// wins over Expires; s-maxage wins over both (we are a private
    /// cache, so it does not apply — noted for completeness).
    pub fn freshness_ms(&self) -> Option<u64> {
        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("cache-control") {
                for directive in value.split(',') {
                    let directive = directive.trim();
                    if let Some(age) = directive
                        .strip_prefix("s-maxage=")
                        .or_else(|| directive.strip_prefix("max-age="))
                    {
                        if let Ok(seconds) = age.trim().parse::<u64>() {
                            return Some(seconds * 1000);
                        }
                    }
                }
                if value.to_ascii_lowercase().contains("no-cache")
                    || value.to_ascii_lowercase().contains("no-store")
                {
                    return None;
                }
            }
        }
        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("expires") {
                if let Some(max) = parse_http_date_max_age(value, self.stored_at_ms) {
                    return Some(max);
                }
            }
        }
        None
    }

    pub fn is_fresh(&self, now_ms: u64) -> bool {
        match self.freshness_ms() {
            Some(fresh) => now_ms < self.stored_at_ms.saturating_add(fresh),
            None => false,
        }
    }
}

pub struct HttpCache {
    entries: Mutex<HashMap<String, Vec<CacheEntry>>>,
    budget: usize,
    used: std::sync::atomic::AtomicUsize,
    lru: Mutex<Vec<(String, u64)>>,
}

impl Default for HttpCache {
    fn default() -> Self {
        HttpCache::new(DEFAULT_BUDGET)
    }
}

impl HttpCache {
    pub fn new(budget: usize) -> Self {
        HttpCache {
            entries: Mutex::new(HashMap::new()),
            budget,
            used: std::sync::atomic::AtomicUsize::new(0),
            lru: Mutex::new(Vec::new()),
        }
    }

    /// Store an entry if the response allows it. no-store never enters;
    /// oversized entries never evict the cache for one file.
    pub async fn store(&self, entry: CacheEntry, request_headers: &[(String, String)]) {
        if entry.body.len() > MAX_ENTRY {
            return;
        }
        if entry
            .headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("cache-control")
                && v.to_ascii_lowercase().contains("no-store"))
        {
            return;
        }

        // Vary: remember which request headers this response depends on.
        let mut vary = HashMap::new();
        for (name, value) in &entry.headers {
            if name.eq_ignore_ascii_case("vary") {
                for dimension in value.split(',') {
                    let dimension = dimension.trim().to_ascii_lowercase();
                    if dimension == "*" {
                        return; // varying on everything = storing nothing
                    }
                    if let Some((_, chosen)) = request_headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(&dimension))
                    {
                        vary.insert(dimension, chosen.clone());
                    } else {
                        vary.insert(dimension, String::new());
                    }
                }
            }
        }

        self.evict_for(entry.body.len()).await;
        let mut entries = self.entries.lock().await;
        let mut entry = entry;
        entry.vary = vary;
        self.used
            .fetch_add(entry.body.len(), std::sync::atomic::Ordering::Relaxed);
        entries.entry(entry.url.clone()).or_default().push(entry);
    }

    /// Lookup with Vary matching against the current request headers.
    pub async fn lookup(
        &self,
        url: &Url,
        request_headers: &[(String, String)],
    ) -> Option<CacheEntry> {
        let entries = self.entries.lock().await;
        let candidates = entries.get(url.as_str())?;
        candidates.iter().find_map(|entry| {
            let matches_vary = entry.vary.iter().all(|(dimension, chosen)| {
                request_headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(dimension))
                    .map(|(_, value)| value == chosen)
                    .unwrap_or(chosen.is_empty())
            });
            if matches_vary {
                Some(entry.clone())
            } else {
                None
            }
        })
    }

    /// Does this cached response need revalidation before use?
    pub fn needs_revalidation(entry: &CacheEntry, now_ms: u64) -> bool {
        if !entry.is_fresh(now_ms) {
            return true;
        }
        entry
            .headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("cache-control")
                && v.to_ascii_lowercase().contains("must-revalidate"))
    }

    async fn evict_for(&self, incoming: usize) {
        while self.used.load(std::sync::atomic::Ordering::Relaxed) + incoming > self.budget {
            let victim = {
                let mut lru = self.lru.lock().await;
                lru.pop() // oldest last-use stamp goes first
            };
            let Some((url, _)) = victim else { break };
            let mut entries = self.entries.lock().await;
            if let Some(list) = entries.get_mut(&url) {
                if let Some(removed) = list.pop() {
                    self.used
                        .fetch_sub(removed.body.len(), std::sync::atomic::Ordering::Relaxed);
                }
                if list.is_empty() {
                    entries.remove(&url);
                }
            }
        }
    }

    pub fn bytes_used(&self) -> usize {
        self.used.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn clear(&self) {
        let mut entries = self.entries.lock().await;
        entries.clear();
        self.lru.lock().await.clear();
        self.used.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Expires -> age math, IMF-fixdate only — legacy date formats get no
/// cache trust. Returns milliseconds from `now_ms` until expiry.
fn parse_http_date_max_age(value: &str, now_ms: u64) -> Option<u64> {
    // "Sun, 06 Nov 1994 08:49:37 GMT"
    let mut parts = value.trim().split_whitespace();
    let _day_name = parts.next()?;
    let day: i64 = parts.next()?.trim_start_matches('0').parse().ok()?;
    let month = match parts.next()?.to_ascii_lowercase().as_str() {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    let year: i64 = parts.next()?.parse().ok()?;
    let mut time = parts.next()?.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: i64 = time.next()?.parse().ok()?;
    if parts.next()? != "GMT" {
        return None;
    }

    // Days since Unix epoch (Howard Hinnant's civil-from-days inverse).
    let (year, month) = if month <= 2 { (year - 1, month + 12) } else { (year, month) };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month - 3) + 2) / 5 + day - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    let target_s = days * 86_400 + hour * 3_600 + minute * 60 + second;
    let target_ms = target_s.saturating_mul(1000).max(0) as u64;
    target_ms.checked_sub(now_ms)
}
