//! The storage inspector: enumerators per kind, filter, delete-selection,
//! and an export intent that reuses the HAR envelope.

use serde_json::{json, Value};
use std::collections::HashSet;

/// Every storage kind a page can pile up. One inspector, one filter, one
/// delete gesture — the kinds differ in shape, not in verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageKind {
    LocalStorage,
    SessionStorage,
    IndexedDb,
    Cookies,
    CacheStorage,
}

impl StorageKind {
    pub fn key(&self) -> &'static str {
        match self {
            StorageKind::LocalStorage => "storage.local",
            StorageKind::SessionStorage => "storage.session",
            StorageKind::IndexedDb => "storage.idb",
            StorageKind::Cookies => "storage.cookies",
            StorageKind::CacheStorage => "storage.cache",
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyValueRow {
    pub key: String,
    pub value: String,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct IdbStore {
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone)]
pub struct IdbDatabase {
    pub name: String,
    pub stores: Vec<IdbStore>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Lax,
    Strict,
    None,
}

impl SameSite {
    pub fn key(&self) -> &'static str {
        match self {
            SameSite::Lax => "cookie.samesite.lax",
            SameSite::Strict => "cookie.samesite.strict",
            SameSite::None => "cookie.samesite.none",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CookieRow {
    pub name: String,
    pub value: String,
    pub host: String,
    pub expiry_ms: i64,
    pub is_secure: bool,
    pub same_site: SameSite,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub url: String,
    pub bytes: u64,
}

/// The rows of one kind, only one of which is live at a time.
#[derive(Debug, Clone)]
pub enum StorageData {
    Kv(Vec<KeyValueRow>),
    Idb(Vec<IdbDatabase>),
    Cookies(Vec<CookieRow>),
    Cache(Vec<CacheEntry>),
}

/// What the inspector asks the storage layer to do. Export reuses the HAR
/// envelope — entries with a timestamp, duration and body — because every
/// tool that reads HAR already reads it; a second export format would be a
/// second format to maintain for no reader.
#[derive(Debug, Clone)]
pub enum StorageIntent {
    Delete { kind: StorageKind, keys: Vec<String> },
    Clear(StorageKind),
    ExportHar,
}

/// Inspector state for the active kind.
pub struct StorageInspector {
    pub kind: StorageKind,
    pub data: StorageData,
    pub filter: String,
    pub selection: HashSet<String>,
    outbox: Vec<StorageIntent>,
}

impl StorageInspector {
    pub fn new(kind: StorageKind, data: StorageData) -> StorageInspector {
        StorageInspector { kind, data, filter: String::new(), selection: HashSet::new(), outbox: Vec::new() }
    }

    /// Row identity per kind: the key, the database, or the host — whatever
    /// the delete gesture can address.
    pub fn row_key(&self, index: usize) -> Option<String> {
        match &self.data {
            StorageData::Kv(rows) => rows.get(index).map(|row| row.key.clone()),
            StorageData::Idb(rows) => rows.get(index).map(|row| row.name.clone()),
            StorageData::Cookies(rows) => rows.get(index).map(|row| format!("{}@{}", row.name, row.host)),
            StorageData::Cache(rows) => rows.get(index).map(|row| row.url.clone()),
        }
    }

    /// Indices of rows surviving the filter. The filter matches every
    /// textual field the kind has, because users search by value too.
    pub fn visible(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        let test = |haystacks: &[&str]| needle.is_empty() || haystacks.iter().any(|h| h.to_lowercase().contains(&needle));
        match &self.data {
            StorageData::Kv(rows) => rows
                .iter()
                .enumerate()
                .filter(|(_, row)| test(&[&row.key, &row.value]))
                .map(|(index, _)| index)
                .collect(),
            StorageData::Idb(rows) => rows
                .iter()
                .enumerate()
                .filter(|(_, db)| {
                    let names: Vec<&str> = std::iter::once(db.name.as_str())
                        .chain(db.stores.iter().map(|store| store.name.as_str()))
                        .collect();
                    test(&names)
                })
                .map(|(index, _)| index)
                .collect(),
            StorageData::Cookies(rows) => rows
                .iter()
                .enumerate()
                .filter(|(_, row)| test(&[&row.name, &row.value, &row.host]))
                .map(|(index, _)| index)
                .collect(),
            StorageData::Cache(rows) => rows
                .iter()
                .enumerate()
                .filter(|(_, row)| test(&[&row.url]))
                .map(|(index, _)| index)
                .collect(),
        }
    }

    pub fn toggle_selected(&mut self, key: &str) {
        if !self.selection.remove(key) {
            self.selection.insert(key.to_owned());
        }
    }

    /// Deletes the selection locally and queues the intent. Local rows go
    /// immediately so the panel feels instant; storage confirms by
    /// re-enumerating.
    pub fn delete_selection(&mut self) -> usize {
        let keys: Vec<String> = self.selection.iter().cloned().collect();
        let count = keys.len();
        match &mut self.data {
            StorageData::Kv(rows) => rows.retain(|row| !self.selection.contains(&row.key)),
            StorageData::Idb(rows) => rows.retain(|db| !self.selection.contains(&db.name)),
            StorageData::Cookies(rows) => rows
                .retain(|row| !self.selection.contains(&format!("{}@{}", row.name, row.host))),
            StorageData::Cache(rows) => rows.retain(|row| !self.selection.contains(&row.url)),
        }
        if count > 0 {
            self.outbox.push(StorageIntent::Delete { kind: self.kind, keys });
        }
        self.selection.clear();
        count
    }

    /// Wipes the current kind. Confirmations are the caller's problem;
    /// this model does not ask twice for the same thing.
    pub fn clear(&mut self) {
        self.outbox.push(StorageIntent::Clear(self.kind));
        self.selection.clear();
    }

    pub fn export_har(&mut self) {
        self.outbox.push(StorageIntent::ExportHar);
    }

    pub fn drain_intents(&mut self) -> Vec<StorageIntent> {
        std::mem::take(&mut self.outbox)
    }

    pub fn to_json(&self) -> Value {
        let (rows, total_bytes): (Vec<Value>, u64) = match &self.data {
            StorageData::Kv(rows) => (
                rows.iter().map(|row| json!({ "key": row.key, "value": row.value, "size": row.size })).collect(),
                rows.iter().map(|row| row.size as u64).sum(),
            ),
            StorageData::Idb(rows) => (
                rows.iter()
                    .map(|db| {
                        json!({
                            "name": db.name,
                            "stores": db.stores.iter()
                                .map(|store| json!({ "name": store.name, "count": store.count }))
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect(),
                0,
            ),
            StorageData::Cookies(rows) => (
                rows.iter()
                    .map(|row| {
                        json!({
                            "name": row.name, "host": row.host, "expiry": row.expiry_ms,
                            "secure": row.is_secure, "sameSite": row.same_site.key(),
                        })
                    })
                    .collect(),
                0,
            ),
            StorageData::Cache(rows) => (
                rows.iter().map(|row| json!({ "url": row.url, "bytes": row.bytes })).collect(),
                rows.iter().map(|row| row.bytes).sum(),
            ),
        };
        json!({ "kind": self.kind.key(), "rows": rows, "bytes": total_bytes })
    }
}
