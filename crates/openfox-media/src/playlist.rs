//! Playlist: Brave-class. An ordered list of media the user saved, played
//! sequentially with no shuffle (v1), persisted atomically — a torn
//! playlist is a lost playlist.

use crate::{parse_media_url, MediaError};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

/// Audio or video — decides the icon and the default surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Audio,
    Video,
}

/// One saved item; `id` is stable across saves and `url` must outlive the
/// page that saved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration_ms: Option<u64>,
    pub added_at: u64,
    pub kind: MediaKind,
}

impl PlaylistItem {
    /// Build an item from an observed media element: the URL it loaded,
    /// the title scraped around it, the duration it reported. `id` is a
    /// placeholder; [`Playlist::add`] assigns the real one.
    pub fn from_media_element(url: &str, title: &str, duration_ms: Option<u64>, kind: MediaKind) -> Self {
        let added_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        PlaylistItem {
            id: 0,
            url: url.to_owned(),
            title: if title.trim().is_empty() { url.to_owned() } else { title.trim().to_owned() },
            thumbnail: None,
            duration_ms,
            added_at,
            kind,
        }
    }
}

/// What `next()` does at the end of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    /// Stop: the last item is the last item.
    #[default]
    Off,
    /// Replay the current item.
    One,
    /// Wrap to the first item.
    All,
}

/// The playlist: ordered items, a cursor, a repeat mode, and an optional
/// home on disk — sequential playback, no shuffle v1.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Playlist {
    items: Vec<PlaylistItem>,
    repeat: RepeatMode,
    #[serde(skip)]
    path: Option<PathBuf>,
    next_id: u64,
    cursor: Option<usize>,
}

impl Playlist {
    /// An empty, in-memory playlist that saves nowhere.
    pub fn new() -> Self {
        Playlist { next_id: 1, ..Playlist::default() }
    }

    /// Load from disk; a missing file is a first run, not an error.
    pub async fn load(path: impl Into<PathBuf>) -> Result<Self, MediaError> {
        let path = path.into();
        let mut list: Playlist = match fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| MediaError::PlaylistIo(io::Error::new(io::ErrorKind::InvalidData, e)))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Playlist::default(),
            Err(e) => return Err(e.into()),
        };
        list.path = Some(path);
        list.next_id = list.items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        Ok(list)
    }

    /// Persist atomically — write a `.tmp` sibling, then rename over the
    /// target — so a crash mid-write never destroys the good copy: a torn
    /// playlist is a lost playlist.
    pub async fn save(&self) -> Result<(), MediaError> {
        let path = self.path.as_ref().ok_or_else(|| {
            MediaError::PlaylistIo(io::Error::new(io::ErrorKind::InvalidInput, "playlist has no path"))
        })?;
        let bytes = serde_json::to_vec(self)
            .map_err(|e| MediaError::PlaylistIo(io::Error::new(io::ErrorKind::InvalidData, e)))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, bytes).await?;
        fs::rename(&tmp, path).await?;
        Ok(())
    }

    /// Add an item, deduping by URL — `Ok(false)` means it was already
    /// here. `blob:` and `data:` URLs are refused on the way in: they die
    /// with the page that minted them, and saving either grows ghosts.
    pub fn add(&mut self, mut item: PlaylistItem) -> Result<bool, MediaError> {
        parse_media_url(&item.url)?;
        if self.items.iter().any(|i| i.url == item.url) {
            return Ok(false);
        }
        item.id = self.next_id;
        self.next_id += 1;
        self.items.push(item);
        Ok(true)
    }

    /// Remove an item by id; the cursor follows the playing item and
    /// clears if the list empties.
    pub fn remove(&mut self, id: u64) -> bool {
        let Some(pos) = self.items.iter().position(|i| i.id == id) else { return false };
        self.items.remove(pos);
        self.cursor = match self.cursor {
            Some(_c) if self.items.is_empty() => None,
            Some(c) if c > pos => Some(c - 1),
            Some(c) => Some(c.min(self.items.len() - 1)),
            None => None,
        };
        true
    }

    /// Move an item to `to_index` (clamped); the cursor travels with the
    /// playing item, not the index.
    pub fn reorder(&mut self, id: u64, to_index: usize) -> bool {
        let Some(from) = self.items.iter().position(|i| i.id == id) else { return false };
        let to = to_index.min(self.items.len() - 1);
        if from == to { return true; }
        let playing = self.cursor.and_then(|c| self.items.get(c)).map(|i| i.id);
        let item = self.items.remove(from);
        self.items.insert(to, item);
        if let Some(id) = playing {
            self.cursor = self.items.iter().position(|i| i.id == id);
        }
        true
    }

    /// Set the repeat mode.
    pub fn set_repeat(&mut self, repeat: RepeatMode) { self.repeat = repeat; }

    /// Index of the playing item, if playback has started.
    pub fn current_index(&self) -> Option<usize> { self.cursor }

    /// Advance and return the next item: wrap on [`RepeatMode::All`], stay
    /// put on [`RepeatMode::One`], and on [`RepeatMode::Off`] stop at the
    /// end — `None`, cursor parked on the last item. Sequential playback,
    /// no shuffle v1 — the Brave playlist semantics.
    pub fn next(&mut self) -> Option<&PlaylistItem> {
        if self.items.is_empty() { return None; }
        match self.cursor {
            None => self.cursor = Some(0),
            Some(_) if self.repeat == RepeatMode::One => {}
            Some(c) if c + 1 < self.items.len() => self.cursor = Some(c + 1),
            Some(_) if self.repeat == RepeatMode::All => self.cursor = Some(0),
            Some(_) => return None,
        }
        self.cursor.and_then(|c| self.items.get(c))
    }

    /// All items, in order.
    pub fn items(&self) -> &[PlaylistItem] { &self.items }
}
