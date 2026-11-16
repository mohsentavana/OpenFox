//! Passwords sealed at rest with AES-256-GCM under an Argon2id key derived
//! from the master password. Breach checks are k-anonymous by SHA-1 range.
//! The vault never leaves the process unsealed.
//!
//! The pipeline, end to end:
//!
//! 1. [`form`] classifies fields and finds login forms the way Firefox's
//!    form manager does.
//! 2. [`autofill`] turns a detected form into a capture decision at submit
//!    and into a [`FillPlan`] — field indices, never secrets — at fill
//!    time.
//! 3. [`store`] persists sealed records and lets plaintext back out only
//!    through [`PasswordStore::reveal`], under an unlocked [`VaultCrypto`].
//! 4. [`breach`] checks passwords against the HIBP range protocol: five
//!    hex characters cross the network, the match happens locally.
//! 5. [`crypto`] is the floor under all of it — Argon2id on the master
//!    password, AES-256-GCM per record, key zeroed on drop.
//!
//! Threat model: an attacker holding the vault files has ciphertext, a
//! salt, and a verifier — everything except the master password. An
//! attacker on the wire learns a hash prefix shared by millions of
//! passwords. An attacker inside the process is out of scope; no password
//! manager survives that one.

pub mod autofill;
pub mod breach;
pub mod crypto;
pub mod form;
pub mod store;

use thiserror::Error;

/// Everything the vault can refuse to do.
#[derive(Debug, Error)]
pub enum PasswordError {
    /// Sealing or unsealing failed: KDF rejection or a GCM tag mismatch on
    /// anything that is not the verifier.
    #[error("seal or unseal failed: {0}")]
    SealUnseal(String),

    /// The verifier seal did not open under the derived key.
    #[error("wrong master password")]
    BadMasterPassword,

    /// An operation needing the unlocked key ran against a locked vault.
    #[error("the vault is locked")]
    NotUnlocked,

    /// Vault file IO, including a file that is not the JSON it should be.
    #[error("vault io: {0}")]
    Io(#[from] std::io::Error),

    /// The form moved between detection and fill; stale indices would
    /// write secrets into the wrong fields.
    #[error("form detection failed: {0}")]
    FormDetection(String),

    /// The breach range lookup failed or was handed a malformed prefix.
    #[error("breach lookup failed: {0}")]
    BreachLookup(String),

    /// Plaintext export is refused by policy.
    #[error("plaintext export is disabled")]
    ExportDisabled,
}

pub use autofill::{AutofillDriver, CaptureOutcome, FillPlan, FillState};
pub use breach::{BreachMonitor, BreachStatus, RangeResponse};
pub use crypto::{Sealed, VaultCrypto};
pub use form::{FieldDescriptor, FieldKind, LoginFormDetector, LoginCandidate};
pub use store::{PasswordRecord, PasswordStore};
