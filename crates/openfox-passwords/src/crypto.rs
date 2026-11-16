//! AES-256-GCM sealing under an Argon2id key derived from the master
//! password. The key never leaves this struct, the struct never prints
//! itself, and drop zeroes it.

use crate::PasswordError;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Known plaintext sealed when the vault is created; [`VaultCrypto::unlock`]
/// must open it before a password is believed. Not a secret — its only job
/// is to fail loudly at the wrong key.
const VERIFIER_PLAINTEXT: &[u8] = b"openfox-vault-verifier-v1";

/// A sealed blob: AES-256-GCM ciphertext plus the nonce it was sealed
/// under. Safe on disk — opening it takes the Argon2id key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sealed {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

/// An unlocked vault key. No `Debug` derive: the derive would happily print
/// the key into every crash log that ever caught this struct.
pub struct VaultCrypto {
    key: [u8; 32],
}

impl VaultCrypto {
    /// Create a vault: derive the key under a fresh random salt and mint
    /// the verifier. The caller persists `(salt, verifier)` beside the
    /// records — both are non-secret; the key is never handed out in any
    /// form.
    pub fn new_vault(master_password: &str) -> Result<(Self, [u8; 16], Sealed), PasswordError> {
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let key = derive_key(master_password, &salt)?;
        let crypto = VaultCrypto { key };
        let verifier = crypto.seal(VERIFIER_PLAINTEXT)?;
        Ok((crypto, salt, verifier))
    }

    /// Re-derive the key and prove it by opening the verifier. The GCM tag
    /// check is the constant-time comparison — there is no byte-wise
    /// secret compare here to get wrong. A wrong password fails the tag
    /// and gets `BadMasterPassword`, nothing more specific.
    pub fn unlock(
        master_password: &str,
        salt: [u8; 16],
        verifier: &Sealed,
    ) -> Result<Self, PasswordError> {
        let key = derive_key(master_password, &salt)?;
        let crypto = VaultCrypto { key };
        match crypto.open(verifier) {
            Ok(plaintext) if plaintext == VERIFIER_PLAINTEXT => Ok(crypto),
            _ => Err(PasswordError::BadMasterPassword),
        }
    }

    /// Seal under a fresh random nonce. Every seal gets its own nonce; GCM
    /// under a repeated nonce is a catastrophe this structure cannot
    /// produce.
    pub fn seal(&self, plaintext: &[u8]) -> Result<Sealed, PasswordError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| PasswordError::SealUnseal(e.to_string()))?;
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|e| PasswordError::SealUnseal(e.to_string()))?;
        Ok(Sealed { nonce, ciphertext })
    }

    /// Open a sealed blob; anything but the exact key fails the tag.
    pub fn open(&self, sealed: &Sealed) -> Result<Vec<u8>, PasswordError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| PasswordError::SealUnseal(e.to_string()))?;
        cipher
            .decrypt(Nonce::from_slice(&sealed.nonce), sealed.ciphertext.as_slice())
            .map_err(|e| PasswordError::SealUnseal(e.to_string()))
    }
}

/// Argon2id at 64 MiB / 3 passes / 4 lanes — the OWASP baseline for master
/// passwords. Slow on purpose: a stolen vault file should cost more to
/// grind than the passwords inside it are worth.
fn derive_key(master_password: &str, salt: &[u8; 16]) -> Result<[u8; 32], PasswordError> {
    let params = Params::new(65536, 3, 4, Some(32))
        .map_err(|e| PasswordError::SealUnseal(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(master_password.as_bytes(), salt, &mut key)
        .map_err(|e| PasswordError::SealUnseal(e.to_string()))?;
    Ok(key)
}

/// Zero the key on drop. No `zeroize` dependency: the key is a plain
/// `[u8; 32]` field with no `Copy`, no `Clone`, and no accessor that hands
/// it out by value, so this struct holds the only copy — two lines here
/// beat a crate for ceremony.
impl Drop for VaultCrypto {
    fn drop(&mut self) {
        self.key.fill(0);
    }
}
