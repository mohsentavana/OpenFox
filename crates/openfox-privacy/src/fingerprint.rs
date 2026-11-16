//! Fingerprint farbling: lie to trackers, consistently. Every surface a
//! script can measure gets a deterministic perturbation derived from the
//! session seed and the site's origin, so the same fingerprint is stable
//! on one site — pages keep working — and different on every other
//! site — correlation goes nowhere.

use std::time::{SystemTime, UNIX_EPOCH};

/// The farbling contract: perturbed values are stable within a site for
/// the whole session and useless across sites. A tracker reading canvas
/// output on example.com sees the same jitter all day; the same tracker
/// on example.org sees different jitter; neither matches the real
/// machine. A fingerprint stops being an identity and becomes noise.
#[derive(Debug, Clone)]
pub struct Farbler {
    /// The per-session key every surface stream derives from. Never
    /// persisted, never exposed to the page.
    session_seed: [u8; 32],
}

/// The three platform strings that dominate the installed base.
const PLATFORMS: [&str; 3] = ["Win32", "MacIntel", "X11"];

impl Farbler {
    /// A farbler with a caller-supplied seed, for restoring a session's
    /// lies verbatim or pinning them in tests.
    pub fn from_seed(session_seed: [u8; 32]) -> Self {
        Farbler { session_seed }
    }

    /// A farbler seeded from the clock, the pid and a stack address.
    /// That is not cryptographic entropy — it only has to be unguessable
    /// by the page, not by the kernel.
    pub fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut hasher = blake3::Hasher::new();
        hasher.update(&nanos.to_le_bytes());
        hasher.update(&std::process::id().to_le_bytes());
        hasher.update(&(&nanos as *const u64 as usize).to_le_bytes());
        let mut seed = [0u8; 32];
        seed.copy_from_slice(hasher.finalize().as_bytes());
        Farbler { session_seed: seed }
    }

    /// The deterministic stream for one (origin, surface) pair: keyed
    /// BLAKE3 over "origin|surface", first 8 bytes as the seed of a
    /// xorshift64* generator. Same pair, same stream, every time.
    fn stream(&self, origin: &str, surface: &str) -> Xorshift64Star {
        let mut hasher = blake3::Hasher::new_keyed(&self.session_seed);
        hasher.update(origin.as_bytes());
        hasher.update(b"|");
        hasher.update(surface.as_bytes());
        let mut chunk = [0u8; 8];
        chunk.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
        Xorshift64Star::new(u64::from_le_bytes(chunk))
    }

    /// A deterministic −1, 0 or +1 perturbation for the canvas pixel at
    /// (x, y). Invisible to the eye, fatal to a pixel hash.
    pub fn canvas_jitter(&self, origin: &str, x: u32, y: u32) -> i8 {
        let mut rng = self.stream(origin, "canvas");
        let mixed = rng
            .next_u64()
            ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        (mixed % 3) as i8 - 1
    }

    /// A constant fake noise floor for this origin, somewhere in the
    /// −96…−84 dB range where real ones live. The AudioContext
    /// fingerprint reads the same lie twice on the same site.
    pub fn audio_noise_floor(&self, origin: &str) -> f32 {
        let mut rng = self.stream(origin, "audio");
        -96.0 + rng.next_f32() * 12.0
    }

    /// Quantize a font measurement to a tenth of a unit. Sub-0.1
    /// differences are rendering noise anyway — keeping them only helps
    /// trackers. Origin-blind by design: the quantum carries no entropy,
    /// so there is nothing to derive.
    pub fn font_metric(&self, origin: &str, metric: f32) -> f32 {
        let _ = origin;
        (metric * 10.0).round() / 10.0
    }

    /// Report one of the three dominant `navigator.platform` strings.
    /// Real values already among the big three pass through; anything
    /// exotic collapses to a deterministic pick. This reduces entropy
    /// instead of adding it — an oddball truth is a tracking gift.
    pub fn client_platform(&self, origin: &str, real: &str) -> String {
        if PLATFORMS.contains(&real) {
            return real.to_owned();
        }
        let mut rng = self.stream(origin, "platform");
        PLATFORMS[(rng.next_u64() % PLATFORMS.len() as u64) as usize].to_owned()
    }
}

/// xorshift64*: one multiply away from decent. The point is determinism,
/// not cryptography — the secret is the key, not the generator.
struct Xorshift64Star {
    state: u64,
}

impl Xorshift64Star {
    fn new(seed: u64) -> Self {
        Xorshift64Star { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A float in [0, 1): 24 mantissa bits, no rounding surprises.
    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }
}
