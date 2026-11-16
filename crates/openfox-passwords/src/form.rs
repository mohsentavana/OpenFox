//! Login-form detection in the shape of Firefox's form manager: a password
//! field makes a form a login form, autocomplete tokens refine the guess,
//! and a second password field means signup.

/// What the DOM layer believes a field is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text.
    Text,
    /// `type=email`.
    Email,
    /// `type=password` — the field the whole module hangs on.
    Password,
    /// Search boxes, checkboxes, everything else.
    Other,
}

/// A form field as classified by the DOM layer, so the detector never
/// touches markup.
#[derive(Debug, Clone)]
pub struct FieldDescriptor {
    pub kind: FieldKind,
    /// `autocomplete` tokens, already lowercased by the DOM layer.
    pub autocomplete: Vec<String>,
    /// Raw `name` attribute, kept for diagnostics.
    pub name: String,
    /// Raw `id` attribute, kept for diagnostics.
    pub id: String,
}

/// What the detector concluded about one form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginCandidate {
    /// Best username guess, if any — password-only logins exist and are
    /// worth saving.
    pub username_index: Option<usize>,
    /// The password field; a candidate without one is not a candidate.
    pub password_index: usize,
    /// Signup or change-password form: capture yes, autofill never.
    pub looks_like_signup: bool,
}

/// Stateless detector — one function, no knobs, because form heuristics
/// with configuration are how password managers fill the wrong field.
pub struct LoginFormDetector;

impl LoginFormDetector {
    /// Classify a form, or return `None` when it is not a login form at
    /// all. Firefox requires a password field first; a lone username box
    /// is a search box.
    ///
    /// Username candidate, in precedence order:
    ///
    /// * `autocomplete="username"` — the strongest signal there is; a
    ///   site setting it is explicitly declaring the login field.
    /// * an email field directly before the password — Firefox's form
    ///   manager walks back from the password field, and on virtually
    ///   every real login page the field right in front of it is the
    ///   username.
    ///
    /// Signup detection: any `autocomplete="new-password"` token — how a
    /// site says "this is signup, not login"; Firefox never autofills a
    /// saved password into such a field — or two adjacent password
    /// fields, because only signup and change flows ask for the password
    /// twice.
    pub fn detect(fields: &[FieldDescriptor]) -> Option<LoginCandidate> {
        let password_index = fields.iter().position(|f| f.kind == FieldKind::Password)?;

        let username_index = fields
            .iter()
            .position(|f| f.autocomplete.iter().any(|t| t.eq_ignore_ascii_case("username")))
            .or_else(|| {
                password_index
                    .checked_sub(1)
                    .and_then(|i| (fields[i].kind == FieldKind::Email).then_some(i))
            });

        let new_password = fields
            .iter()
            .any(|f| f.autocomplete.iter().any(|t| t.eq_ignore_ascii_case("new-password")));
        let two_adjacent_passwords = fields
            .get(password_index + 1)
            .is_some_and(|f| f.kind == FieldKind::Password);

        Some(LoginCandidate {
            username_index,
            password_index,
            looks_like_signup: new_password || two_adjacent_passwords,
        })
    }
}
