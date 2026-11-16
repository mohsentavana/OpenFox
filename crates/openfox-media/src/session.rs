//! Media Session: the browser's half of the hardware media keys. Pages
//! declare metadata and action support; the OS integration reads the
//! resulting [`MediaSession`] to paint the overlays. Nothing here talks
//! to the OS — an unset session renders nothing, which beats "Unknown
//! artist" forever.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One artwork candidate, mirroring the JS `MediaImage` shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artwork {
    pub src: String,
    pub sizes: String,
    pub mime: String,
}

/// Page-declared metadata, serde-shaped like the JS `MediaMetadata`
/// constructor so it crosses IPC without a translator.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork: Vec<Artwork>,
}

impl MediaMetadata {
    /// Parse flat `(key, value)` pairs — the shape meta tags and IPC
    /// bridges hand over: `title`, `artist`, `album`, and artwork in
    /// indexed forms (`artwork[0].src`, `artwork0src`, …) with `src`,
    /// `sizes`, and `type` properties — the JS MediaMetadata shape.
    /// Artwork with no `src` is dropped.
    pub fn from_meta_pairs(pairs: &[(String, String)]) -> Self {
        let mut meta = MediaMetadata::default();
        for (key, value) in pairs {
            let value = value.trim();
            if value.is_empty() { continue; }
            match key.to_ascii_lowercase().as_str() {
                "title" => meta.title = Some(value.to_owned()),
                "artist" => meta.artist = Some(value.to_owned()),
                "album" => meta.album = Some(value.to_owned()),
                lower => {
                    if let Some((idx, prop)) = artwork_key(lower) {
                        if meta.artwork.len() <= idx { meta.artwork.resize_with(idx + 1, Artwork::default); }
                        let slot = &mut meta.artwork[idx];
                        match prop {
                            "src" => slot.src = value.to_owned(),
                            "sizes" => slot.sizes = value.to_owned(),
                            "type" | "mime" => slot.mime = value.to_owned(),
                            _ => {}
                        }
                    }
                }
            }
        }
        meta.artwork.retain(|a| !a.src.is_empty());
        meta
    }
}

/// Split an `artwork[0].src`-style key into (index, property); separators
/// are optional, because the web sends `artwork0src` and worse.
fn artwork_key(lower: &str) -> Option<(usize, &str)> {
    let rest = lower.strip_prefix("artwork")?.trim_start_matches(['.', ' ', '[']);
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 { return None; }
    let idx = rest[..digits].parse().ok()?;
    let prop = rest[digits..].trim_start_matches(['.', ' ', ']']);
    Some((idx, prop))
}

/// Actions a page can declare, per the JS MediaSession action set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaAction {
    Play,
    Pause,
    Previous,
    Next,
    SeekBack,
    SeekForward,
    Stop,
}

/// The keys that physically exist on keyboards and headsets. Mapping from
/// actions is lossy: `SeekBack` has no key, and pretending breeds phantoms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsMediaKey {
    PlayPause,
    Next,
    Previous,
    Stop,
}

/// The OS key that triggers an action, if one exists.
pub fn os_key(action: MediaAction) -> Option<OsMediaKey> {
    match action {
        MediaAction::Play | MediaAction::Pause => Some(OsMediaKey::PlayPause),
        MediaAction::Next => Some(OsMediaKey::Next),
        MediaAction::Previous => Some(OsMediaKey::Previous),
        MediaAction::Stop => Some(OsMediaKey::Stop),
        MediaAction::SeekBack | MediaAction::SeekForward => None,
    }
}

/// Live session state: what the OS integration reads to render hardware
/// media keys. The session owns the math; the OS owns the paint.
#[derive(Debug, Clone, Default)]
pub struct MediaSession {
    metadata: Option<MediaMetadata>,
    supported: HashSet<MediaAction>,
    position_ms: u64,
    duration_ms: Option<u64>,
    playback_rate: f32,
    playing: bool,
}

impl MediaSession {
    /// A fresh session: no metadata, no actions, clock at zero, paused.
    pub fn new() -> Self {
        MediaSession { playback_rate: 1.0, ..MediaSession::default() }
    }

    /// Replace page metadata; absent fields mean the page never set them,
    /// not that they are empty strings.
    pub fn set_metadata(&mut self, metadata: MediaMetadata) { self.metadata = Some(metadata); }

    /// Current metadata, if the page set any.
    pub fn metadata(&self) -> Option<&MediaMetadata> { self.metadata.as_ref() }

    /// Declare or retract support for an action; unsupported actions are
    /// hidden from the OS overlay, not shown in disabled grey.
    pub fn set_action_supported(&mut self, action: MediaAction, supported: bool) {
        if supported { self.supported.insert(action); } else { self.supported.remove(&action); }
    }

    /// Whether the page declared an action.
    pub fn supports(&self, action: MediaAction) -> bool { self.supported.contains(&action) }

    /// Mark playback started.
    pub fn play(&mut self) { self.playing = true; }

    /// Mark playback paused; the position is kept, not reset.
    pub fn pause(&mut self) { self.playing = false; }

    /// Advance by `delta_ms` of wall time; position moves by `delta * rate`,
    /// the media element's own math.
    pub fn advance(&mut self, delta_ms: u64) {
        if !self.playing { return; }
        let end = self.position_ms + (delta_ms as f32 * self.playback_rate) as u64;
        self.position_ms = self.duration_ms.map_or(end, |d| end.min(d));
    }

    /// Seek to a position, clamped to duration when one is known.
    pub fn seek(&mut self, to_ms: u64) { self.position_ms = self.duration_ms.map_or(to_ms, |d| to_ms.min(d)); }

    /// Set or clear the duration; the position clamps immediately.
    pub fn set_duration(&mut self, duration_ms: Option<u64>) {
        self.duration_ms = duration_ms;
        self.position_ms = duration_ms.map_or(self.position_ms, |d| self.position_ms.min(d));
    }

    /// Set the playback rate; negatives clamp to zero — rewinding by rate
    /// is a foot-gun no page needs.
    pub fn set_playback_rate(&mut self, rate: f32) { self.playback_rate = rate.max(0.0); }

    /// Current media position in milliseconds.
    pub fn position(&self) -> u64 { self.position_ms }

    /// Duration in milliseconds, if the page declared one.
    pub fn duration(&self) -> Option<u64> { self.duration_ms }

    /// Whether the clock is running.
    pub fn is_playing(&self) -> bool { self.playing }
}
