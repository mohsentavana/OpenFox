//! Form autofill storage: addresses and sealed credit cards, usage
//! recency, field-label match heuristics. Card numbers arrive already
//! sealed by openfox-passwords' crypto — this crate stores opaque
//! bytes and knows nothing about keys.

use crate::StoreError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Address {
    pub given_name: String,
    pub family_name: String,
    pub street: String,
    pub city: String,
    pub postal_code: String,
    pub country: String,
    pub email: String,
    pub tel: String,
    pub times_used: u32,
    pub last_used_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreditCard {
    pub label: String,
    /// Sealed by openfox-passwords' VaultCrypto. Opaque here by design:
    /// storage never holds card numbers in the clear, and the unseal
    /// path lives behind the same master-password gate as passwords.
    pub number_sealed: String,
    pub exp_month: u8,
    pub exp_year: u16,
    pub times_used: u32,
    pub last_used_ms: u64,
}

/// The autofill field kinds and the label fragments that identify them.
/// Real-world label text is a swamp; the table errs toward precision
/// over recall — a wrong autofill is worse than a missing one.
pub const FIELD_HINTS: &[(&str, AutofillField)] = &[
    ("email", AutofillField::Email),
    ("e-mail", AutofillField::Email),
    ("zip", AutofillField::PostalCode),
    ("postal", AutofillField::PostalCode),
    ("city", AutofillField::City),
    ("street", AutofillField::Street),
    ("address", AutofillField::Street),
    ("first name", AutofillField::GivenName),
    ("given name", AutofillField::GivenName),
    ("last name", AutofillField::FamilyName),
    ("surname", AutofillField::FamilyName),
    ("family name", AutofillField::FamilyName),
    ("full name", AutofillField::FullName),
    ("phone", AutofillField::Tel),
    ("mobile", AutofillField::Tel),
    ("card number", AutofillField::CardNumber),
    ("cardnumber", AutofillField::CardNumber),
    ("cc number", AutofillField::CardNumber),
    ("cvc", AutofillField::CardCode),
    ("cvv", AutofillField::CardCode),
    ("security code", AutofillField::CardCode),
    ("exp", AutofillField::CardExpiry),
    ("country", AutofillField::Country),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutofillField {
    GivenName,
    FamilyName,
    FullName,
    Street,
    City,
    PostalCode,
    Country,
    Email,
    Tel,
    CardNumber,
    CardCode,
    CardExpiry,
}

/// Match a field label to an autofill kind. Case-insensitive substring,
/// longest hint first so "card number" beats "card".
pub fn classify_field(label: &str) -> Option<AutofillField> {
    let label = label.to_ascii_lowercase();
    FIELD_HINTS
        .iter()
        .filter(|(hint, _)| label.contains(hint))
        .max_by_key(|(hint, _)| hint.len())
        .map(|(_, field)| *field)
}

pub struct FormStore {
    addresses: Vec<Address>,
    cards: Vec<CreditCard>,
    /// Hosts where the user said "never autofill." Respected absolutely.
    never_store: Vec<String>,
}

impl Default for FormStore {
    fn default() -> Self {
        FormStore {
            addresses: Vec::new(),
            cards: Vec::new(),
            never_store: Vec::new(),
        }
    }
}

impl FormStore {
    pub fn add_address(&mut self, address: Address) {
        self.addresses.retain(|existing| {
            existing.email != address.email || existing.street != address.street
        });
        self.addresses.push(address);
    }

    pub fn add_card(&mut self, card: CreditCard) {
        self.cards.retain(|existing| existing.label != card.label);
        self.cards.push(card);
    }

    /// The most-recently-used address, if any: one profile is the 90%
    /// case, and offering a dropdown of one is noise.
    pub fn best_address(&self) -> Option<&Address> {
        self.addresses.iter().max_by_key(|a| a.last_used_ms)
    }

    pub fn mark_address_used(&mut self, email: &str, now_ms: u64) {
        for address in &mut self.addresses {
            if address.email == email {
                address.times_used += 1;
                address.last_used_ms = now_ms;
            }
        }
    }

    pub fn never_autofill(&mut self, host: &str) {
        if !self.never_store.iter().any(|h| h == host) {
            self.never_store.push(host.to_owned());
        }
    }

    pub fn autofill_allowed(&self, host: &str) -> bool {
        !self.never_store.iter().any(|h| h == host)
    }

    pub fn addresses(&self) -> &[Address] {
        &self.addresses
    }

    pub fn cards(&self) -> &[CreditCard] {
        &self.cards
    }

    /// Persist through the sqlite store: JSON columns, one row per
    /// record. The schema is deliberately dumb — the sealed card blob is
    /// the interesting part and it is opaque here.
    pub fn save(&self, store: &mut crate::Store) -> Result<(), StoreError> {
        let conn = store.conn();
        conn.execute("DELETE FROM form_addresses", [])?;
        for address in &self.addresses {
            let payload = serde_json::to_string(address).unwrap_or_default();
            conn.execute(
                "INSERT INTO form_addresses (payload, last_used_ms) VALUES (?1, ?2)",
                rusqlite::params![payload, address.last_used_ms as i64],
            )?;
        }
        conn.execute("DELETE FROM form_cards", [])?;
        for card in &self.cards {
            let payload = serde_json::to_string(card).unwrap_or_default();
            conn.execute(
                "INSERT INTO form_cards (payload, last_used_ms) VALUES (?1, ?2)",
                rusqlite::params![payload, card.last_used_ms as i64],
            )?;
        }
        Ok(())
    }
}
