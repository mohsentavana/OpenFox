use std::time::Instant;

/// Stable handle for a tab. Ids are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum LoadState {
    Idle,
    Loading,
    Loaded { status: u16, bytes: u64 },
    Failed { code: u16 },
}

impl LoadState {
    pub fn is_loading(&self) -> bool {
        matches!(self, LoadState::Loading)
    }
}

/// The numbers behind the shield badge. Counts things that actually happened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShieldCounters {
    pub ads_blocked: u32,
    pub trackers_blocked: u32,
    pub requests_saved: u32,
    pub bytes_saved: u64,
}

impl ShieldCounters {
    pub fn total_blocked(&self) -> u32 {
        self.ads_blocked + self.trackers_blocked
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
}

/// One tab: a bounded history stack and a cursor into it. Forward entries
/// survive a Back, exactly like the browsers users know.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub entries: Vec<HistoryEntry>,
    pub cursor: usize,
    pub state: LoadState,
    pub shield: ShieldCounters,
    pub loaded_at: Option<Instant>,
}

impl Tab {
    pub fn new(id: TabId, url: Option<String>) -> Self {
        let entries = url
            .map(|u| {
                vec![HistoryEntry { url: u, title: String::new() }]
            })
            .unwrap_or_default();
        Tab {
            id,
            cursor: entries.len().saturating_sub(1),
            entries,
            state: LoadState::Idle,
            shield: ShieldCounters::default(),
            loaded_at: None,
        }
    }

    pub fn current(&self) -> Option<&HistoryEntry> {
        self.entries.get(self.cursor)
    }

    pub fn url(&self) -> Option<&str> {
        self.current().map(|e| e.url.as_str())
    }

    /// Pushes a new entry, truncating any forward entries. Returns true when
    /// the URL differs from the current one; a duplicate push is a reload,
    /// not a history entry.
    pub fn push(&mut self, url: String, title: String) -> bool {
        if self.current().is_some_and(|e| e.url == url) {
            return false;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(HistoryEntry { url, title });
        self.cursor = self.entries.len() - 1;
        true
    }

    pub fn set_title(&mut self, title: String) {
        if let Some(entry) = self.entries.get_mut(self.cursor) {
            entry.title = title;
        }
    }

    pub fn back(&mut self) -> Option<&HistoryEntry> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        self.entries.get(self.cursor)
    }

    pub fn forward(&mut self) -> Option<&HistoryEntry> {
        self.cursor += 1;
        self.entries.get(self.cursor)
    }
}

/// Owning collection of tabs. The active tab is an index, kept valid on
/// every mutation — close, open, select.
#[derive(Debug, Default)]
pub struct TabManager {
    tabs: Vec<Tab>,
    active: Option<usize>,
    next_id: u64,
}

impl TabManager {
    pub fn new() -> Self {
        TabManager { tabs: Vec::new(), active: None, next_id: 1 }
    }

    pub fn open(&mut self, url: Option<String>) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.push(Tab::new(id, url));
        self.active = Some(self.tabs.len() - 1);
        id
    }

    pub fn close(&mut self, id: TabId) -> bool {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
            return false;
        };
        self.tabs.remove(idx);
        match self.active {
            Some(a) if a == idx => {
                self.active = if self.tabs.is_empty() {
                    None
                } else {
                    Some(a.min(self.tabs.len() - 1))
                };
            }
            Some(a) if a > idx => self.active = Some(a - 1),
            _ => {}
        }
        true
    }

    pub fn select(&mut self, id: TabId) -> bool {
        match self.tabs.iter().position(|t| t.id == id) {
            Some(idx) => {
                self.active = Some(idx);
                true
            }
            None => false,
        }
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        match self.active {
            Some(i) => self.tabs.get_mut(i),
            None => None,
        }
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}
