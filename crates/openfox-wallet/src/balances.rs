//! Balances: the JSON-RPC *shapes* an EVM node or Solana RPC expects,
//! with every byte of network I/O parked behind a trait. The wallet
//! builds `eth_getBalance` and `getBalance` requests as pure data and
//! formats what comes back — the app layer owns the socket, the
//! endpoint list and the privacy policy that goes with them. A wallet
//! that cannot phone home by construction is the only wallet that can
//! promise it never will.

use crate::accounts::{Account, Coin};
use crate::WalletError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A JSON-RPC 2.0 request as data: method, params, id. Ready to be
/// POSTed by whoever owns an HTTP client — this crate never opens one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: &'static str,
    pub method: String,
    pub params: Value,
    pub id: u64,
}

impl RpcRequest {
    /// The request body as a JSON value, for transports that want the
    /// whole envelope (HTTP) rather than the wire bytes (some stdio
    /// proxies hand methods and params separately).
    pub fn body(&self) -> Value {
        json!({
            "jsonrpc": self.jsonrpc,
            "method": self.method,
            "params": self.params,
            "id": self.id,
        })
    }
}

/// `eth_getBalance` for an EIP-55 address at the latest block. The
/// result is wei as a decimal string — JSON-RPC big numbers are hex
/// strings by spec, and the conversion is the caller's transport detail.
pub fn eth_get_balance_request(address: &str, id: u64) -> RpcRequest {
    RpcRequest {
        jsonrpc: "2.0",
        method: "eth_getBalance".into(),
        params: json!([address, "latest"]),
        id,
    }
}

/// Solana `getBalance` for a base58 address, at the finalized commitment
/// level — the balance a spendable transaction actually sees.
pub fn sol_get_balance_request(address: &str, id: u64) -> RpcRequest {
    RpcRequest {
        jsonrpc: "2.0",
        method: "getBalance".into(),
        params: json!([address, { "commitment": "finalized" }]),
        id,
    }
}

/// Where balances come from. Implementations are expected to run the
/// request shapes above against an endpoint and return the raw units —
/// wei as a decimal string, lamports as the RPC's u64 — so this trait
/// stays sync, side-effect-free from the crate's point of view, and
/// trivially fakeable in tests.
pub trait BalanceSource {
    /// Wei balance of an EIP-55 address as a decimal string.
    fn eth_get_balance(&self, address: &str) -> Result<String, WalletError>;

    /// Lamports balance of a base58 Solana address.
    fn sol_get_balance(&self, address: &str) -> Result<u64, WalletError>;
}

/// Human-readable balances over any [`BalanceSource`]. One struct, two
/// formatters — the unit math is string arithmetic because the numbers
/// overflow every native integer and a bigint dependency for a
/// decimal-point insertion is not a dependency worth having.
pub struct Balances<'a> {
    source: &'a dyn BalanceSource,
}

impl<'a> Balances<'a> {
    pub fn new(source: &'a dyn BalanceSource) -> Self {
        Balances { source }
    }

    /// ETH with 18 decimals, trailing zeros trimmed.
    pub fn eth_human(&self, account: &Account) -> Result<String, WalletError> {
        let wei = self.source.eth_get_balance(&account.address)?;
        Ok(format_units(&wei, 18))
    }

    /// SOL with 9 decimals, trailing zeros trimmed.
    pub fn sol_human(&self, account: &Account) -> Result<String, WalletError> {
        let lamports = self.source.sol_get_balance(&account.address)?;
        Ok(format_units(&lamports.to_string(), 9))
    }

    /// Dispatch on the account's coin.
    pub fn human(&self, account: &Account) -> Result<String, WalletError> {
        match account.coin {
            Coin::Ethereum => self.eth_human(account),
            Coin::Solana => self.sol_human(account),
        }
    }
}

/// Render `units` (a non-negative decimal string) divided by 10^decimals
/// as a fixed-point decimal string. Malformed input becomes zero — a
/// balance display that panics over a rogue RPC response is worse than
/// one that shows 0.0 and lets the caller wonder why.
pub fn format_units(units: &str, decimals: u8) -> String {
    if !units.bytes().all(|b| b.is_ascii_digit()) || units.is_empty() {
        return format!("0.{}", "0".repeat(decimals as usize));
    }
    let padded_len = units.len().max(decimals as usize + 1);
    let padded = format!("{:0>width$}", units, width = padded_len);
    let split = padded_len - decimals as usize;
    let (whole, frac) = padded.split_at(split);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        whole.to_owned()
    } else {
        format!("{whole}.{frac}")
    }
}
