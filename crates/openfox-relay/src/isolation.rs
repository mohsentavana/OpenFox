//! Per-tab-stream isolation: the rule that two tabs never share a
//! circuit, stated as a key.
//!
//! Why a key and not a convention: a circuit is a machine that sees
//! every byte and every timing of everything routed through it, in
//! order. Two tabs sharing one circuit are one tab as far as the guard
//! and exit can tell — their visits correlate, their sessions merge,
//! and the privacy property of a "private window" quietly becomes the
//! privacy property of a shared window. Tor's answer is circuit
//! isolation per stream; this is the same answer with a SOCKS chain in
//! place of an onion network.
//!
//! The key carries the container id (the same identity string
//! `openfox-core`'s profiles use for cookie-jar isolation) so network
//! isolation matches identity isolation exactly: work tabs and personal
//! tabs never share a circuit even in the same window, because they
//! never share a jar either. Tab ids scope it further, and the stream
//! nonce scopes it to the last mile — a fetch and a WebSocket in the
//! same tab walk different chains.

use std::fmt;

/// An opaque isolation identity. Build one per (container, tab) at
/// least; per (container, tab, stream) when the cost of a fresh circuit
/// is cheaper than the linkability of a shared one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IsolationKey(String);

impl IsolationKey {
    /// One circuit space per tab: all streams of a tab that are allowed
    /// to correlate (they share a page, after all) share the tab's
    /// circuits.
    pub fn for_tab(container: &str, tab_id: u64) -> Self {
        IsolationKey(format!("tab:{container}:{tab_id}"))
    }

    /// One circuit space per stream: nothing shares, not even sibling
    /// requests in the same tab. The right key for private windows on
    /// hostile networks, at the cost of a chain build per stream.
    pub fn for_stream(container: &str, tab_id: u64, stream_nonce: u64) -> Self {
        IsolationKey(format!("stream:{container}:{tab_id}:{stream_nonce}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IsolationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
