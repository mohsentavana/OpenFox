use crate::{LoadState, ShieldCounters, Tab, TabId, TabManager};
use std::path::{Path, PathBuf};

/// Crash-only session format, learned from Firefox: the file is a sequence of
/// self-contained JSON lines. A write that dies mid-line is a line that gets
/// dropped on restore. Nothing is truncated, nothing is corrupted, there is
/// no footer, no checksum ceremony.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("profile directory unusable: {0}")]
    ProfileDir(std::io::Error),
    #[error("session line {0} unparseable; skipped")]
    BadLine(usize),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TabRecord {
    id: u64,
    entries: Vec<(String, String)>,
    cursor: usize,
    state: LoadState,
    shield: ShieldCounters,
}

impl From<&Tab> for TabRecord {
    fn from(t: &Tab) -> Self {
        TabRecord {
            id: t.id.0,
            entries: t.entries.iter().map(|e| (e.url.clone(), e.title.clone())).collect(),
            cursor: t.cursor,
            state: t.state.clone(),
            shield: t.shield,
        }
    }
}

pub struct Session {
    path: PathBuf,
}

impl Session {
    pub fn new(profile_dir: &Path) -> Result<Self, SessionError> {
        std::fs::create_dir_all(profile_dir).map_err(SessionError::ProfileDir)?;
        Ok(Session { path: profile_dir.join("session.jsonl") })
    }

    /// Serializes the full tab set as JSON lines, written atomically: build
    /// the payload in memory, then one write + rename.
    pub fn save(&self, tabs: &TabManager) -> Result<(), SessionError> {
        let mut body = String::new();
        for tab in tabs.iter() {
            let record = TabRecord::from(tab);
            let line = serde_json::to_string(&record).expect("session record serializes");
            body.push_str(&line);
            body.push('\n');
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, body).map_err(SessionError::ProfileDir)?;
        std::fs::rename(&tmp, &self.path).map_err(SessionError::ProfileDir)?;
        Ok(())
    }

    /// Restores tabs, dropping any line that does not parse instead of
    /// failing the session. The last tab opened becomes active.
    pub fn restore(&self) -> Result<TabManager, SessionError> {
        let mut tabs = TabManager::new();
        let Ok(body) = std::fs::read_to_string(&self.path) else {
            return Ok(tabs);
        };
        for (i, line) in body.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(record) = serde_json::from_str::<TabRecord>(line) else {
                return Err(SessionError::BadLine(i));
            };
            let mut tab = Tab::new(TabId(record.id), None);
            tab.entries = record
                .entries
                .into_iter()
                .map(|(url, title)| crate::tab::HistoryEntry { url, title })
                .collect();
            tab.cursor = record.cursor.min(tab.entries.len().saturating_sub(1));
            tab.state = record.state;
            tab.shield = record.shield;
            // Re-open through the manager so invariants (active, next_id)
            // stay true, then patch in the restored identity.
            let _ = tabs.open(None);
            if let Some(slot) = tabs.active_mut() {
                slot.id = tab.id;
                slot.entries = tab.entries;
                slot.cursor = tab.cursor;
                slot.state = tab.state.clone();
                slot.shield = tab.shield;
            }
        }
        Ok(tabs)
    }
}
