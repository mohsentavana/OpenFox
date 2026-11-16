//! Downloads: queue, progress, resume. A download is a small state
//! machine with a file on disk — everything here exists to keep that
//! state honest.

pub mod manager;
pub mod queue;
pub mod resume;

pub use manager::{DownloadItem, DownloadManager, DownloadState};
pub use queue::{DownloadQueue, RateLimiter};
pub use resume::ResumePolicy;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download {0} not found")]
    Unknown(u64),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid target path")]
    BadPath,
    #[error("server does not support ranges; restart required")]
    NoRangeSupport,
}
