//! The keyring: one BIP-39 seed phrase unlocked into two master trees —
//! BIP-32 secp256k1 for EVM chains and SLIP-0010 ed25519 for Solana —
//! and nothing else. The seed itself is consumed at unlock and forgotten;
//! what stays in memory is the master key/chaincode pair of each tree,
//! zeroed on drop by hand (the house rule from the password vault: no
//! `zeroize` dependency for 32 bytes of xor).
//!
//! Derivation is hand-rolled HMAC-SHA512 rather than a BIP-32 crate on
//! purpose: the two curves need two standards (BIP-32 for secp256k1 with
//! hardened and normal children, SLIP-0010 for ed25519 with hardened-only
//! children), and both are ~60 lines each — smaller than the surface of
//! any crate that wraps them, and auditable in one read.

use crate::accounts::{Account, Coin};
use crate::WalletError;
use bip39::Mnemonic;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256, Sha512};

type HmacSha512 = Hmac<Sha512>;

/// BIP-39 passphrases may be empty; the seed differs either way, and the
/// caller decides which one they meant.
pub const EMPTY_PASSPHRASE: &str = "";

/// One BIP-32 node: secret key bytes plus chain code. Secrets are kept as
/// raw bytes (not library types) so this file owns their lifetime — and
/// their zeroing.
struct SecpNode {
    key: [u8; 32],
    chaincode: [u8; 32],
}

impl Drop for SecpNode {
    fn drop(&mut self) {
        self.key.fill(0);
        self.chaincode.fill(0);
    }
}

/// One SLIP-0010 ed25519 node: same shape, different arithmetic.
struct EdNode {
    key: [u8; 32],
    chaincode: [u8; 32],
}

impl Drop for EdNode {
    fn drop(&mut self) {
        self.key.fill(0);
        self.chaincode.fill(0);
    }
}

/// An unlocked wallet keyring. Send between threads freely — the secrets
/// are plain bytes with no library state attached — but never print it:
/// `Debug` is deliberately not implemented.
pub struct Keyring {
    secp: SecpNode,
    ed: EdNode,
    /// BIP-32 fingerprint of the secp master key: the first four bytes of
    /// ripemd160(sha256(master pubkey)). It identifies the keyring in UIs
    /// without revealing anything derivable about it.
    fingerprint: [u8; 4],
}

impl Keyring {
    /// Unlock a keyring from a BIP-39 phrase. The phrase is validated
    /// (wordlist membership and checksum) by the `bip39` crate, converted
    /// to its 64-byte seed once, and then dropped from memory — the two
    /// master trees are all the keyring keeps.
    pub fn from_mnemonic(phrase: &str, passphrase: &str) -> Result<Self, WalletError> {
        let mnemonic = Mnemonic::parse_in(bip39::Language::English, phrase)?;
        let mut seed = mnemonic.to_seed(passphrase);
        let keyring = Self::from_seed(&seed)?;
        // Zero the seed before it leaves scope: after this line the only
        // copies of the key material are the two master nodes, and both
        // of those zero themselves on drop.
        seed.fill(0);
        Ok(keyring)
    }

