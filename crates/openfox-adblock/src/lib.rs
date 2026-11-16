pub mod cosmetic;
pub mod engine;
pub mod lists;
pub mod resources;
pub mod rules;
pub mod scriptlets;

pub use cosmetic::CosmeticEngine;
pub use engine::{Decision, Engine, Totals};
pub use rules::{ListKind, NetworkRule, ResourceType};

/// Phase 1 matcher semantics, stated once: `||` anchors by domain label,
/// `*` and `^` are boundary wildcards matched as ordered literals,
/// `|` anchors prefix/suffix. The compiled-regex matcher with separator
/// classes is a phase 2 item; this approximation covers the hosts-style
/// rules that make up the bulk of shipping lists.
#[derive(Debug, thiserror::Error)]
pub enum AdblockError {
    #[error("rule {0:?} has no parseable content")]
    EmptyRule(String),
    #[error("unsupported option {0} in rule {1:?}")]
    UnsupportedOption(String, String),
}
