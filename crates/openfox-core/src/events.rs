//! The event bus: one broadcast channel, every crate listens, nobody
//! polls. The UI reduces events into chrome state; devtools mirrors
//! them into panels; MCP reflects them to agents.

use crate::tab::{ShieldCounters, TabId};
use tokio::sync::broadcast;

/// Every observable browser happening. The UI's reducer matches this
/// exhaustively — adding a variant is a compile error there, on purpose.
#[derive(Debug, Clone, serde::Serialize)]
pub enum BrowserEvent {
    TabOpened { tab: TabId },
    TabClosed { tab: TabId },
    NavigationStarted { tab: TabId, url: String },
    NavigationCompleted {
        tab: TabId,
        url: String,
        status: u16,
        elapsed_ms: u64,
        blocked: u32,
    },
    NavigationFailed { tab: TabId, url: String, code: u16 },
    TitleChanged { tab: TabId, title: String },
    ShieldUpdated { tab: TabId, counters: ShieldCounters },
    Console {
        tab: TabId,
        level: u8,
        message: String,
    },
    NetworkRequest {
        tab: TabId,
        url: String,
        kind: NetworkKind,
        blocked: bool,
    },
    DownloadProgress {
        id: u64,
        received: u64,
        total: Option<u64>,
    },
    ZoomChanged { tab: TabId, level: f32 },
    ReaderAvailabilityChanged { tab: TabId, available: bool },
    FaviconChanged { tab: TabId, url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum NetworkKind {
    Document,
    Script,
    Stylesheet,
    Image,
    Font,
    Media,
    Xhr,
    Other,
}

pub struct EventBus {
    sender: broadcast::Sender<BrowserEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

impl EventBus {
    /// The cap exists so a slow listener lags and drops, never stalls
    /// the publisher. A stuck panel must not stall a page load.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(16));
        EventBus { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BrowserEvent> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: BrowserEvent) {
        // A send error means zero listeners — the common case in
        // headless mode, and not an error.
        let _ = self.sender.send(event);
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}
