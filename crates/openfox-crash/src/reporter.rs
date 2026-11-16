//! The reporter: local queue, dedup by crash hash, opt-in upload. The
//! default is OFF — a crash report is the user's data, and the user
//! decides where it goes.

use crate::CrashError;
use std::path::PathBuf;

/// The upload policy, as one honest constant. Opt-in, rate-limited,
/// deduplicated. The UI can only turn it on with an explicit user
/// action; there is no experiment that flips it.
pub const DEFAULT_UPLOAD_POLICY: UploadPolicy = UploadPolicy {
    upload_enabled: false,
    max_per_day: 10,
};

#[derive(Debug, Clone, Copy)]
pub struct UploadPolicy {
    pub upload_enabled: bool,
    pub max_per_day: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrashReport {
    /// sha256 of the first 4 KiB of the minidump — enough to dedup the
    /// same crash across boots, not enough to identify anything else.
    pub crash_hash: String,
    pub path: PathBuf,
    pub seen_at_ms: u64,
    pub uploaded: bool,
}

/// The async transport the reporter uploads through — implemented by
/// `openfox-net` at the app layer, kept abstract here.
pub trait UploadTransport {
    fn send(&self, report: &CrashReport, bytes: &[u8]) -> Result<(), CrashError>;
}

pub struct CrashReporter {
    queue: Vec<CrashReport>,
    policy: UploadPolicy,
    uploaded_today: u32,
    day_ms: u64,
}

impl CrashReporter {
    pub fn new(policy: UploadPolicy) -> Self {
        CrashReporter {
            queue: Vec::new(),
            policy,
            uploaded_today: 0,
            day_ms: 0,
        }
    }

    pub fn queue(&self) -> &[CrashReport] {
        &self.queue
    }

    /// Register a new dump. Same crash hash within the queue bumps a
    /// counter instead of growing the list — one bug, one line.
    pub fn record(&mut self, path: PathBuf, dump: &[u8], now_ms: u64) -> String {
        let crash_hash = crash_hash(dump);
        if let Some(existing) = self
            .queue
            .iter_mut()
            .find(|report| report.crash_hash == crash_hash)
        {
            existing.seen_at_ms = now_ms;
            return crash_hash;
        }
        self.queue.push(CrashReport {
            crash_hash: crash_hash.clone(),
            path,
            seen_at_ms: now_ms,
            uploaded: false,
        });
        crash_hash
    }

    /// Flush pending uploads through a transport, honoring policy. Returns
    /// how many reports left the device (zero unless the user opted in).
    pub fn flush(
        &mut self,
        transport: &dyn UploadTransport,
        now_ms: u64,
    ) -> Result<u32, CrashError> {
        if !self.policy.upload_enabled {
            return Ok(0);
        }
        // A day boundary resets the rate limit.
        if now_ms.saturating_sub(self.day_ms) > 86_400_000 {
            self.uploaded_today = 0;
            self.day_ms = now_ms;
        }
        let mut sent = 0u32;
        for report in &self.queue {
            if report.uploaded || self.uploaded_today >= self.policy.max_per_day {
                continue;
            }
            let Ok(bytes) = std::fs::read(&report.path) else {
                continue;
            };
            if transport.send(report, &bytes).is_ok() {
                sent += 1;
                self.uploaded_today += 1;
            }
        }
        if sent > 0 {
            for report in &mut self.queue {
                if report.seen_at_ms <= now_ms && !report.path.as_os_str().is_empty() {
                    report.uploaded = true;
                }
            }
        }
        Ok(sent)
    }

    /// Purge reports older than 14 days — a stale crash report helps
    /// nobody and narrates badly.
    pub fn purge(&mut self, now_ms: u64) -> Vec<PathBuf> {
        let cutoff = now_ms.saturating_sub(14 * 86_400_000);
        let expired: Vec<CrashReport> = self
            .queue
            .iter()
            .filter(|report| report.seen_at_ms < cutoff)
            .cloned()
            .collect();
        self.queue.retain(|report| report.seen_at_ms >= cutoff);
        expired.into_iter().map(|report| report.path).collect()
    }
}

fn crash_hash(dump: &[u8]) -> String {
    // FNV-1a over the dump prefix: not cryptographic, and it does not
    // need to be — dedup wants spread, not secrecy.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in &dump[..dump.len().min(4096)] {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}
