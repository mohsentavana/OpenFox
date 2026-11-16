//! The wallet: a BIP-39 keyring that derives EVM and Solana accounts,
//! signs messages without ever holding network state, hands balances to
//! a caller-owned source, and shows dApps only the accounts their origin
//! was granted. Brave Wallet's feature surface minus the parts that
//! conflict with this browser's charter: no token-price tickers, no
//! "watched" dApp suggestions, no telemetry about what anyone signed.
//!
//! The split is deliberate. `keyring` owns secrets and derivation;
//! `accounts` owns the public shapes; `balances` owns JSON-RPC request
//! construction and unit math but no sockets; `permissions` owns the
//! origin-to-account grant table; `signing` owns message binding. Each
//! file is one concern, and the secret material crosses exactly one of
//! those boundaries — from `keyring` into a `signing` call.

pub mod accounts;
pub mod balances;
pub mod keyring;
pub mod permissions;
pub mod signing;

pub use accounts::{Account, Coin, SUPPORTED_COINS};
pub use balances::{
    eth_get_balance_request, format_units, sol_get_balance_request, BalanceSource, Balances,
    RpcRequest,
};
pub use keyring::{Keyring, EMPTY_PASSPHRASE};
pub use permissions::{DappGrant, DappPermissions};

/// Everything the wallet can fail at. Missing here on purpose: network
/// errors. The wallet does no networking; the balance source behind
/// [`balances::BalanceSource`] reports its own failures as
/// [`WalletError::Balance`], and that is as far as the wallet's interest
/// in sockets goes.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    /// The phrase failed BIP-39 validation — wrong word count, a word
    /// outside the English list, or a checksum mismatch.
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(#[from] bip39::Error),

    /// The seed produced key material a curve rejected. This is not
    /// recoverable by re-deriving; the phrase is wrong for this curve's
    /// master construction.
    #[error("seed does not produce a valid master key")]
    BadSeed,

    /// BIP-32 child derivation hit an invalid child (tweak ≥ curve order
    /// or a zero child key, probability ~2^-127 per step). The caller
    /// moves to the next index; the spec's answer and ours.
    #[error("invalid child at this index")]
    InvalidChild,

    /// A balance source answered with something that is not a balance.
    #[error("balance source: {0}")]
    Balance(String),
}
