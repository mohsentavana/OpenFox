//! The engine: pull, unseal, merge, push, advance the cursor. Backoff is
//! honored to the millisecond the server asked for — no retry storms.

use crate::client::{PushResult, SyncClient};
use crate::merge::{self, Entry};
use crate::record::{decode, SyncRecord};
use crate::{SyncError, SyncIdentity};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use x25519_dalek::PublicKey;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    Idle,
    Checking,
    Pulling(String),
    Merging(String),
    Pushing(String),
    Backoff(Duration),
    Done,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct SyncSummary {
    pub pulled: u32,
    pub pushed: u32,
    pub conflicts_resolved: u32,
    pub duration_ms: u64,
}

pub struct SyncEngine {
    cursors: HashMap<String, u64>,
    backoff_until: Option<Instant>,
}

impl Default for SyncEngine {
    fn default() -> Self {
        SyncEngine {
            cursors: HashMap::new(),
            backoff_until: None,
        }
    }
}

impl SyncEngine {
    pub fn cursor(&self, collection: &str) -> u64 {
        self.cursors.get(collection).copied().unwrap_or(0)
    }

    pub fn state(&self) -> SyncState {
        match self.backoff_until {
            Some(until) if until > Instant::now() => SyncState::Backoff(until - Instant::now()),
            _ => SyncState::Idle,
        }
    }

    /// One full sync pass across the given collections. The local records
    /// come in as a parameter — the engine does not own storage.
    pub async fn sync(
        &mut self,
        client: &SyncClient<'_>,
        identity: &SyncIdentity,
        remote_public: &PublicKey,
        collections: &[&str],
        local: &[SyncRecord],
        now_ms: u64,
    ) -> Result<SyncSummary, SyncError> {
        let started = Instant::now();

        if let Some(until) = self.backoff_until {
            if until > Instant::now() {
                tokio::time::sleep(until - Instant::now()).await;
            }
        }
        self.backoff_until = None;

        let mut summary = SyncSummary::default();

        for collection in collections {
            let cursor = self.cursor(collection);
            let pulled = client.pull(collection, cursor)?;
            summary.pulled += pulled.len() as u32;

            // Unseal with the skew guard: a remote clock lying about the
            // future is refused, not obeyed.
            let mut remote_entries: Vec<Entry> = Vec::with_capacity(pulled.len());
            for record in &pulled {
                if merge::clock_skew(record.modified_ms, now_ms) {
                    return Err(SyncError::ClockSkew);
                }
                let opened = identity.open(remote_public, collection, &record.envelope)?;
                let payload = decode(&opened)?;
                remote_entries.push(Entry {
                    record: record.clone(),
                    payload,
                });
            }

            // Local records arrive sealed from the caller; the engine
            // opens them with the same key so the merge never sees mixed
            // provenance.
            let mut local_entries: Vec<Entry> = Vec::new();
            for record in local.iter().filter(|r| r.collection == *collection) {
                if let Ok(opened) = identity.open(remote_public, collection, &record.envelope) {
                    if let Ok(payload) = decode(&opened) {
                        local_entries.push(Entry {
                            record: record.clone(),
                            payload,
                        });
                    }
                }
            }

            let merged = match *collection {
                "bookmarks" => merge::merge_bookmarks(&local_entries, &remote_entries),
                "passwords" => merge::merge_passwords(&local_entries, &remote_entries),
                _ => merge::merge_history(&local_entries, &remote_entries),
            };
            let conflicts = merged.conflicts;

            let to_push: Vec<SyncRecord> = merged
                .winners
                .into_iter()
                .filter(|record| !pulled.iter().any(|p| p.id == record.id))
                .collect();
            if !to_push.is_empty() {
                let push_result: PushResult = client.push(collection, &to_push)?;
                summary.pushed += push_result.accepted;
            }

            if let Some(newest) = pulled.iter().map(|record| record.modified_ms).max() {
                self.cursors.insert((*collection).to_owned(), newest);
            }
            summary.conflicts_resolved += conflicts;
        }

        summary.duration_ms = started.elapsed().as_millis() as u64;
        Ok(summary)
    }

    /// The server said wait — remember it for the next pass.
    pub fn note_backoff(&mut self, ms: u64) {
        self.backoff_until = Some(Instant::now() + Duration::from_millis(ms));
    }
}
