//! The record: a BSO-shaped envelope with a sealed payload. Tombstones are
//! the one payload whose meaning survives without unsealing — a delete must
//! be able to win a merge even when the winner's key is gone.

use crate::SyncError;

/// What the opened bytes mean. The first byte `0x00` is the tombstone
/// marker; everything else is JSON.
pub const TOMBSTONE_PLAINTEXT: &[u8; 1] = b"\x00";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncRecord {
    pub id: String,
    pub collection: String,
    pub modified_ms: u64,
    pub ttl: Option<u32>,
    pub envelope: crate::crypto::Envelope,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ClearPayload {
    Bookmark {
        title: String,
        uri: String,
        parent: Option<String>,
        position: u32,
    },
    HistoryVisit {
        uri: String,
        title: String,
        visited_at_ms: u64,
    },
    Password {
        origin: String,
        username: String,
        secret: String,
    },
    Tab {
        title: String,
        url: String,
        last_used_ms: u64,
    },
    Tombstone,
}

impl ClearPayload {
    /// True when the payload deletes its id rather than describing it.
    pub fn is_tombstone(&self) -> bool {
        matches!(self, ClearPayload::Tombstone)
    }
}

/// Decode opened bytes into a payload. The tombstone marker is checked
/// before JSON parsing — a delete must survive even a payload the loser
/// wrote with a different schema.
pub fn decode(opened: &[u8]) -> Result<ClearPayload, SyncError> {
    if opened.first() == Some(&TOMBSTONE_PLAINTEXT[0]) {
        return Ok(ClearPayload::Tombstone);
    }
    serde_json::from_slice(opened).map_err(|e| SyncError::BadRecord(e.to_string()))
}

/// Encode a payload into opened bytes, tombstone marker included.
pub fn encode(payload: &ClearPayload) -> Result<Vec<u8>, SyncError> {
    if payload.is_tombstone() {
        return Ok(TOMBSTONE_PLAINTEXT.to_vec());
    }
    serde_json::to_vec(payload).map_err(SyncError::Serde)
}
