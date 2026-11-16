//! Consent dialogs: the industry's favorite dark pattern, answered
//! before the user ever sees it. This crate never touches the DOM — the
//! engine reports that a dialog was seen and executes the verdict we
//! hand back. All the state we keep is which hosts were handled.

use std::collections::HashMap;

/// Selectors matching the consent-management platforms we know about.
/// The engine walks the DOM with these; we only remember what happened
/// where.
pub const CMP_SELECTORS: &[&str] = &[
    ".fc-consent-root",
    "#onetrust-banner-sdk",
    "[data-testid='cookie-dialog']",
    ".cmp-wrapper",
    "#cmpwrapper",
    ".cookie-banner",
    "#CybotCookiebotDialog",
];

/// What to do when a consent dialog appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentPolicy {
    /// Find the reject button and press it. Consent is not the default
    /// state of anything.
    AutoReject,
    /// Close the dialog without answering. No answer is not yes.
    AutoDismiss,
    /// Hands off: the user deals with it themselves.
    Ignore,
}

/// The verdict for one dialog sighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentVerdict {
    /// Do this to the dialog — "reject" or "dismiss". The engine knows
    /// which button that is; this crate never touches the DOM.
    Act(&'static str),
    /// Nothing to do: no dialog seen, or the policy says hands off.
    Skip,
    /// Already answered on this host; the dialog is a re-render.
    AlreadyHandled,
}

/// Remembers which hosts have been handled so the same dialog is not
/// fought twice per visit. One instance lives next to the engine's
/// per-site state.
#[derive(Debug, Clone)]
pub struct ConsentGate {
    policy: ConsentPolicy,
    /// Host → how many dialog sightings have been recorded there.
    handled: HashMap<String, u64>,
}

impl ConsentGate {
    /// A gate running `policy`.
    pub fn new(policy: ConsentPolicy) -> Self {
        ConsentGate { policy, handled: HashMap::new() }
    }

    /// `dialog_seen` is the engine's word that one of `CMP_SELECTORS`
    /// matched on `host`. Under AutoReject we answer "reject", under
    /// AutoDismiss we answer "dismiss", and either way the host is
    /// remembered so re-renders are ignored instead of re-fought.
    pub fn verdict(&mut self, host: &str, dialog_seen: bool) -> ConsentVerdict {
        let action = match self.policy {
            ConsentPolicy::Ignore => return ConsentVerdict::Skip,
            ConsentPolicy::AutoReject => "reject",
            ConsentPolicy::AutoDismiss => "dismiss",
        };
        if !dialog_seen {
            return ConsentVerdict::Skip;
        }
        if let Some(sightings) = self.handled.get_mut(host) {
            *sightings += 1;
            return ConsentVerdict::AlreadyHandled;
        }
        self.handled.insert(host.to_owned(), 1);
        ConsentVerdict::Act(action)
    }

    /// Drop the memory for `host` — a fresh top-level navigation or a
    /// cleared site deserves a fresh answer.
    pub fn forget(&mut self, host: &str) {
        self.handled.remove(host);
    }

    /// How many dialog sightings this gate has recorded for `host`.
    pub fn handle_count(&self, host: &str) -> u64 {
        self.handled.get(host).copied().unwrap_or(0)
    }
}

impl Default for ConsentGate {
    /// AutoReject by default: consent was never the default state, so
    /// refusing it is.
    fn default() -> Self {
        ConsentGate::new(ConsentPolicy::AutoReject)
    }
}
