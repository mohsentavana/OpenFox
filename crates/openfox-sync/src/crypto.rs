//! The identity and the envelope. X25519 to agree, HKDF to derive,
//! AES-256-GCM to seal. One subkey per collection so a leaked bookmark key
//! never opens a password.

use crate::SyncError;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

/// The device identity. The secret half stays on-device — on desktop it
/// lives in the OS credential store; only `public_bytes` ever persists in
/// the clear.
pub struct SyncIdentity {
    secret: StaticSecret,
    public: PublicKey,
}

impl SyncIdentity {
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        SyncIdentity { secret, public }
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let secret = StaticSecret::from(*bytes);
        let public = PublicKey::from(&secret);
        SyncIdentity { secret, public }
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }

    fn subkey(&self, remote: &PublicKey, collection: &str) -> Result<[u8; 32], SyncError> {
        let shared = self.secret.diffie_hellman(remote);
        let hk = Hkdf::<Sha256>::new(Some(b"openfox-sync"), shared.as_bytes());
        let mut key = [0u8; 32];
        hk.expand(collection.as_bytes(), &mut key)
            .map_err(|_| SyncError::Crypto("hkdf expand failed".into()))?;
        Ok(key)
    }

    /// Seal a payload for one collection. A fresh nonce every time — GCM
    /// nonce reuse is the one unforgivable sin here.
    pub fn seal(
        &self,
        remote_public: &PublicKey,
        collection: &str,
        plaintext: &[u8],
    ) -> Result<Envelope, SyncError> {
        let key = self.subkey(remote_public, collection)?;
        let cipher = Aes256Gcm::new((&key).into());
        let nonce = Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| SyncError::Crypto("seal failed".into()))?;
        Ok(Envelope {
            nonce: nonce.into(),
            ciphertext,
        })
    }

    /// Open a payload addressed to this device.
    pub fn open(
        &self,
        remote_public: &PublicKey,
        collection: &str,
        envelope: &Envelope,
    ) -> Result<Vec<u8>, SyncError> {
        let key = self.subkey(remote_public, collection)?;
        let cipher = Aes256Gcm::new((&key).into());
        cipher
            .decrypt(Nonce::from_slice(&envelope.nonce), envelope.ciphertext.as_slice())
            .map_err(|_| SyncError::Crypto("open failed: wrong key or tampered".into()))
    }
}

/// The sealed unit the server stores. Opaque by construction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Envelope {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}
