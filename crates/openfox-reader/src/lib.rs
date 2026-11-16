//! Reader mode: readability extraction, typography, per-site memory.
//! The page minus the chrome, the ads, and the noise.

pub mod extract;
pub mod mode;
pub mod typography;

pub use extract::{extract, Article, ExtractNode};
pub use mode::{evaluate, ModeState, ReaderMode, SiteMemory};
pub use typography::{LayoutParams, ReaderFont, ReaderTheme, ReaderTypography};

/// Reader-mode failures are storage failures: extraction itself reports
/// "no article" as a value, because a page that will not extract is a
/// normal page, not an error condition.
#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    #[error("reader memory io: {0}")]
    Io(#[from] std::io::Error),
    #[error("reader memory json: {0}")]
    Json(#[from] serde_json::Error),
}
