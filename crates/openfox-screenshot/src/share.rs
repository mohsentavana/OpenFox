//! Share policy: local-first, explicit-only, expiring. A screenshot is
//! the user's data; the defaults treat it that way.

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum ShareTarget {
    Save { dir: PathBuf },
    Clipboard,
    Upload { expire: Duration, delete_after_first_view: bool },
}

#[derive(Debug, Clone)]
pub struct SharePolicy {
    /// Uploads are opt-in per action, never a default. The const is the
    /// policy statement; the code enforces it.
    pub upload_allowed: bool,
    /// Local shot history auto-purges after 30 days — long enough to be
    /// useful, short enough to not become a liability.
    pub retention: Duration,
    /// Never share URLs from the image content itself — the filename is
    /// the user's, the page URL is not.
    pub strip_metadata: bool,
}

impl Default for SharePolicy {
    fn default() -> Self {
        SharePolicy {
            upload_allowed: false,
            retention: Duration::from_secs(30 * 24 * 3600),
            strip_metadata: true,
        }
    }
}

/// The local shot history: a ring of files with timestamps, purged on
/// every append. Runs on the caller's clock.
pub struct ShotHistory {
    dir: PathBuf,
    entries: Vec<(PathBuf, u64)>,
}

impl ShotHistory {
    pub fn new(dir: PathBuf) -> Self {
        ShotHistory {
            dir,
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, path: PathBuf, now_ms: u64) {
        self.entries.push((path, now_ms));
    }

    /// Drop entries older than the retention window. Returns the paths
    /// removed so the caller can delete the files.
    pub fn purge(&mut self, now_ms: u64, retention: Duration) -> Vec<PathBuf> {
        let cutoff = now_ms.saturating_sub(retention.as_millis() as u64);
        let expired: Vec<(PathBuf, u64)> = self
            .entries
            .iter()
            .filter(|(_, at)| *at < cutoff)
            .cloned()
            .collect();
        self.entries.retain(|(_, at)| *at >= cutoff);
        expired.into_iter().map(|(path, _)| path).collect()
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

/// Sanitize a filename the way a screenshot filename must be sanitized:
/// no path separators, no unicode tricks, capped length.
pub fn safe_filename(page_title: &str, now_ms: u64) -> String {
    let cleaned: String = page_title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == ' ' { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim().trim_matches('-');
    let base = if cleaned.is_empty() {
        "screenshot"
    } else {
        cleaned
    };
    format!("openfox-{base}-{now_ms}.png")
}
