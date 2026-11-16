//! The manager: every download the browser knows about, its state, its
//! progress, and the intents the UI can issue. The manager never touches
//! sockets — a byte source is handed to it (see `resume`).

use crate::{queue::DownloadQueue, resume::ResumePolicy, DownloadError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum DownloadState {
    Queued,
    Active,
    Paused,
    Done { bytes: u64 },
    Failed { reason: String },
    Canceled,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub target_path: PathBuf,
    pub state: DownloadState,
    pub total: Option<u64>,
    pub received: u64,
    pub started_at_ms: u64,
    pub mime: Option<String>,
    pub open_when_done: bool,
    /// Bump on every chunk so the UI can show honest speed, not a poll.
    pub last_chunk_ms: u64,
}

/// What the platform crate executes when the user asks "show me the file".
#[derive(Debug, Clone)]
pub enum Intent {
    Reveal { path: PathBuf },
    Open { path: PathBuf },
}

pub struct DownloadManager {
    items: Arc<Mutex<HashMap<u64, DownloadItem>>>,
    queue: DownloadQueue,
    events: broadcast::Sender<DownloadItem>,
    next_id: u64,
    resume: ResumePolicy,
}

impl DownloadManager {
    pub fn new() -> Self {
        let (events, _) = broadcast::channel(256);
        DownloadManager {
            items: Arc::new(Mutex::new(HashMap::new())),
            queue: DownloadQueue::new(4),
            events,
            next_id: 1,
            resume: ResumePolicy::default(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DownloadItem> {
        self.events.subscribe()
    }

    pub async fn list(&self) -> Vec<DownloadItem> {
        let items = self.items.lock().await;
        let mut list: Vec<DownloadItem> = items.values().cloned().collect();
        // Newest first — the download the user just started is the one
        // they are looking for.
        list.sort_by(|a, b| b.id.cmp(&a.id));
        list
    }

    pub async fn start(
        &mut self,
        url: &str,
        filename: &str,
        target_dir: PathBuf,
        total: Option<u64>,
        mime: Option<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let target_path = target_dir.join(filename);
        let item = DownloadItem {
            id,
            url: url.to_owned(),
            filename: filename.to_owned(),
            target_path,
            state: DownloadState::Queued,
            total,
            received: 0,
            started_at_ms: now_ms(),
            mime,
            open_when_done: false,
            last_chunk_ms: now_ms(),
        };
        self.queue.enqueue(id);
        self.items.lock().await.insert(id, item.clone());
        let _ = self.events.send(item);
        id
    }

    /// Byte source hands progress to the manager; the manager decides
    /// what the UI sees. A paused or canceled item stops accepting bytes.
    pub async fn on_chunk(&self, id: u64, bytes: u64) -> Result<bool, DownloadError> {
        let mut items = self.items.lock().await;
        let item = items.get_mut(&id).ok_or(DownloadError::Unknown(id))?;
        match item.state {
            DownloadState::Active | DownloadState::Queued => {
                item.received += bytes;
                item.last_chunk_ms = now_ms();
                let still_active = true;
                let _ = self.events.send(item.clone());
                Ok(still_active)
            }
            _ => Ok(false),
        }
    }

    pub async fn finish(&self, id: u64) -> Result<Option<Intent>, DownloadError> {
        let mut items = self.items.lock().await;
        let item = items.get_mut(&id).ok_or(DownloadError::Unknown(id))?;
        item.state = DownloadState::Done { bytes: item.received };
        let _ = self.events.send(item.clone());
        if item.open_when_done {
            return Ok(Some(Intent::Open {
                path: item.target_path.clone(),
            }));
        }
        Ok(None)
    }

    pub async fn pause(&self, id: u64) -> Result<(), DownloadError> {
        let mut items = self.items.lock().await;
        let item = items.get_mut(&id).ok_or(DownloadError::Unknown(id))?;
        if matches!(item.state, DownloadState::Active | DownloadState::Queued) {
            item.state = DownloadState::Paused;
            let _ = self.events.send(item.clone());
        }
        Ok(())
    }

    pub async fn resume(&self, id: u64) -> Result<(), DownloadError> {
        let mut items = self.items.lock().await;
        let item = items.get_mut(&id).ok_or(DownloadError::Unknown(id))?;
        if matches!(item.state, DownloadState::Paused) {
            item.state = DownloadState::Queued;
            let _ = self.events.send(item.clone());
        }
        Ok(())
    }

    pub async fn cancel(&self, id: u64) -> Result<(), DownloadError> {
        let mut items = self.items.lock().await;
        let item = items.get_mut(&id).ok_or(DownloadError::Unknown(id))?;
        item.state = DownloadState::Canceled;
        let _ = self.events.send(item.clone());
        Ok(())
    }

    pub fn resume_policy(&self) -> &ResumePolicy {
        &self.resume
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
