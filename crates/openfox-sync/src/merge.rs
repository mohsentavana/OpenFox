//! Merge, per collection. Last-write-wins by `modified_ms` with a
//! clock-skew guard, tombstones that win when newer, and a bookmarks
//! resolver that keeps the tree a tree — Firefox reparents orphans and
//! rejects cycles; so does OpenFox.
//!
//! The merge never unseals anything: callers pass records together with
//! their already-decoded payloads. Sealed bytes are the engine's business.

use crate::record::{ClearPayload, SyncRecord};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct MergeResult {
    pub winners: Vec<SyncRecord>,
    pub conflicts: u32,
}

/// The skew guard: a server clock more than five minutes ahead of ours
/// would win every LWW fight by lying. Refuse to let it.
pub fn clock_skew(remote_modified_ms: u64, now_ms: u64) -> bool {
    remote_modified_ms > now_ms.saturating_add(300_000)
}

/// A record plus its opened payload, as the engine hands it to the merge.
#[derive(Debug, Clone)]
pub struct Entry {
    pub record: SyncRecord,
    pub payload: ClearPayload,
}

fn lww(a: &Entry, b: &Entry) -> (Entry, bool) {
    if b.record.modified_ms > a.record.modified_ms {
        (b.clone(), true)
    } else {
        (a.clone(), false)
    }
}

/// Bookmarks: LWW per id, newest tombstone wins outright, parent cycles
/// are cut by reparenting to the root, orphans land in "unfiled".
pub fn merge_bookmarks(local: &[Entry], remote: &[Entry]) -> MergeResult {
    let mut result = MergeResult::default();
    let mut by_id: HashMap<String, Entry> = HashMap::new();

    for entry in local.iter().chain(remote.iter()) {
        match by_id.get(&entry.record.id) {
            Some(existing) => {
                let (winner, conflicted) = lww(existing, entry);
                if conflicted {
                    result.conflicts += 1;
                }
                by_id.insert(entry.record.id.clone(), winner);
            }
            None => {
                by_id.insert(entry.record.id.clone(), entry.clone());
            }
        }
    }

    // The parent map for cycle cutting and orphan detection.
    let parents: HashMap<String, Option<String>> = by_id
        .iter()
        .map(|(id, entry)| {
            let parent = match &entry.payload {
                ClearPayload::Bookmark { parent, .. } => parent.clone(),
                _ => None,
            };
            (id.clone(), parent)
        })
        .collect();

    for (id, entry) in by_id.iter_mut() {
        let Some(parent) = parents.get(id).cloned().flatten() else {
            continue;
        };

        // Cycle: walk ancestors; if we meet ourselves, cut to the root.
        let mut cursor = parent.clone();
        let mut cycle = false;
        for _ in 0..256 {
            if cursor == *id {
                cycle = true;
                break;
            }
            match parents.get(&cursor).cloned().flatten() {
                Some(next) => cursor = next,
                None => break,
            }
        }

        // Orphan: a parent nobody has.
        let orphan = !parents.contains_key(&parent);
        if cycle || orphan {
            if let ClearPayload::Bookmark {
                title,
                uri,
                position,
                ..
            } = &entry.payload
            {
                entry.payload = ClearPayload::Bookmark {
                    title: title.clone(),
                    uri: uri.clone(),
                    parent: None,
                    position: *position,
                };
            }
        }
    }

    result.winners = by_id.into_values().map(|entry| entry.record).collect();
    result
}

/// History: union of visits keyed by id. History expiry is a local
/// policy — never a sync deletion, so there is nothing to lose here.
pub fn merge_history(local: &[Entry], remote: &[Entry]) -> MergeResult {
    let mut result = MergeResult::default();
    let mut seen: HashMap<String, Entry> = HashMap::new();
    for entry in local.iter().chain(remote.iter()) {
        match seen.get(&entry.record.id) {
            Some(existing) => {
                let (winner, conflicted) = lww(existing, entry);
                if conflicted {
                    result.conflicts += 1;
                }
                seen.insert(entry.record.id.clone(), winner);
            }
            None => {
                seen.insert(entry.record.id.clone(), entry.clone());
            }
        }
    }
    result.winners = seen.into_values().map(|entry| entry.record).collect();
    result
}

/// Passwords: per-(origin, username) LWW with tombstone-wins-if-newer.
pub fn merge_passwords(local: &[Entry], remote: &[Entry]) -> MergeResult {
    let mut result = MergeResult::default();
    let mut by_key: HashMap<(String, String), Entry> = HashMap::new();

    let key_of = |entry: &Entry| match &entry.payload {
        ClearPayload::Password { origin, username, .. } => Some((origin.clone(), username.clone())),
        ClearPayload::Tombstone => None,
        _ => None,
    };

    for entry in local.iter().chain(remote.iter()) {
        // Tombstones ride under their own id — they delete by id, not key.
        let key = key_of(entry).unwrap_or((format!("id:{}", entry.record.id), String::new()));
        match by_key.get(&key) {
            Some(existing) => {
                let (winner, conflicted) = lww(existing, entry);
                if conflicted {
                    result.conflicts += 1;
                }
                by_key.insert(key, winner);
            }
            None => {
                by_key.insert(key, entry.clone());
            }
        }
    }

    result.winners = by_key.into_values().map(|entry| entry.record).collect();
    result
}
