use serde_json::{json, Value};

/// Frame timing. Honest numbers: mean, max, and a real p95 over a bounded
/// window — not a smoothed "FPS" that hides the spikes.
pub struct FrameProfiler {
    samples: std::collections::VecDeque<f32>,
    frames: u64,
}

const WINDOW: usize = 600; // ten seconds at 60 Hz
const BAD_FRAME_MS: f32 = 25.0;

impl FrameProfiler {
    pub fn new() -> Self {
        FrameProfiler { samples: std::collections::VecDeque::with_capacity(WINDOW), frames: 0 }
    }

    pub fn frame(&mut self, ms: f32) {
        self.frames += 1;
        if self.samples.len() == WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn mean_ms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f32>() / self.samples.len() as f32
    }

    pub fn max_ms(&self) -> f32 {
        self.samples.iter().copied().fold(0.0, f32::max)
    }

    /// p95 of the recent window: 19 of 20 frames are at least this fast.
    pub fn p95_ms(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f32> = self.samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("f32 frame times are never NaN"));
        sorted[(sorted.len() as f32 * 0.95) as usize % sorted.len()]
    }

    pub fn dropped_count(&self) -> u64 {
        self.samples.iter().filter(|&&ms| ms > BAD_FRAME_MS).count() as u64
    }

    pub fn to_json(&self) -> Value {
        json!({
            "frames": self.frames,
            "meanMs": self.mean_ms(),
            "maxMs": self.max_ms(),
            "p95Ms": self.p95_ms(),
            "framesOver25Ms": self.dropped_count(),
        })
    }
}

impl Default for FrameProfiler {
    fn default() -> Self {
        FrameProfiler::new()
    }
}
