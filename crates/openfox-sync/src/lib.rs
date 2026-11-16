//! End-to-end encrypted sync. The server stores sealed envelopes it cannot
//! read; keys never leave the device. Collections: bookmarks, history,
//! passwords, tabs.
//!
//! The trust model is one line: the server is untrusted storage. A full
//! server compromise yields ciphertext plus collection names and sizes —
//! nothing else.

pub mod client;
pub mod crypto;
pub mod engine;
pub mod merge;
pub mod record;

pub use client::{HttpFetch, PushResult, SyncClient};
pub use crypto::{Envelope, SyncIdentity};
pub use engine::{SyncEngine, SyncState, SyncSummary};
pub use merge::{Entry, MergeResult};
pub use record::{ClearPayload, SyncRecord};

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("http transport: {0}")]
    Http(String),
    #[error("bad record: {0}")]
    BadRecord(String),
    #[error("remote clock ahead of local by more than the skew guard")]
    ClockSkew,
}
