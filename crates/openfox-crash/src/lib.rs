//! Crash handling: minidumps on crash, recovery on boot. The browser is
//! designed crash-only — a clean shutdown is just a crash we scheduled.
//! Sessions stay valid through the worst of it.

pub mod minidump;
pub mod recover;
pub mod reporter;

pub use minidump::{StreamType, MINIDUMP_MAGIC};
pub use recover::recover_session;
pub use reporter::{CrashReport, CrashReporter, UploadPolicy};

#[derive(Debug, thiserror::Error)]
pub enum CrashError {
    #[error("minidump format: {0}")]
    Format(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("report queue corrupted")]
    QueueCorrupt,
}
