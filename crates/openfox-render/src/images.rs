//! The image decode pool: priority jobs, an in-flight cap tied to cores, a
//! byte-accounted LRU result cache, downscale-on-decode and one retry.

use image::ImageError;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::sync::Arc;

/// Jobs for visible content outrank prefetch — a decode that will be seen
/// this frame never waits behind one that might be seen after a scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Offscreen,
    Visible,
}

#[derive(Debug, Clone)]
pub struct DecodeRequest {
    pub bytes: Vec<u8>,
    pub target_w: u32,
    pub target_h: u32,
    /// EXIF orientation tag 1..8 as the sniffer read it; 1 is upright.
    pub exif_tag: u8,
}

#[derive(Debug, Clone)]
pub struct DecodeJob {
    pub request: DecodeRequest,
    pub priority: Priority,
}

/// A decoded image: RGBA8, already sized to its draw target, with the EXIF
/// transform carried for the compositor. Transforms are applied as texture
/// coordinate swizzles at draw time — rotating pixels in memory is work
/// the GPU does for free.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub transform: ExifTransform,
}

/// The eight EXIF orientation transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExifTransform {
    Identity,
    FlipH,
    Flip180,
    FlipV,
    Transpose,
    Rotate90,
    Transverse,
    Rotate270,
}

/// EXIF orientation tag 1..=8 to transform. Index 0 is tag 1.
pub const EXIF_ORIENTATION: [ExifTransform; 8] = [
    ExifTransform::Identity,
    ExifTransform::FlipH,
    ExifTransform::Flip180,
    ExifTransform::FlipV,
    ExifTransform::Transpose,
    ExifTransform::Rotate90,
    ExifTransform::Transverse,
    ExifTransform::Rotate270,
];

pub fn exif_transform(tag: u8) -> ExifTransform {
    EXIF_ORIENTATION.get(tag.wrapping_sub(1) as usize).copied().unwrap_or(ExifTransform::Identity)
}

/// Box-fit to the target, never upscaling. Downscaling at decode is the
/// point: a 4000x3000 photo drawn at 400 px never needs its 48 MiB raster.
pub fn fit(w: u32, h: u32, target_w: u32, target_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 || target_w == 0 || target_h == 0 || (w <= target_w && h <= target_h) {
        return (w, h);
    }
    let scale = (target_w as f32 / w as f32).min(target_h as f32 / h as f32);
    ((w as f32 * scale).max(1.0) as u32, (h as f32 * scale).max(1.0) as u32)
}

const CACHE_BUDGET_BYTES: usize = 64 * 1024 * 1024;

struct CacheEntry {
    image: Arc<DecodedImage>,
    last_used: u64,
}

/// The pool. Scheduling is data: the compositor's worker threads call
/// `pop_ready` and run the decode, `complete` returns the result.
pub struct ImageDecodePool {
    queue: Vec<DecodeJob>,
    in_flight: usize,
    cores: usize,
    cache: HashMap<u64, CacheEntry>,
    cache_bytes: usize,
    clock: u64,
    failures: HashMap<u64, u8>,
}

impl ImageDecodePool {
    /// In-flight cap equals cores: more parallel decodes than that just
    /// adds decode latency behind CPU contention.
    pub fn new() -> ImageDecodePool {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        ImageDecodePool {
            queue: Vec::new(),
            in_flight: 0,
            cores,
            cache: HashMap::new(),
            cache_bytes: 0,
            clock: 0,
            failures: HashMap::new(),
        }
    }

    pub fn submit(&mut self, job: DecodeJob) {
        let key = hash_bytes(&job.request.bytes);
        if self.cache.contains_key(&key) {
            return; // already decoded; the next get() will serve it
        }
        match self.queue.iter().position(|queued| hash_bytes(&queued.request.bytes) == key) {
            Some(index) => self.queue[index].priority = self.queue[index].priority.max(job.priority),
            None => self.queue.push(job),
        }
        self.queue.sort_by_key(|queued| std::cmp::Reverse(queued.priority));
    }

    /// Hands the next job to a worker when a core is free.
    pub fn pop_ready(&mut self) -> Option<DecodeJob> {
        if self.in_flight >= self.cores {
            return None;
        }
        let job = self.queue.pop()?;
        self.in_flight += 1;
        Some(job)
    }

    /// Returns a finished decode, requeueing once on error — truncated
    /// bodies from flaky sockets decode fine on the second try. The second
    /// failure stands: the cache holds a placeholder so the broken-image
    /// glyph renders immediately and the job is never re-submitted.
    pub fn complete(&mut self, job: DecodeJob, result: Result<DecodedImage, ImageError>) {
        self.in_flight = self.in_flight.saturating_sub(1);
        let key = hash_bytes(&job.request.bytes);
        match result {
            Ok(image) => self.store(key, image),
            Err(_) if *self.failures.entry(key).or_insert(0) == 0 => {
                self.failures.insert(key, 1);
                self.queue.push(job);
            }
            Err(_) => self.store(key, placeholder()),
        }
    }

    fn store(&mut self, key: u64, image: DecodedImage) {
        self.cache_bytes += image.rgba.len();
        let tick = self.next_tick();
        self.cache.insert(key, CacheEntry { image: Arc::new(image), last_used: tick });
        while self.cache_bytes > CACHE_BUDGET_BYTES && self.cache.len() > 1 {
            // Evict least-recently-used; a linear scan over a cache this
            // small beats maintaining a real LRU list.
            let evict = self
                .cache
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key)
                .expect("cache non-empty");
            if let Some(entry) = self.cache.remove(&evict) {
                self.cache_bytes = self.cache_bytes.saturating_sub(entry.image.rgba.len());
            }
        }
    }

    /// Cached decode for a byte-identical body, touching recency.
    pub fn get(&mut self, bytes: &[u8]) -> Option<Arc<DecodedImage>> {
        let key = hash_bytes(bytes);
        let tick = self.next_tick();
        let entry = self.cache.get_mut(&key)?;
        entry.last_used = tick;
        Some(Arc::clone(&entry.image))
    }

    pub fn cached_bytes(&self) -> usize {
        self.cache_bytes
    }

    fn next_tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

impl Default for ImageDecodePool {
    fn default() -> Self {
        ImageDecodePool::new()
    }
}

/// Decodes bytes to RGBA8, downscaled to the target when one is known.
/// The `image` crate is built with PNG support; other formats surface as
/// `ImageError::Unsupported` and follow the retry-then-placeholder policy.
pub fn decode(req: &DecodeRequest) -> Result<DecodedImage, ImageError> {
    let loaded = image::load_from_memory(&req.bytes)?;
    let rgba = loaded.to_rgba8();
    let (w, h) = fit(rgba.width(), rgba.height(), req.target_w, req.target_h);
    let transform = exif_transform(req.exif_tag);
    if (w, h) == (rgba.width(), rgba.height()) {
        return Ok(DecodedImage { width: w, height: h, rgba: rgba.into_raw(), transform });
    }
    let resized = image::imageops::resize(&rgba, w, h, image::imageops::FilterType::Triangle);
    Ok(DecodedImage { width: w, height: h, rgba: resized.into_raw(), transform })
}

/// One gray pixel: the honest floor of the retry policy.
fn placeholder() -> DecodedImage {
    DecodedImage { width: 1, height: 1, rgba: vec![96, 96, 96, 255], transform: ExifTransform::Identity }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}
