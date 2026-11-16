use serde_json::{json, Value};
use std::collections::VecDeque;

const CAP: usize = 500;

#[derive(Debug, Clone)]
pub struct RequestRecord {
    pub url: String,
    pub method: String,
    pub status: u16,
    pub elapsed_ms: u64,
    pub bytes: u64,
    pub blocked: bool,
    pub via: &'static str,
    /// Milliseconds from log start to the first byte of this request.
    pub start_ms: u64,
}

/// One bar of the waterfall: where in the page's lifetime a request started,
/// how long it ran, and whether it died at the policy engine before any
/// socket moved.
#[derive(Debug, Clone)]
pub struct WaterfallBar {
    pub url: String,
    pub start_ms: u64,
    pub total_ms: u64,
    pub blocked: bool,
}

pub struct NetworkLog {
    records: VecDeque<RequestRecord>,
    epoch: std::time::Instant,
}

impl NetworkLog {
    pub fn new() -> Self {
        NetworkLog { records: VecDeque::with_capacity(CAP), epoch: std::time::Instant::now() }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        url: impl Into<String>,
        method: &str,
        status: u16,
        elapsed_ms: u64,
        bytes: u64,
        blocked: bool,
        via: &'static str,
    ) {
        if self.records.len() == CAP {
            self.records.pop_front();
        }
        let start_ms = self.epoch.elapsed().as_millis() as u64;
        self.records.push_back(RequestRecord {
            url: url.into(),
            method: method.into(),
            status,
            elapsed_ms,
            bytes,
            blocked,
            via,
            start_ms,
        });
    }

    /// The waterfall: offsets are relative to the first retained request,
    /// blocked entries render zero-width because they never ran.
    pub fn waterfall(&self) -> Vec<WaterfallBar> {
        let base = self.records.front().map(|first| first.start_ms).unwrap_or(0);
        self.records
            .iter()
            .map(|record| WaterfallBar {
                url: record.url.clone(),
                start_ms: record.start_ms.saturating_sub(base),
                total_ms: if record.blocked { 0 } else { record.elapsed_ms },
                blocked: record.blocked,
            })
            .collect()
    }

    pub fn to_json(&self, filter: Option<&str>) -> Value {
        let records: Vec<Value> = self
            .records
            .iter()
            .filter(|record| filter.map_or(true, |needle| record.url.contains(needle)))
            .map(|record| {
                json!({
                    "url": record.url,
                    "method": record.method,
                    "status": record.status,
                    "elapsedMs": record.elapsed_ms,
                    "bytes": record.bytes,
                    "blocked": record.blocked,
                    "via": record.via,
                    "startMs": record.start_ms,
                })
            })
            .collect();
        json!({ "requests": records })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

impl Default for NetworkLog {
    fn default() -> Self {
        NetworkLog::new()
    }
}
