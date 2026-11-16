//! The sync server client. A transport trait so the engine never touches
//! sockets — `openfox-net` implements this over the real client, and the
//! tests implement it over a Vec.

use crate::{SyncError, SyncRecord};
use std::collections::HashMap;

/// The transport the engine is allowed to know about. Nothing more.
pub trait HttpFetch {
    fn get(&self, path: &str, headers: &[(String, String)]) -> Result<String, SyncError>;
    fn post(
        &self,
        path: &str,
        body: &str,
        headers: &[(String, String)],
    ) -> Result<String, SyncError>;
}

#[derive(Debug)]
pub struct PushResult {
    pub accepted: u32,
    pub failed: u32,
    pub backoff_ms: Option<u64>,
}

pub struct SyncClient<'a> {
    base_url: String,
    token: String,
    fetch: &'a dyn HttpFetch,
}

impl<'a> SyncClient<'a> {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>, fetch: &'a dyn HttpFetch) -> Self {
        SyncClient {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            fetch,
        }
    }

    fn auth(&self) -> Vec<(String, String)> {
        vec![
            ("Authorization".into(), format!("Bearer {}", self.token)),
            ("Content-Type".into(), "application/json".into()),
        ]
    }

    /// The server's collection -> last-modified map. This is the cheap
    /// "is there anything to do" call.
    pub fn info_collections(&self) -> Result<HashMap<String, u64>, SyncError> {
        let body = self
            .fetch
            .get(&format!("{}/info/collections", self.base_url), &self.auth())?;
        serde_json::from_str(&body).map_err(SyncError::Serde)
    }

    /// Pull everything in a collection modified after the cursor.
    pub fn pull(&self, collection: &str, newer_than_ms: u64) -> Result<Vec<SyncRecord>, SyncError> {
        let body = self.fetch.get(
            &format!("{}/{collection}?newer={newer_than_ms}", self.base_url),
            &self.auth(),
        )?;
        serde_json::from_str(&body).map_err(SyncError::Serde)
    }

    /// Push a batch. The server answers per-id; we summarize.
    pub fn push(&self, collection: &str, records: &[SyncRecord]) -> Result<PushResult, SyncError> {
        let body = serde_json::to_string(records).map_err(SyncError::Serde)?;
        let response = self.fetch.post(
            &format!("{}/{collection}", self.base_url),
            &body,
            &self.auth(),
        )?;
        let mut accepted = 0u32;
        let mut failed = 0u32;
        if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&response) {
            for value in map.values() {
                if value.is_object() {
                    failed += 1;
                } else {
                    accepted += 1;
                }
            }
        }
        Ok(PushResult {
            accepted,
            failed,
            backoff_ms: None,
        })
    }
}

/// Parse `X-Weave-Backoff` or `Retry-After` out of a raw header block.
/// The server said wait — this is how the engine learns by how much.
pub fn parse_backoff(header_block: &str) -> Option<u64> {
    for line in header_block.lines() {
        let (name, value) = line.split_once(':')?;
        let name = name.trim().to_ascii_lowercase();
        if name == "x-weave-backoff" || name == "retry-after" {
            if let Ok(seconds) = value.trim().parse::<u64>() {
                return Some(seconds * 1000);
            }
        }
    }
    None
}
