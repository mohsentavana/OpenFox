//! Permissions: per-origin grants, per-container scoping, one prompt at
//! a time, ephemeral grants that die with the tab. A permission nobody
//! can find to revoke is not a permission, it is a leak.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Permission {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    Midi,
    PersistentStorage,
    ScreenWakeLock,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Geolocation => "geolocation",
            Permission::Notifications => "notifications",
            Permission::Camera => "camera",
            Permission::Microphone => "microphone",
            Permission::Midi => "midi",
            Permission::PersistentStorage => "persistent-storage",
            Permission::ScreenWakeLock => "screen-wake-lock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PermissionState {
    /// The browser default: ask. This is the state almost every origin
    /// should be in, forever.
    Prompt,
    Granted,
    Denied,
}

#[derive(Debug, Clone)]
pub struct Grant {
    pub state: PermissionState,
    /// Unix ms expiry. None means until revoked — camera and microphone
    /// grants are always ephemeral, which is a policy this file keeps.
    pub expires_at_ms: Option<u64>,
    pub granted_at_ms: u64,
}

/// The key: (container, origin, permission). Container-scoped by
/// construction — a work container's camera grant never leaks into the
/// personal container on the same site.
type Key = (String, String, Permission);

pub struct PermissionManager {
    grants: HashMap<Key, Grant>,
    prompt_queue: Vec<Key>,
}

impl Default for PermissionManager {
    fn default() -> Self {
        PermissionManager {
            grants: HashMap::new(),
            prompt_queue: Vec::new(),
        }
    }
}

impl PermissionManager {
    /// Fast path: camera and microphone never persist across sessions.
    fn always_ephemeral(permission: Permission) -> bool {
        matches!(permission, Permission::Camera | Permission::Microphone)
    }

    pub fn state(
        &self,
        container: &str,
        origin: &str,
        permission: Permission,
        now_ms: u64,
    ) -> PermissionState {
        match self.grants.get(&(container.into(), origin.into(), permission)) {
            Some(grant) => match grant.expires_at_ms {
                Some(expiry) if expiry <= now_ms => PermissionState::Prompt,
                _ => grant.state,
            },
            None => PermissionState::Prompt,
        }
    }

    pub fn grant(
        &mut self,
        container: &str,
        origin: &str,
        permission: Permission,
        now_ms: u64,
    ) {
        let expires_at_ms = if Self::always_ephemeral(permission) {
            // Ephemeral grants: the tab close clears them (see
            // on_tab_closed); the timestamp is the belt to that brace.
            Some(now_ms + 12 * 3600 * 1000)
        } else {
            None
        };
        self.grants.insert(
            (container.into(), origin.into(), permission),
            Grant {
                state: PermissionState::Granted,
                expires_at_ms,
                granted_at_ms: now_ms,
            },
        );
    }

    pub fn deny(&mut self, container: &str, origin: &str, permission: Permission, now_ms: u64) {
        self.grants.insert(
            (container.into(), origin.into(), permission),
            Grant {
                state: PermissionState::Denied,
                expires_at_ms: None,
                granted_at_ms: now_ms,
            },
        );
    }

    pub fn revoke(&mut self, container: &str, origin: &str, permission: Permission) {
        self.grants
            .remove(&(container.into(), origin.into(), permission));
    }

    /// The one-prompt-at-a-time queue: sites love stacking permission
    /// prompts over each other; users hate it more.
    pub fn enqueue_prompt(&mut self, container: &str, origin: &str, permission: Permission) {
        let key: Key = (container.into(), origin.into(), permission);
        if !self.prompt_queue.contains(&key) {
            self.prompt_queue.push(key);
        }
    }

    pub fn next_prompt(&mut self) -> Option<(String, String, Permission)> {
        let key = self.prompt_queue.first().cloned()?;
        Some(key)
    }

    pub fn resolve_prompt(&mut self) -> Option<(String, String, Permission)> {
        self.prompt_queue.pop()
    }

    /// Tab close: ephemeral grants die here. The caller passes the
    /// container id whose tabs all closed.
    pub fn on_container_closed(&mut self, container: &str) {
        self.grants.retain(|(c, _, _), _| c != container);
        self.prompt_queue.retain(|(c, _, _)| c != container);
    }

    /// Every grant for the exceptions editor in settings — the "what
    /// have I allowed" list, readable and revocable.
    pub fn list_grants(&self) -> Vec<(String, String, Permission, PermissionState)> {
        self.grants
            .iter()
            .map(|((container, origin, permission), grant)| {
                (container.clone(), origin.clone(), *permission, grant.state)
            })
            .collect()
    }
}
