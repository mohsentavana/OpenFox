//! minisign-style signatures: ed25519 over the sha256 digest, two-key
//! policy — the build key signs, the release key countersigns. Both must
//! verify. One compromised key is a incident; two is an impossibility
//! budget.

use crate::UpdateError;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub struct SigningKeys {
    build: SigningKey,
    release: SigningKey,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Signature {
    /// base64 of the ed25519 signature over sha256(artifact).
    pub build_sig: String,
    /// The countersignature over sha256(build_sig bytes).
    pub release_sig: String,
}

impl SigningKeys {
    pub fn generate() -> Self {
        let build = SigningKey::generate(&mut rand_and_trust());
        let release = SigningKey::generate(&mut rand_and_trust());
        SigningKeys { build, release }
    }

    pub fn build_public(&self) -> [u8; 32] {
        self.build.verifying_key().to_bytes()
    }

    pub fn release_public(&self) -> [u8; 32] {
        self.release.verifying_key().to_bytes()
    }

    /// Sign an artifact with both keys, release countersigning the build
    /// signature itself.
    pub fn sign(&self, artifact: &[u8]) -> Signature {
        let digest = Sha256::digest(artifact);
        let build_sig = self.build.sign(&digest);
        let release_sig = self.release.sign(&build_sig.to_bytes());
        Signature {
            build_sig: base64_encode(&build_sig.to_bytes()),
            release_sig: base64_encode(&release_sig.to_bytes()),
        }
    }

    /// Verify both signatures against pinned public keys. The pinned keys
    /// ship in the binary; there is no TOFU, no freshness argument.
    pub fn verify(
        signature: &Signature,
        artifact: &[u8],
        build_public: &VerifyingKey,
        release_public: &VerifyingKey,
    ) -> Result<(), UpdateError> {
        let digest = Sha256::digest(artifact);
        let build_bytes: [u8; 64] = base64_decode(&signature.build_sig)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(UpdateError::BadSignature)?;
        let build_sig = ed25519_dalek::Signature::from_bytes(&build_bytes);
        build_public
            .verify(&digest, &build_sig)
            .map_err(|_| UpdateError::BadSignature)?;

        let release_bytes: [u8; 64] = base64_decode(&signature.release_sig)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(UpdateError::BadSignature)?;
        let release_sig = ed25519_dalek::Signature::from_bytes(&release_bytes);
        release_public
            .verify(&build_sig.to_bytes(), &release_sig)
            .map_err(|_| UpdateError::BadSignature)?;
        Ok(())
    }
}

fn rand_and_trust() -> impl rand_core::CryptoRngCore {
    // OsRng is the only honest choice for key generation.
    rand_core::OsRng
}

/// base64, standard alphabet, padding. In-house because the 40 lines are
/// cheaper than the dependency and this file is already the trust root.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let value = ALPHABET.iter().position(|&a| a == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}