    /// Create a fresh keyring: 256 bits of OS entropy, 24 words, returned
    /// exactly once for the user to write down. The phrase is not kept —
    /// a keyring that can silently re-display its own seed is a keyring
    /// that logs it eventually.
    pub fn create(passphrase: &str) -> Result<(Self, String), WalletError> {
        let mut entropy = [0u8; 32];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut entropy);
        let mnemonic = Mnemonic::from_entropy(&entropy)?;
        let phrase = mnemonic.to_string();
        let keyring = Self::from_mnemonic(&phrase, passphrase)?;
        Ok((keyring, phrase))
    }

    fn from_seed(seed: &[u8; 64]) -> Result<Self, WalletError> {
        // BIP-32 master: HMAC-SHA512 under the fixed "Bitcoin seed" key.
        let secp_i = hmac_pair(b"Bitcoin seed", seed);
        SecretKey::from_slice(&secp_i.0).map_err(|_| WalletError::BadSeed)?;
        let secp = SecpNode { key: secp_i.0, chaincode: secp_i.1 };

        // SLIP-0010 ed25519 master: same construction, its own key string.
        let ed_i = hmac_pair(b"ed25519 seed", seed);
        let ed = EdNode { key: ed_i.0, chaincode: ed_i.1 };

        let fingerprint = master_fingerprint(&secp.key)?;
        Ok(Keyring { secp, ed, fingerprint })
    }

    /// Derive account `index` on `coin` down its standard path.
    /// `index` is the account index, not a raw BIP-32 child number — the
    /// path template belongs to the coin, and callers should never be
    /// assembling derivation paths by hand.
    pub fn derive(&self, coin: Coin, index: u32) -> Result<Account, WalletError> {
        match coin {
            Coin::Ethereum => {
                // m/44'/60'/0'/0/index — purpose, coin, account, change.
                let node = self
                    .secp
                    .derive(0x8000_002C)?
                    .derive(0x8000_003C)?
                    .derive(0x8000_0000)?
                    .derive(0)?
                    .derive(index)?;
                let secret =
                    SecretKey::from_slice(&node.key).map_err(|_| WalletError::BadSeed)?;
                Account::ethereum(&secret, index)
            }
            Coin::Solana => {
                // m/44'/501'/0'/index' — ed25519 children are always
                // hardened, per SLIP-0010.
                let node = self
                    .ed
                    .derive(0x8000_002C)?
                    .derive(0x8000_01F5)?
                    .derive(0x8000_0000)?
                    .derive(index | 0x8000_0000)?;
                let signing = SigningKey::from_bytes(&node.key);
                Account::solana(&signing, index)
            }
        }
    }

    /// The BIP-32 master fingerprint as eight hex characters — the "which
    /// keyring is this" label in settings screens.
    pub fn fingerprint(&self) -> String {
        self.fingerprint.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl SecpNode {
    /// BIP-32 CKDpriv for one child index. Hardened children hash the
    /// parent secret; normal children hash the parent public key so
    /// public-side derivation stays possible. Either way the child secret
    /// is (parent + IL) mod n and the child chain code is IR.
    fn derive(&self, index: u32) -> Result<SecpNode, WalletError> {
        let ctx = Secp256k1::new();
        let secret =
            SecretKey::from_slice(&self.key).map_err(|_| WalletError::BadSeed)?;
        let mut data = Vec::with_capacity(37);
        if index >= 0x8000_0000 {
            data.push(0x00);
            data.extend_from_slice(&secret.secret_bytes());
        } else {
            data.extend_from_slice(&PublicKey::from_secret_key(&ctx, &secret).serialize());
        }
        data.extend_from_slice(&index.to_be_bytes());

        let (il, ir) = hmac_pair(&self.chaincode, &data);
        let child = ck_add(&secret, &il)?;
        Ok(SecpNode { key: child.secret_bytes(), chaincode: ir })
    }
}

impl EdNode {
    /// SLIP-0010 CKDpriv for ed25519: hardened-only, and the child key is
    /// IL itself (ed25519 addition would break SLIP-0010 compatibility),
    /// IR the chain code.
    fn derive(&self, index: u32) -> Result<EdNode, WalletError> {
        let mut data = Vec::with_capacity(37);
        data.push(0x00);
        data.extend_from_slice(&self.key);
        data.extend_from_slice(&index.to_be_bytes());
        let (il, ir) = hmac_pair(&self.chaincode, &data);
        Ok(EdNode { key: il, chaincode: ir })
    }
}

/// HMAC-SHA512 split into its (left = key material, right = chain code)
/// halves — the one primitive every BIP-32/SLIP-0010 step is made of.
fn hmac_pair(key: &[u8], data: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha512::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    left.copy_from_slice(&out[..32]);
    right.copy_from_slice(&out[32..]);
    (left, right)
}

/// (parent + tweak) mod n, with BIP-32's two failure modes — tweak ≥ n
/// and sum = 0 — both reported as the same error so callers simply move
/// on to the next index, exactly what the spec prescribes.
fn ck_add(parent: &SecretKey, tweak: &[u8; 32]) -> Result<SecretKey, WalletError> {
    let scalar = secp256k1::Scalar::from_be_bytes(*tweak)
        .map_err(|_| WalletError::InvalidChild)?;
    parent
        .add_tweak(&scalar)
        .map_err(|_| WalletError::InvalidChild)
}

/// The BIP-32 master fingerprint: ripemd160(sha256(compressed master
/// pubkey))[0..4]. RIPEMD-160 earns its place in the dependency list
/// here and only here — it is the specified construction, and wallets
/// that show a different fingerprint are showing a different key.
fn master_fingerprint(master_key: &[u8; 32]) -> Result<[u8; 4], WalletError> {
    let ctx = Secp256k1::new();
    let secret = SecretKey::from_slice(master_key).map_err(|_| WalletError::BadSeed)?;
    let pubkey = PublicKey::from_secret_key(&ctx, &secret).serialize();
    let sha = Sha256::digest(&pubkey);
    let ripe = ripemd::Ripemd160::digest(sha);
    let mut out = [0u8; 4];
    out.copy_from_slice(&ripe[..4]);
    Ok(out)
}