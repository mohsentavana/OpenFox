//! The queue: priority, concurrency cap, bandwidth ceiling. Four
//! concurrent downloads is the number Firefox arrived at after years of
//! router complaints — keep it.

use std::collections::VecDeque;

pub struct DownloadQueue {
    /// User-initiated downloads jump the line; prefetch-class downloads
    /// (update checks, feed fetches) wait their turn.
    user: VecDeque<u64>,
    background: VecDeque<u64>,
    max_concurrent: usize,
    active: usize,
}

impl DownloadQueue {
    pub fn new(max_concurrent: usize) -> Self {
        DownloadQueue {
            user: VecDeque::new(),
            background: VecDeque::new(),
            max_concurrent: max_concurrent.max(1),
            active: 0,
        }
    }

    pub fn enqueue(&mut self, id: u64) {
        self.user.push_back(id);
    }

    pub fn enqueue_background(&mut self, id: u64) {
        self.background.push_back(id);
    }

    /// Pop the next download allowed to run, if any slot is free.
    pub fn next_ready(&mut self) -> Option<u64> {
        if self.active >= self.max_concurrent {
            return None;
        }
        let id = self.user.pop_front().or_else(|| self.background.pop_front())?;
        self.active += 1;
        Some(id)
    }

    pub fn on_complete(&mut self) {
        self.active = self.active.saturating_sub(1);
    }

    pub fn pause_all(&mut self) {
        self.active = 0;
    }

    pub fn len(&self) -> usize {
        self.user.len() + self.background.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A token bucket: capacity in bytes, refill at bytes/sec. The point is
/// not precision — it is that a big download cannot starve the page the
/// user is actually reading.
pub struct RateLimiter {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last_refill: std::time::Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: f64) -> Self {
        // One second of burst: enough to keep pipelines full, not enough
        // to hollow out the connection for a second and a half.
        RateLimiter {
            capacity: bytes_per_sec,
            tokens: bytes_per_sec,
            refill_per_sec: bytes_per_sec,
            last_refill: std::time::Instant::now(),
        }
    }

    pub fn unlimited() -> Self {
        RateLimiter::new(f64::INFINITY)
    }

    /// Try to take `bytes`; if the bucket is empty, return how long to
    /// wait for that many tokens.
    pub fn acquire(&mut self, bytes: f64) -> Result<(), std::time::Duration> {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);

        if self.tokens >= bytes {
            self.tokens -= bytes;
            return Ok(());
        }
        let deficit = bytes - self.tokens;
        Err(std::time::Duration::from_secs_f64(deficit / self.refill_per_sec))
    }

    pub fn set_rate(&mut self, bytes_per_sec: f64) {
        self.capacity = bytes_per_sec;
        self.refill_per_sec = bytes_per_sec;
        if self.tokens > bytes_per_sec {
            self.tokens = bytes_per_sec;
        }
    }
}
