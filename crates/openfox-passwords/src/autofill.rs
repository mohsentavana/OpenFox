//! Capture and fill orchestration. The driver is a state machine that
//! never holds secret bytes: credentials pass through as references at
//! submit, and sealed records live in the store.

use crate::form::LoginCandidate;
use crate::store::PasswordRecord;
use crate::PasswordError;

/// What the driver decided to do with a submitted login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome<'a> {
    /// A login this origin has never seen — offer to save.
    Save { username: &'a str, password: &'a str },
    /// A known username at this origin with a new password — offer to
    /// update the stored record.
    Update { record_id: String, username: &'a str, password: &'a str },
    /// Nothing to capture: no detected form, wrong origin, empty password.
    Ignore,
}

/// The capture pipeline, one state at a time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FillState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// A login form was detected at this origin; its next submit is
    /// capture-worthy.
    Candidate { origin: String },
    /// The user is looking at the save/update doorhanger.
    Prompted,
    /// The user accepted; the record is written.
    Saved,
    /// The user said never mind.
    Discarded,
}

/// Drives capture: form seen → submit classified → prompt resolved. The
/// embedding app applies outcomes through [`crate::store::PasswordStore`].
#[derive(Debug, Default)]
pub struct AutofillDriver {
    state: FillState,
}

impl AutofillDriver {
    pub fn new() -> Self {
        AutofillDriver { state: FillState::Idle }
    }

    /// Prime capture for an origin. Signup shape deliberately changes
    /// nothing here — Firefox captures new logins from signup submits too —
    /// because capture decisions happen at submit, when the typed bytes
    /// tell the truth. The candidate mattered to detection; priming only
    /// needs the origin.
    pub fn on_form_detected(&mut self, _candidate: &LoginCandidate, origin: &str) {
        self.state = FillState::Candidate { origin: origin.to_owned() };
    }

    /// Classify a submit. Submits from origins where no login form was
    /// ever detected are phishing-shaped and ignored; so are empty
    /// passwords — Firefox never saves those either. An empty username is
    /// fine: Firefox saves password-only logins, because plenty of banks
    /// put the username on an earlier page. `existing` comes from
    /// [`crate::store::PasswordStore::find_by_origin`] — same-origin
    /// records only; cross-origin matching is how phishing pages harvest
    /// vaults. A matching username yields `Update`; the caller still
    /// unseals the stored password to compare before rewriting —
    /// identical passwords deserve no doorhanger.
    pub fn on_submit<'a>(
        &mut self,
        origin: &str,
        username: &'a str,
        password: &'a str,
        existing: Option<&PasswordRecord>,
    ) -> CaptureOutcome<'a> {
        let primed = matches!(&self.state, FillState::Candidate { origin: o } if o == origin);
        if !primed || password.is_empty() {
            return CaptureOutcome::Ignore;
        }
        self.state = FillState::Prompted;
        match existing {
            Some(record) if record.username == username => CaptureOutcome::Update {
                record_id: record.id.clone(),
                username,
                password,
            },
            _ => CaptureOutcome::Save { username, password },
        }
    }

    /// Fold the user's doorhanger answer back into the state machine. A
    /// resolution with no prompt in flight is dropped on the floor — a
    /// stale "save" from a vanished doorhanger must write nothing.
    pub fn resolve_prompt(&mut self, save: bool) {
        if matches!(self.state, FillState::Prompted) {
            self.state = if save { FillState::Saved } else { FillState::Discarded };
        }
    }

    /// Current state, for the UI to render.
    pub fn state(&self) -> &FillState {
        &self.state
    }
}

/// Where to write credentials on fill. Carries no secret bytes: secrets
/// resolve inside the engine at fill time, through
/// [`crate::store::PasswordStore::reveal`], and never transit UI state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillPlan {
    /// Field index for the username. `usize::MAX` means the form has no
    /// username field — fill the password, nothing else.
    pub username_target: usize,
    /// Field index for the password.
    pub password_target: usize,
}

impl FillPlan {
    /// Build a plan from a candidate, validated against the form's current
    /// field count. SPAs rerender between detection and fill, and a stale
    /// index would write a password into a stranger's field — that is a
    /// `FormDetection` failure, not a best-effort fill.
    pub fn from_candidate(
        candidate: &LoginCandidate,
        field_count: usize,
    ) -> Result<FillPlan, PasswordError> {
        if candidate.password_index >= field_count {
            return Err(PasswordError::FormDetection(format!(
                "password field {} is gone from a {}-field form",
                candidate.password_index, field_count
            )));
        }
        if let Some(u) = candidate.username_index {
            if u >= field_count {
                return Err(PasswordError::FormDetection(format!(
                    "username field {u} is gone from a {field_count}-field form"
                )));
            }
        }
        Ok(FillPlan {
            username_target: candidate.username_index.unwrap_or(usize::MAX),
            password_target: candidate.password_index,
        })
    }
}
