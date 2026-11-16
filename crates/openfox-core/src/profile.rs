//! Profiles and containers. Profiles separate people; containers
//! separate identities within a person. Each container owns its cookie
//! jar key, so a work tab and a personal tab on the same site never
//! share a session. Private profiles are ephemeral by construction:
//! nothing persists, session cookies only, history writes are dropped.

use std::collections::HashMap;
use std::path::PathBuf;

/// The eight Firefox container colors, kept because users have years of
/// muscle memory for them.
pub const CONTAINER_COLORS: [&str; 8] = [
    "blue", "turquoise", "green", "yellow", "orange", "red", "pink", "purple",
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Container {
    pub identity_id: String,
    pub name: String,
    /// Index into CONTAINER_COLORS.
    pub color_key: u8,
    pub icon_key: u8,
    /// The cookie-jar isolation key. Same jar key, same session space;
    /// different key, different everything.
    pub cookie_jar_key: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// Ephemeral profile: private-window semantics for the whole
    /// profile. Writes to disk are refused, not queued.
    pub ephemeral: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProfileManager {
    profiles: Vec<Profile>,
    containers: Vec<Container>,
    /// Per-container history flag: some identities should leave no
    /// trace even in local history.
    history_disabled: HashMap<String, bool>,
}

impl ProfileManager {
    pub fn add_profile(&mut self, profile: Profile) {
        self.profiles.retain(|existing| existing.id != profile.id);
        self.profiles.push(profile);
    }

    /// Create an ephemeral profile under the temp root. Nothing about it
    /// survives the process: that is the guarantee, and refusing writes
    /// is how it is kept.
    pub fn ephemeral() -> Profile {
        let id = format!("private-{}", std::process::id());
        let path = std::env::temp_dir().join(format!("openfox-{id}"));
        Profile {
            id,
            name: "Private".into(),
            path,
            ephemeral: true,
        }
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn active_is_ephemeral(&self, profile_id: &str) -> bool {
        self.profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .is_some_and(|profile| profile.ephemeral)
    }

    pub fn add_container(&mut self, name: &str, color_key: u8, icon_key: u8) -> Container {
        let identity_id = format!("container-{}", self.containers.len() + 1);
        let cookie_jar_key = format!("jar-{identity_id}");
        let container = Container {
            identity_id: identity_id.clone(),
            name: name.to_owned(),
            color_key: color_key.min(CONTAINER_COLORS.len() as u8 - 1),
            icon_key,
            cookie_jar_key,
        };
        self.containers.push(container.clone());
        container
    }

    pub fn containers(&self) -> &[Container] {
        &self.containers
    }

    pub fn container_for(&self, identity_id: &str) -> Option<&Container> {
        self.containers
            .iter()
            .find(|container| container.identity_id == identity_id)
    }

    /// The jar key for an identity: the container's own jar, or the
    /// default jar for plain tabs. This one lookup is the entire
    /// isolation mechanism — everything else flows from it.
    pub fn jar_for(&self, identity_id: Option<&str>) -> String {
        identity_id
            .and_then(|id| self.container_for(id))
            .map(|container| container.cookie_jar_key.clone())
            .unwrap_or_else(|| "jar-default".into())
    }

    pub fn set_history_disabled(&mut self, identity_id: &str, disabled: bool) {
        self.history_disabled.insert(identity_id.to_owned(), disabled);
    }

    pub fn history_allowed(&self, identity_id: Option<&str>) -> bool {
        identity_id
            .and_then(|id| self.history_disabled.get(id).copied())
            .map(|disabled| !disabled)
            .unwrap_or(true)
    }
}
