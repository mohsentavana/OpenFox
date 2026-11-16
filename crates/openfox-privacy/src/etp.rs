//! Enhanced Tracking Protection: the policy layer that turns classifier
//! verdicts into allow/block decisions. Classification says what a
//! request is; this module decides what we do about it, per site.

use crate::classify::{self, TrackerCategory};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// How aggressive the shields run, before per-site exceptions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EtpLevel {
    /// Blocks third-party advertising, analytics and social trackers.
    /// The web keeps working; the surveillance middlemen do not.
    Standard,
    /// Blocks all five tracker categories, third-party. Sites that break
    /// were monetizing the breakage.
    Strict,
    /// The user picked the list themselves. `blocked` is what to block;
    /// `third_party_only` limits the blocking to cross-site requests
    /// when set, and lifts the first-party exemption when not.
    Custom {
        blocked: HashSet<TrackerCategory>,
        third_party_only: bool,
    },
}

impl Default for EtpLevel {
    /// Standard is the default: it breaks the tracker economy without
    /// breaking the logins and embeds people actually came for.
    fn default() -> Self {
        EtpLevel::Standard
    }
}

/// The tracking-protection policy for a profile: one global level plus
/// the per-site exceptions the user granted or revoked themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EtpPolicy {
    /// The level everywhere except where `site_exceptions` says else.
    pub level: EtpLevel,
    /// Site → level. Keys are registrable domains; an entry here always
    /// beats the global level for that site.
    pub site_exceptions: HashMap<String, EtpLevel>,
}

impl EtpPolicy {
    /// A policy at `level` with a clean exception table.
    pub fn new(level: EtpLevel) -> Self {
        EtpPolicy { level, site_exceptions: HashMap::new() }
    }

    /// Set (or overwrite) the exception for one site.
    pub fn set_exception(&mut self, site: &str, level: EtpLevel) {
        self.site_exceptions.insert(site.to_owned(), level);
    }

    /// Whether a classified request is allowed on `site`. A site
    /// exception always wins over the global level — the user's word
    /// beats ours. First-party is always allowed, except under a Custom
    /// level that explicitly asked for more.
    pub fn allows(&self, site: &str, decision: &classify::Decision) -> bool {
        let level = self.site_exceptions.get(site).unwrap_or(&self.level);
        match level {
            EtpLevel::Standard => {
                !(decision.third_party && is_standard_blocked(decision.category))
            }
            EtpLevel::Strict => {
                !(decision.third_party && decision.category != TrackerCategory::Content)
            }
            EtpLevel::Custom { blocked, third_party_only } => {
                let applies = !*third_party_only || decision.third_party;
                !(applies && blocked.contains(&decision.category))
            }
        }
    }
}

/// The three categories Standard blocks: the bulk of the tracker economy,
/// chosen so that logins, embeds and payments keep working.
fn is_standard_blocked(category: TrackerCategory) -> bool {
    matches!(
        category,
        TrackerCategory::Advertising | TrackerCategory::Analytics | TrackerCategory::Social
    )
}
