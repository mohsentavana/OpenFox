//! The vault: sealed records matched by origin, persisted as a bare JSON
//! array. The Argon2 salt and verifier live in the embedding app's vault
//! header (see [`crate::crypto::VaultCrypto::new_vault`]); this file never
//! holds key material in any form.

use crate::breach::sha1_hex;
use crate::crypto::{Sealed, VaultCrypto};
use crate::PasswordError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

/// One saved login. The username sits in the clear — it is what the user
/// sees in the doorhanger and what matches a form; the password is sealed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordRecord {
    pub id: String,
    pub origin: String,
    pub username: String,
    pub sealed: Sealed,
    pub created_at: u64,
    pub updated_at: u64,
    pub times_used: u32,
}

/// Sealed records plus where they persist; `path: None` is session-only.
pub struct PasswordStore {
    records: Vec<PasswordRecord>,
    path: Option<PathBuf>,
}

impl PasswordStore {
    /// A vault that never touches disk — session-only passwords.
    pub fn new_in_memory() -> Self {
        PasswordStore { records: Vec::new(), path: None }
    }

    /// Load the vault from `path`, or create it empty on first run. The
    /// file is a JSON array of sealed records; anything else on disk is an
    /// error, not a silent reset.
    pub async fn new_on_disk(path: PathBuf) -> Result<Self, PasswordError> {
        match tokio::fs::read_to_string(&path).await {
            Ok(text) => {
                let records = serde_json::from_str::<Vec<PasswordRecord>>(&text)
                    .map_err(|e| PasswordError::Io(std::io::Error::other(e)))?;
                Ok(PasswordStore { records, path: Some(path) })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let store = PasswordStore { records: Vec::new(), path: Some(path) };
                store.save().await?;
                Ok(store)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Persist; an in-memory vault persists nothing by design.
    pub async fn save(&self) -> Result<(), PasswordError> {
        let Some(path) = &self.path else { return Ok(()) };
        let text = serde_json::to_string_pretty(&self.records)
            .map_err(|e| PasswordError::Io(std::io::Error::other(e)))?;
        tokio::fs::write(path, text).await?;
        Ok(())
    }

    /// Records whose origin matches exactly — the same shape the cookie
    /// jar keys on.
    pub fn find_by_origin(&self, origin: &str) -> Vec<&PasswordRecord> {
        self.records.iter().filter(|r| r.origin == origin).collect()
    }

    /// Write side of [`crate::autofill::CaptureOutcome`]: insert a login,
    /// or replace the password of the existing (origin, username) pair.
    pub fn upsert(&mut self, origin: &str, username: &str, sealed: Sealed) -> bool {
        let now = now_secs();
        let existing = self
            .records
            .iter_mut()
            .find(|r| r.origin == origin && r.username == username);
        if let Some(record) = existing {
            record.sealed = sealed;
            record.updated_at = now;
            true
        } else {
            self.records.push(PasswordRecord {
                id: sha1_hex(format!("{origin}|{username}|{now}").as_bytes()),
                origin: origin.to_owned(),
                username: username.to_owned(),
                sealed,
                created_at: now,
                updated_at: now,
                times_used: 0,
            });
            false
        }
    }

    /// Resolve a record to plaintext at fill time. `crypto: None` is a
    /// locked vault, and a locked vault fills nothing. This is the only
    /// path by which a stored password comes back out.
    pub fn reveal(
        &self,
        id: &str,
        crypto: Option<&VaultCrypto>,
    ) -> Result<Option<String>, PasswordError> {
        let Some(crypto) = crypto else { return Err(PasswordError::NotUnlocked) };
        let Some(record) = self.records.iter().find(|r| r.id == id) else { return Ok(None) };
        let bytes = crypto.open(&record.sealed)?;
        String::from_utf8(bytes).map(Some).map_err(|_| PasswordError::SealUnseal("not utf-8".into()))
    }

    /// Count a use — when the engine fills a login, not when the
    /// doorhanger merely offers it.
    pub fn mark_used(&mut self, id: &str) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| r.id == id) else { return false };
        record.times_used += 1;
        record.updated_at = now_secs();
        true
    }

    /// Delete by id; returns whether anything went.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.records.len();
        self.records.retain(|r| r.id != id);
        before != self.records.len()
    }

    /// Import Firefox-format CSV (`url,username,password` per row).
    /// Plaintext passwords ride through this function by necessity — CSV
    /// import is a user-initiated, user-visible action, never a background
    /// one. Each password is sealed on arrival; the plaintext dies at
    /// return. Returns the number of rows sealed.
    pub fn import_csv(&mut self, csv: &str, crypto: &VaultCrypto) -> Result<usize, PasswordError> {
        let mut imported = 0;
        for (n, line) in csv.lines().enumerate() {
            let fields = csv_fields(line);
            if fields.len() != 3 || (n == 0 && fields[0].eq_ignore_ascii_case("url")) {
                continue; // header row, short row, or blank — nothing to seal
            }
            let (url, username, password) = (&fields[0], &fields[1], &fields[2]);
            let Ok(parsed) = Url::parse(url) else { continue };
            if password.is_empty() || !matches!(parsed.scheme(), "http" | "https") {
                continue;
            }
            let sealed = crypto.seal(password.as_bytes())?;
            self.upsert(&parsed.origin().ascii_serialization(), username, sealed);
            imported += 1;
        }
        Ok(imported)
    }

    /// Deliberately disabled. The export path exists so the gate is a
    /// real, auditable decision point — one place carrying the reasoning —
    /// instead of a missing feature nobody can find. Plaintext export will
    /// want re-authentication and an OS file picker before it ever ships;
    /// until then the only answer is no.
    pub fn export_csv(&self) -> Result<String, PasswordError> {
        Err(PasswordError::ExportDisabled)
    }
}

/// Minimal CSV splitter: quote wrapping and commas inside quotes — the
/// whole of Firefox's export shape. The fewer lines of parser, the fewer
/// parser bugs.
fn csv_fields(line: &str) -> Vec<String> {
    let (mut fields, mut current, mut quoted) = (Vec::new(), String::new(), false);
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
