//! Accounts: the public face of a derived key. An account is a coin, a
//! derivation index, an address string in the coin's native encoding, and
//! the raw public key for verification — never the private key, which
//! exists only inside `keyring` and `signing` calls.
//!
//! Address encodings are done by hand on purpose. EIP-55 checksumming is
//! keccak over the lowercase hex of the address — six lines — and base58
//! is one division loop; neither justifies a dependency, and owning them
//! keeps the crate's trust surface to the two curve libraries plus the
//! hash functions.

use crate::keyring::Keyring;
use crate::WalletError;
use ed25519_dalek::SigningKey;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// The coins this wallet speaks. Two is a feature: every chain added is
/// a promise about derivation paths, address encodings and signing
/// schemes, and the list stays honest by staying short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Coin {
    Ethereum,
    Solana,
}

impl Coin {
    pub fn name(self) -> &'static str {
        match self {
            Coin::Ethereum => "ethereum",
            Coin::Solana => "solana",
        }
    }

    /// The derivation path template for account `index`, with hardened
    /// levels marked `h` — for display; the keyring owns the real path.
    pub fn derivation_path(self, index: u32) -> String {
        match self {
            Coin::Ethereum => format!("m/44'/60'/0'/0/{index}"),
            Coin::Solana => format!("m/44'/501'/0'/{index}'"),
        }
    }
}

/// Static coin surface, for settings pages and the MCP tool: what this
/// build supports, without touching any keyring to say so.
pub const SUPPORTED_COINS: &[Coin] = &[Coin::Ethereum, Coin::Solana];

/// One derived account. `public_key` is the compressed 33-byte secp256k1
/// key or the raw 32-byte ed25519 key — the two encodings dApps verify
/// against — and `address` is its user-facing form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub coin: Coin,
    pub index: u32,
    pub address: String,
    pub public_key: Vec<u8>,
}

impl Account {
    pub(crate) fn ethereum(secret: &SecretKey, index: u32) -> Result<Self, WalletError> {
        let ctx = Secp256k1::new();
        let pubkey = PublicKey::from_secret_key(&ctx, secret);
        let address = eip55_address(&pubkey)?;
        Ok(Account {
            coin: Coin::Ethereum,
            index,
            address,
            public_key: pubkey.serialize().to_vec(),
        })
    }

    pub(crate) fn solana(signing: &SigningKey, index: u32) -> Result<Self, WalletError> {
        let verifying = signing.verifying_key();
        let address = base58_encode(&verifying.to_bytes());
        Ok(Account {
            coin: Coin::Solana,
            index,
            address,
            public_key: verifying.to_bytes().to_vec(),
        })
    }

    /// The `0x`-prefixed EIP-55 form, or None for coins that do not use
    /// one — a small adapter so callers can render uniformly.
    pub fn hex_address(&self) -> Option<&str> {
        match self.coin {
            Coin::Ethereum => Some(self.address.as_str()),
            Coin::Solana => None,
        }
    }
}

/// The EIP-55 checksummed address of a secp256k1 public key: keccak256
/// over the lowercase hex of the 20 address bytes, uppercasing each hex
/// digit whose corresponding hash nibble is ≥ 8. A checksum in the
/// casing — worth the six lines because it kills whole classes of
/// typo-and-clipboard-hijack transfers.
pub fn eip55_address(pubkey: &PublicKey) -> Result<String, WalletError> {
    let uncompressed = pubkey.serialize_uncompressed();
    // Without the 0x04 prefix: X || Y.
    let digest = Keccak256::digest(&uncompressed[1..]);
    let body = &digest[12..];
    let lower: String = body.iter().map(|b| format!("{b:02x}")).collect();
    let checksum = Keccak256::digest(lower.as_bytes());

    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, ch) in lower.chars().enumerate() {
        let nibble = if i % 2 == 0 {
            checksum[i / 2] >> 4
        } else {
            checksum[i / 2] & 0x0F
        };
        if nibble >= 8 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

/// Base58, the Bitcoin alphabet. Solana addresses are the raw base58 of
/// the 32-byte public key with no checksum — the chain's own choice, and
/// the reason `base58check` below is a separate function rather than
/// baked in.
pub fn base58_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 58] =
        b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        let mut carry = byte as u32;
        for digit in digits.iter_mut() {
            carry += (*digit as u32) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    // Leading zero bytes encode as leading '1's.
    let ones = bytes.iter().take_while(|&&b| b == 0).count();
    let mut out = String::with_capacity(ones + digits.len());
    for _ in 0..ones {
        out.push('1');
    }
    for digit in digits.iter().rev() {
        out.push(ALPHABET[*digit as usize] as char);
    }
    out
}

/// Base58Check: base58 of payload + first four bytes of the double
/// SHA-256 of the payload. The Bitcoin construction, provided for chains
/// that use it; Solana does not, and this crate will not pretend it does.
pub fn base58check_encode(payload: &[u8]) -> String {
    use sha2::Sha256;
    let round1 = Sha256::digest(payload);
    let round2 = Sha256::digest(round1);
    let mut checked = payload.to_vec();
    checked.extend_from_slice(&round2[..4]);
    base58_encode(&checked)
}

/// Convenience for callers holding a keyring: accounts `0..count` on one
/// coin. Wallet UIs show a page of addresses; that is exactly this loop.
pub fn first_accounts(keyring: &Keyring, coin: Coin, count: u32) -> Vec<Account> {
    (0..count)
        .filter_map(|index| keyring.derive(coin, index).ok())
        .collect()
}
