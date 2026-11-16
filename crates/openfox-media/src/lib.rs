//! Media: picture-in-picture, media-session keys, playlist, codec truth
//! tables. Video is a frame producer; this crate owns the policy around it —
//! surfaces belong to the platform, frames to openfox-render.
//!
//! Module map:
//! - [`pip`]: when a PiP window exists and how big it is
//! - [`session`]: what the OS media-key overlay reads
//! - [`playlist`]: Brave-class saved media — sequential, persisted, no
//!   shuffle v1
//! - [`codec`]: can-play and hardware-decode truth tables
//!
//! Nothing here opens a window, touches an audio device, or links a codec
//! library. This crate is the paperwork; the platform crates do the work,
//! and the truth tables keep that work from being guesswork. The split is
//! deliberate: policy decisions (should PiP auto-enter? is this codec
//! playable? which item is next?) are plain, replayable data here, while
//! everything that needs a compositor or an audio device lives elsewhere.
//!
//! Root utilities: [`parse_media_url`] is the one URL gate every saver
//! walks through, and [`format_timecode`] is the one string every media UI
//! needs. Both exist so the policy modules don't grow private copies.

pub mod codec;
pub mod pip;
pub mod playlist;
pub mod session;

use thiserror::Error;

/// Everything that can go wrong in media policy. Stream failures belong to
/// the engine; these are the failures of deciding, listing, and persisting.
#[derive(Debug, Error)]
pub enum MediaError {
    /// An operation needed live media and found none.
    #[error("no active media")]
    NoActiveMedia,
    /// The playlist file could not be read, written, or parsed.
    #[error("playlist io: {0}")]
    PlaylistIo(#[from] std::io::Error),
    /// A media URL used a scheme playback can never fetch back.
    #[error("unsupported scheme: {0}")]
    UnsupportedScheme(String),
}

/// Parse a media URL, keeping only the schemes a playlist can fetch again
/// tomorrow. `blob:` and `data:` die with the page that minted them —
/// persisting either is how playlists grow ghosts.
pub fn parse_media_url(raw: &str) -> Result<url::Url, MediaError> {
    let url = url::Url::parse(raw).map_err(|_| MediaError::UnsupportedScheme(raw.to_owned()))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        other => Err(MediaError::UnsupportedScheme(other.to_owned())),
    }
}

/// Render a millisecond position as `m:ss` / `h:mm:ss` — the one string
/// every media UI needs, and the one everyone re-implements badly.
pub fn format_timecode(ms: u64) -> String {
    let secs = ms / 1000;
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

// The crate's public surface, flattened: callers shouldn't need to know
// which module holds what.
pub use codec::{can_play, hardware_decode, HardwareBackend, Playability, Platform};
pub use pip::{AutoEnterPolicy, PipCommand, PipState, PipWindow, SurfaceSpec};
pub use playlist::{MediaKind, Playlist, PlaylistItem, RepeatMode};
pub use session::{Artwork, MediaAction, MediaMetadata, MediaSession, OsMediaKey};
