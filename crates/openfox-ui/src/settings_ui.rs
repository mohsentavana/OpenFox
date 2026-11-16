//! The settings surface: typed rows bound to openfox-core settings keys, a
//! per-site exception editor, and search-within-settings.

use openfox_core::Settings;
use std::collections::HashSet;

/// The five shelves of settings. Sync and Extensions exist as sections even
// while their subsystems are young, so the surface stops moving before
/// users build habits around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    General,
    Privacy,
    Sync,
    Extensions,
    Advanced,
}

impl SettingsSection {
    pub fn title_key(&self) -> &'static str {
        match self {
            SettingsSection::General => "settings.general",
            SettingsSection::Privacy => "settings.privacy",
            SettingsSection::Sync => "settings.sync",
            SettingsSection::Extensions => "settings.extensions",
            SettingsSection::Advanced => "settings.advanced",
        }
    }
}

/// A settings row, typed by control. `key` names the openfox-core
/// `Settings` field, which is also the storage key — one vocabulary from
/// UI to disk.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingRow {
    Toggle { key: &'static str, value: bool },
    Select { key: &'static str, options: &'static [&'static str], current: &'static str },
    Action { key: &'static str },
    Text { key: &'static str, value: String },
}

/// The permission a site exception covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SitePermission {
    Camera,
    Microphone,
    Location,
    Notifications,
}

impl SitePermission {
    pub fn key(&self) -> &'static str {
        match self {
            SitePermission::Camera => "perm.camera",
            SitePermission::Microphone => "perm.microphone",
            SitePermission::Location => "perm.location",
            SitePermission::Notifications => "perm.notifications",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionState {
    Allow,
    Block,
    Ask,
}

/// One line of the per-site exceptions editor.
#[derive(Debug, Clone)]
pub struct SiteException {
    pub origin: String,
    pub permission: SitePermission,
    pub state: ExceptionState,
}

/// The settings surface state.
#[derive(Debug, Clone, Default)]
pub struct SettingsSurface {
    pub section: SettingsSection,
    pub rows: Vec<SettingRow>,
    pub exceptions: Vec<SiteException>,
    /// Search-within-settings: matches row keys, case-insensitive.
    pub filter: String,
}

impl SettingsSurface {
    /// Builds the surface from live settings. Rows mirror the openfox-core
    /// `Settings` struct field-for-field so nothing settable is unreachable.
    pub fn from_settings(settings: &Settings) -> SettingsSurface {
        SettingsSurface {
            section: SettingsSection::General,
            rows: vec![
                SettingRow::Text { key: "homepage", value: settings.homepage.clone() },
                SettingRow::Text { key: "search_template", value: settings.search_template.clone() },
                SettingRow::Toggle { key: "block_ads", value: settings.block_ads },
                SettingRow::Toggle { key: "block_trackers", value: settings.block_trackers },
                SettingRow::Toggle { key: "cosmetic_filtering", value: settings.cosmetic_filtering },
                SettingRow::Toggle { key: "first_party_cookies", value: settings.first_party_cookies },
                SettingRow::Toggle { key: "privacy_signals", value: settings.privacy_signals },
                SettingRow::Toggle { key: "trim_referer", value: settings.trim_referer },
                SettingRow::Text { key: "user_agent", value: settings.user_agent.clone() },
                SettingRow::Action { key: "settings.action.clear_browsing_data" },
                SettingRow::Action { key: "settings.action.manage_extensions" },
            ],
            exceptions: Vec::new(),
            filter: String::new(),
        }
    }

    /// Rows surviving the search filter. Keys are matched, not display
    /// strings: the filter must work in every language the UI ships.
    pub fn visible_rows(&self) -> Vec<&SettingRow> {
        let needle = self.filter.to_lowercase();
        self.rows
            .iter()
            .filter(|row| {
                let key = match row {
                    SettingRow::Toggle { key, .. }
                    | SettingRow::Select { key, .. }
                    | SettingRow::Action { key }
                    | SettingRow::Text { key, .. } => *key,
                };
                needle.is_empty() || key.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Adds a site exception, replacing any exception for the same origin
    /// and permission — duplicates would render as two rows and mean one.
    pub fn add_exception(&mut self, exception: SiteException) {
        self.exceptions.retain(|existing| {
            !(existing.origin == exception.origin && existing.permission == exception.permission)
        });
        self.exceptions.push(exception);
    }

    pub fn remove_exception(&mut self, origin: &str, permission: SitePermission) -> bool {
        let before = self.exceptions.len();
        self.exceptions
            .retain(|existing| !(existing.origin == origin && existing.permission == permission));
        before != self.exceptions.len()
    }

    /// Origins the user has made any exception for — the shield panel and
    /// the site permissions door hanger both start from this set.
    pub fn exception_origins(&self) -> HashSet<&str> {
        self.exceptions.iter().map(|exception| exception.origin.as_str()).collect()
    }
}
