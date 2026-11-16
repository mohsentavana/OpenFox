//! Per-tab reader-mode state and the per-site memory that makes reader
//! mode feel remembered rather than re-negotiated on every page.

use crate::extract::Article;
use crate::ReaderError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Below this much extractable text the page is chrome or an app; offering
/// reader mode there produces an empty pane and a confused user.
const MIN_TEXT_CHARS: usize = 700;

/// What the reader button shows for the tab.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum ModeState {
    /// No article-shaped content — the button hides.
    #[default]
    Unavailable,
    /// Article found; the button offers itself, with text mass as score.
    Available { score: u32 },
    /// Reader is on; the UI shows title and minutes instead of the page.
    Active { title: String, minutes: u32 },
}

/// Availability from a (possibly failed) extraction. 700 characters is
/// roughly two paragraphs — the minimum that reads as an article.
pub fn evaluate(article: Option<&Article>) -> ModeState {
    match article {
        Some(a) if a.text_chars >= MIN_TEXT_CHARS => ModeState::Available { score: a.text_chars as u32 },
        _ => ModeState::Unavailable,
    }
}

/// Per-site auto-enable memory: registrable domain to "reader was left on
/// here", persisted as JSON at a caller-chosen path.
#[derive(Debug, Default)]
pub struct SiteMemory {
    sites: HashMap<String, bool>,
    path: Option<PathBuf>,
}

impl SiteMemory {
    /// Detached memory: tracks preferences but persists nowhere — for
    /// private windows, where remembering is the one thing not allowed.
    pub fn new() -> Self {
        SiteMemory { sites: HashMap::new(), path: None }
    }

    /// Loads memory from `path`. A missing file is fresh memory, not an
    /// error — first run must behave like every other run.
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, ReaderError> {
        let path = path.as_ref();
        let sites = match tokio::fs::read_to_string(path).await {
            Ok(raw) => serde_json::from_str(&raw)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(SiteMemory { sites, path: Some(path.to_owned()) })
    }

    /// Writes memory back to its path; detached memory saves as a no-op.
    pub async fn save(&self) -> Result<(), ReaderError> {
        if let Some(path) = &self.path {
            tokio::fs::write(path, serde_json::to_string_pretty(&self.sites)?).await?;
        }
        Ok(())
    }

    /// Whether the user left reader on at this site. Unknown sites are
    /// off: auto-enabling where nobody asked is the behavior users hate.
    pub fn prefers_reader(&self, site: &str) -> bool {
        self.sites.get(site).copied().unwrap_or(false)
    }

    pub fn set(&mut self, site: &str, on: bool) {
        self.sites.insert(site.to_owned(), on);
    }
}

/// What the last extraction carried, kept so a toggle can rebuild either
/// side of ModeState without re-running extraction.
#[derive(Debug)]
struct LastArticle {
    title: String,
    minutes: u32,
    score: u32,
}

/// Reader-mode controller, one per tab.
#[derive(Debug, Default)]
pub struct ReaderMode {
    /// Current state of the tab's reader button.
    pub state: ModeState,
    site: Option<String>,
    last: Option<LastArticle>,
}

impl ReaderMode {
    pub fn new() -> Self {
        ReaderMode::default()
    }

    /// A navigation landed: re-evaluate against the new page and honor
    /// per-site memory before the user touches anything.
    pub fn on_navigate(&mut self, site: Option<&str>, article: Option<&Article>, memory: &SiteMemory) {
        self.site = site.map(str::to_owned);
        self.last = article.map(|a| LastArticle {
            title: a.title.clone(),
            minutes: a.est_read_minutes,
            score: a.text_chars as u32,
        });
        self.state = evaluate(article);
        if let (Some(site), Some(last)) = (&self.site, &self.last) {
            if memory.prefers_reader(site) {
                self.state = ModeState::Active { title: last.title.clone(), minutes: last.minutes };
            }
        }
    }

    /// The reader button. Toggling records the choice in site memory —
    /// leaving reader on at a domain means "and again next time".
    pub fn toggle(&mut self, memory: &mut SiteMemory) {
        let site = self.site.clone();
        match self.state.clone() {
            ModeState::Unavailable => {}
            ModeState::Available { .. } => {
                if let Some(last) = &self.last {
                    self.state = ModeState::Active { title: last.title.clone(), minutes: last.minutes };
                }
                if let Some(site) = site {
                    memory.set(&site, true);
                }
            }
            ModeState::Active { .. } => {
                // Score survives in `last`; the button goes back to offered.
                self.state = ModeState::Available { score: self.last.as_ref().map_or(0, |l| l.score) };
                if let Some(site) = site {
                    memory.set(&site, false);
                }
            }
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, ModeState::Active { .. })
    }
}
