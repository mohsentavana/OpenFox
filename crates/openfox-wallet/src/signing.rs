//! Message signing: `personal_sign` for EVM and raw-message ed25519 for
//! Solana, each with its verify twin. Signatures are produced from key
//! material handed in by the keyring caller — this module never stores a
//! key, it only binds one to a message.
//!
//! The EVM scheme is the EIP-191 personal-sign construction: the
//! `\x19Ethereum Signed Message:\n<len>` prefix is the domain separator
//! that stops a wallet signature from being replayed as a transaction,
//! and the recovery id lets a verifier reconstruct the signer's address
//! from the signature alone — the property every dApp login in the wild
//! depends on.

use crate::WalletError;
use ed25519_dalek::{Signature as EdSignature, Signer, SigningKey, Verifier, VerifyingKey};
use secp256k1::{ecdsa::RecoverableSignature, Message, Secp256k1, SecretKey};
use sha3::{Digest, Keccak256};

/// The EIP-191 personal-sign prefix, as bytes, before the length.
const EIP191_PREFIX: &[u8] = b"\x19Ethereum Signed Message:\n";

/// Hash a message the way `personal_sign` does: prefix + ASCII length +
/// message, keccak256. Exposed because the same hash feeds both signing
/// and recovery, and the two must never disagree about the recipe.
pub fn personal_message_hash(message: &[u8]) -> [u8; 32] {
    let mut prefixed = Vec::with_capacity(EIP191_PREFIX.len() + 10 + message.len());
    prefixed.extend_from_slice(EIP191_PREFIX);
    prefixed.extend_from_slice(message.len().to_string().as_bytes());
    prefixed.extend_from_slice(message);
    Keccak256::digest(&prefixed).into()
}

/// Sign a message with an EVM key (32 raw secret bytes): returns the
/// 65-byte `r || s || v` form with v = 27 + recovery id, as dApp
/// sign-in flows expect. Deterministic RFC-6979 nonce — no RNG on the
/// signing path, so a weak RNG can never leak the key.
pub fn sign_evm_message(secret: &[u8; 32], message: &[u8]) -> Result<[u8; 65], WalletError> {
    let ctx = Secp256k1::new();
    let key = SecretKey::from_slice(secret).map_err(|_| WalletError::BadSeed)?;
    let digest = personal_message_hash(message);
    let recoverable = ctx
        .sign_ecdsa_recoverable(&Message::from_digest(digest), &key);
    let (recovery, compact) = recoverable.serialize_compact();
    let mut out = [0u8; 65];
    out[..64].copy_from_slice(&compact);
    // EIP-191 personal_sign convention: 27/28, not 0/1. Wallets and
    // verifiers have shipped both; signers choose 27/28 because that is
    // what the first dApps hardcoded.
    out[64] = 27 + recovery.to_i32() as u8;
    Ok(out)
}

/// Verify an EVM `personal_sign` signature against the address it claims
/// to come from: recover the signer from (hash, r, s, v), re-derive the
/// EIP-55 address, compare case-insensitively (the checksum casing is
/// for humans; verification must not fail on a lowercase paste). Both
/// v-conventions (27/28 and 0/1, plus EIP-155's 35/36 style) are read.
pub fn verify_evm_message(address: &str, message: &[u8], signature: &[u8; 65]) -> bool {
    let recovery_id = match signature[64] {
        v @ 0..=1 => v,
        v @ 27..=28 => v - 27,
        v @ 35..=36 => v - 35,
        _ => return false,
    };
    let Ok(recovery) = secp256k1::ecdsa::RecoveryId::from_i32(recovery_id as i32) else {
        return false;
    };
    let mut compact = [0u8; 64];
    compact.copy_from_slice(&signature[..64]);
    let Ok(recoverable) = RecoverableSignature::from_compact(&compact, recovery) else {
        return false;
    };
    let ctx = Secp256k1::new();
    let digest = personal_message_hash(message);
    let Ok(pubkey) = ctx.recover_ecdsa(&Message::from_digest(digest), &recoverable) else {
        return false;
    };
    match crate::accounts::eip55_address(&pubkey) {
        Ok(recovered) => recovered.eq_ignore_ascii_case(address),
        Err(_) => false,
    }
}

/// Sign a message with a Solana key (32 raw secret bytes): ed25519 over
/// the raw bytes, no prefix — Solana's signMessage has no domain
/// separator, so the message itself must carry any intent wording.
pub fn sign_solana_message(secret: &[u8; 32], message: &[u8]) -> [u8; 64] {
    SigningKey::from_bytes(secret).sign(message).to_bytes()
}

/// Verify a Solana message signature against the 32-byte public key.
pub fn verify_solana_message(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> bool {
    let Ok(verifying) = VerifyingKey::from_bytes(public_key) else {
        return false;
    };
    verifying
        .verify(message, &EdSignature::from_bytes(signature))
        .is_ok()
}
