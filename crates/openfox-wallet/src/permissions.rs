//! dApp permissions: which origins may see which accounts. The model is
//! the cookie jar's — per-site by construction, default deny, revocable
//! from a list — because a dApp's grant to read your addresses is
//! exactly as sensitive as its cookies, and deserves exactly the same
//! shape of control.
//!
//! Keys are origins (scheme + host + port, the EIP-1193 definition), not
//! registrable domains: `https://app.example.com` and
//! `https://evil.example.com` are different origins even though they
//! share a registrable domain, and a wallet that conflates them is a
//! phisher's helper. Subdomains get no inheritance for free.

use crate::accounts::{Account, Coin};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What a grant stores: who, when, and until when. No grant row is ever
/// secret; the list is the settings UI's "connected sites" page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DappGrant {
    pub account_coin: Coin,
    pub account_index: u32,
    pub granted_at_ms: u64,
    /// Unix ms expiry; None means until revoked. Session-scoped grants
    /// (the "connect for this visit" answer) carry an expiry so a closed
    /// tab never leaves a standing authorization behind.
    pub expires_at_ms: Option<u64>,
}

type Key = (String, Coin, u32);

/// The grant table. One instance per profile, keyed by
/// (origin, coin, account index).
#[derive(Debug, Default)]
pub struct DappPermissions {
    grants: HashMap<Key, DappGrant>,
}

impl DappPermissions {
    pub fn new() -> Self {
        DappPermissions::default()
    }

    /// Grant `origin` access to `account`. Pass `ttl_ms` for
    /// session-scoped grants; None persists until revoked.
    pub fn authorize(
        &mut self,
        origin: &str,
        account: &Account,
        now_ms: u64,
        ttl_ms: Option<u64>,
    ) {
        self.grants.insert(
            (origin.to_owned(), account.coin, account.index),
            DappGrant {
                account_coin: account.coin,
                account_index: account.index,
                granted_at_ms: now_ms,
                expires_at_ms: ttl_ms.map(|ttl| now_ms + ttl),
            },
        );
    }

    /// May `origin` see `account` right now? Expired grants answer false
    /// and are dropped as a side effect — an expiry that lingers in the
    /// table after firing is a bug in waiting.
    pub fn check(&mut self, origin: &str, account: &Account, now_ms: u64) -> bool {
        let key = (origin.to_owned(), account.coin, account.index);
        match self.grants.get(&key) {
            Some(grant) => match grant.expires_at_ms {
                Some(expiry) if expiry <= now_ms => {
                    self.grants.remove(&key);
                    false
                }
                _ => true,
            },
            None => false,
        }
    }

    /// Revoke one origin's access to one account.
    pub fn revoke(&mut self, origin: &str, account: &Account) {
        self.grants
            .remove(&(origin.to_owned(), account.coin, account.index));
    }

    /// Revoke everything for an origin — the "disconnect" button.
    pub fn revoke_origin(&mut self, origin: &str) {
        self.grants.retain(|(o, _, _), _| o != origin);
    }

    /// The accounts `origin` may currently see, in grant order.
    pub fn accounts_for(&self, origin: &str, now_ms: u64) -> Vec<(Coin, u32)> {
        let mut rows: Vec<_> = self
            .grants
            .iter()
            .filter(|((o, _, _), _)| o == origin)
            .filter(|(_, grant)| grant.expires_at_ms.map_or(true, |e| e > now_ms))
            .map(|((_, coin, index), _)| (*coin, *index))
            .collect();
        rows.sort_unstable();
        rows
    }

    /// Every live grant, for the exceptions editor: origin, coin, index,
    /// granted-at, expiry.
    pub fn list(&self, now_ms: u64) -> Vec<(String, DappGrant)> {
        self.grants
            .iter()
            .filter(|(_, grant)| grant.expires_at_ms.map_or(true, |e| e > now_ms))
            .map(|((origin, _, _), grant)| (origin.clone(), grant.clone()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.grants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }
}
