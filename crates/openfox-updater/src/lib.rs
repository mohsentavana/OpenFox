//! The updater: signed manifests, delta patches, atomic swaps. An
//! update that cannot prove its signature never runs — and a failed
//! update rolls back on the next boot without ceremony.

pub mod delta;
pub mod signature;
pub mod update;

pub use delta::{apply, build_delta, Delta, Op};
pub use signature::{Signature, SigningKeys};
pub use update::{UpdateChannel, UpdateManifest, UpdateState, Updater};

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("signature verification failed")]
    BadSignature,
    #[error("manifest parse: {0}")]
    Manifest(String),
    #[error("delta apply: {0}")]
    Delta(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("semver: {0}")]
    Semver(#[from] semver::Error),
}
