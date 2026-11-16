//! The update flow: check, download, verify, stage, swap at next boot.
//! The swap is two renames — anything else is an outage waiting for a
//! power cut.

use crate::signature::{Signature, SigningKeys};
use crate::UpdateError;
use ed25519_dalek::VerifyingKey;
use semver::Version;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateChannel::Stable => "stable",
            UpdateChannel::Beta => "beta",
            UpdateChannel::Nightly => "nightly",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeltaRef {
    pub from_version: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub channel: String,
    pub notes: String,
    pub artifact_sha256: String,
    pub signature: Signature,
    pub deltas: Vec<DeltaRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    Downloading { version: String, progress: f32 },
    Staged { version: String },
    Failed { reason: String },
    UpToDate,
}

pub struct Updater {
    channel: UpdateChannel,
    current: Version,
    state: UpdateState,
    staging_dir: PathBuf,
    pinned_build_key: Option<VerifyingKey>,
    pinned_release_key: Option<VerifyingKey>,
}

impl Updater {
    pub fn new(channel: UpdateChannel, current: Version, staging_dir: PathBuf) -> Self {
        Updater {
            channel,
            current,
            state: UpdateState::Idle,
            staging_dir,
            pinned_build_key: None,
            pinned_release_key: None,
        }
    }

    pub fn pin_keys(&mut self, build: VerifyingKey, release: VerifyingKey) {
        // Pinned at build time; there is no key rotation over the wire.
        self.pinned_build_key = Some(build);
        self.pinned_release_key = Some(release);
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    /// Decide whether a manifest is an upgrade worth staging.
    pub fn evaluate(&mut self, manifest: &UpdateManifest) -> Result<bool, UpdateError> {
        self.state = UpdateState::Checking;
        let offered = Version::parse(&manifest.version)?;
        if offered <= self.current {
            self.state = UpdateState::UpToDate;
            return Ok(false);
        }
        if manifest.channel != self.channel.as_str() {
            self.state = UpdateState::UpToDate;
            return Ok(false);
        }
        Ok(true)
    }

    /// Verify a downloaded artifact against the manifest and pinned keys.
    /// An unsigned or wrongly-signed update never reaches the disk as a
    /// staged candidate.
    pub fn verify_artifact(
        &mut self,
        manifest: &UpdateManifest,
        artifact: &[u8],
    ) -> Result<(), UpdateError> {
        let digest = hex_sha256(artifact);
        if digest != manifest.artifact_sha256 {
            self.state = UpdateState::Failed {
                reason: "sha256 mismatch".into(),
            };
            return Err(UpdateError::BadSignature);
        }
        let (Some(build), Some(release)) = (self.pinned_build_key, self.pinned_release_key)
        else {
            self.state = UpdateState::Failed {
                reason: "no pinned keys".into(),
            };
            return Err(UpdateError::BadSignature);
        };
        SigningKeys::verify(&manifest.signature, artifact, &build, &release)?;
        Ok(())
    }

    /// Stage the artifact: write to staging, write the swap script that
    /// the next boot executes — current becomes .old, staged becomes
    /// current. If the boot after an update finds .old and current is
    /// healthy, .old is deleted; if current is broken, .old is restored.
    pub fn stage(&mut self, version: &str, artifact: &[u8]) -> Result<PathBuf, UpdateError> {
        std::fs::create_dir_all(&self.staging_dir)?;
        let staged = self.staging_dir.join("openfox-next.bin");
        std::fs::write(&staged, artifact)?;
        let marker = self.staging_dir.join("swap.json");
        let plan = serde_json::json!({
            "version": version,
            "staged": staged.display().to_string(),
            "action": "atomic-swap-on-boot",
        });
        std::fs::write(&marker, serde_json::to_string_pretty(&plan).map_err(|e| UpdateError::Manifest(e.to_string()))?)?;
        self.state = UpdateState::Staged {
            version: version.to_owned(),
        };
        Ok(staged)
    }

    /// The boot-time swap, in three moves, idempotent under crash.
    pub fn apply_pending_swap(current_bin: &Path, staged: &Path) -> Result<(), UpdateError> {
        let old = current_bin.with_extension("old");
        if staged.exists() {
            if current_bin.exists() && !old.exists() {
                std::fs::rename(current_bin, &old)?;
            }
            std::fs::rename(staged, current_bin)?;
        }
        Ok(())
    }

    /// Rollback pointer: if the current binary cannot prove it booted
    /// (crash loop guard), the .old sibling comes back.
    pub fn rollback_if_unhealthy(current_bin: &Path) -> Result<bool, UpdateError> {
        let old = current_bin.with_extension("old");
        let health = current_bin.with_extension("healthy");
        if old.exists() && !health.exists() {
            std::fs::rename(current_bin, current_bin.with_extension("broken"))?;
            std::fs::rename(&old, current_bin)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn mark_healthy(current_bin: &Path) -> Result<(), UpdateError> {
        let old = current_bin.with_extension("old");
        let health = current_bin.with_extension("healthy");
        std::fs::write(&health, b"ok")?;
        if old.exists() {
            std::fs::remove_file(&old)?;
        }
        Ok(())
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
