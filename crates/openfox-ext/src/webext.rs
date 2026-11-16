//! WebExtension compatibility: the API surface mapped to OpenFox
//! capabilities, manifest v2 and v3 both. Extensions run as WASM with
//! the capability model from openfox-ext::manifest — this file says
//! what a Chrome extension can expect when it lands here, honestly.

/// The compatibility verdict per API call.
#[derive(Debug, Clone, PartialEq)]
pub enum CompatVerdict {
    /// Mapped natively: the call runs against a real OpenFox surface.
    Native(&'static str),
    /// Runs with limits the doc states (e.g. blocking webRequest in MV3).
    Shim(&'static str),
    /// Refused, with the reason an extension author deserves. Owns its
    /// text: the reason can quote the call, which is runtime data.
    Unsupported(String),
}

use CompatVerdict::{Native, Shim, Unsupported};

/// Map one API call to its OpenFox home. The table is the contract —
/// it is what the AMO listing page will render, so it tells the truth.
pub fn map_api(call: &str) -> CompatVerdict {
    match call {
        // Tabs: fully native — tabs are first-class core state.
        "tabs.query" => Native("openfox_core::TabManager"),
        "tabs.create" => Native("openfox_core::Command::NewTab"),
        "tabs.update" => Native("openfox_core::Command::Navigate"),
        "tabs.remove" => Native("openfox_core::Command::CloseTab"),
        "tabs.onUpdated" => Native("openfox_core::BrowserEvent"),
        "tabs.sendMessage" => Native("openfox_ext::runtime message ports"),

        // Storage: native, with the same partitioning the browser itself
        // uses — per-container storage keys.
        "storage.local.get" | "storage.local.set" => {
            Native("openfox_storage::Store (container-partitioned)")
        }
        "storage.sync.get" | "storage.sync.set" => {
            Shim("routes through openfox-sync when a sync account exists; local-only otherwise")
        }

        // WebRequest: blocking listeners are the MV2 fight. OpenFox
        // honors them — its own shield is a blocking listener, it would
        // be rude to forbid the user's.
        "webRequest.onBeforeRequest" => {
            Native("openfox_net pipeline hook, same stage as the shield")
        }
        "webRequest.onBeforeSendHeaders" => {
            Native("openfox_net pipeline hook, header stage")
        }
        "webRequest.onCompleted" => Native("openfox_core::BrowserEvent::NetworkRequest"),

        // Runtime messaging between the extension's own components.
        "runtime.sendMessage" | "runtime.onMessage" => {
            Native("openfox_ext::runtime, WASM instance to instance")
        }
        "runtime.getURL" => Native("openfox_ext resource table (namespaced)"),

        // Notifications go through the OS integration.
        "notifications.create" => Native("openfox_platform::notify"),

        // Menus (context menus): the context menu model exists; the
        // extension registers items into it.
        "menus.create" | "contextMenus.create" => Native("openfox_ui::context_menu"),

        // Downloads manager.
        "downloads.download" => Native("openfox_download::DownloadManager"),

        // The powers we refuse, and why.
        "webRequest.onAuthRequired" => {
            Unsupported("credential interception is a phishing primitive; OpenFox does not relay it to extensions".to_owned())
        }
        "privacy.network.networkPredictionEnabled" => {
            Unsupported("privacy settings belong to the user, not to an extension".to_owned())
        }
        "proxy.settings" => {
            Unsupported("proxy configuration belongs to the user".to_owned())
        }
        "browserSettings.*" => {
            Unsupported("browser settings belong to the user".to_owned())
        }
        other => Unsupported(other.to_owned()),
    }
}

/// Manifest version support: v2 and v3, mapped onto one capability
/// model. MV3's service workers run as lazily-instantiated WASM tasks —
/// the lifetime is honored (event-driven, not resident), the runtime is
/// ours.
pub fn manifest_verdict(version: &str, permissions: &[String]) -> ManifestVerdict {
    let supported = matches!(version, "2" | "3");
    let unknown: Vec<String> = permissions
        .iter()
        .filter(|permission| !KNOWN_PERMISSIONS.contains(&permission.as_str()))
        .cloned()
        .collect();
    ManifestVerdict {
        manifest_supported: supported,
        unknown_permissions: unknown,
    }
}

#[derive(Debug, Clone)]
pub struct ManifestVerdict {
    pub manifest_supported: bool,
    /// Permissions this build does not recognize — surfaced to the user
    /// at install time as "this extension wants something OpenFox has
    /// never heard of," never silently granted.
    pub unknown_permissions: Vec<String>,
}

pub const KNOWN_PERMISSIONS: &[&str] = &[
    "tabs",
    "storage",
    "webRequest",
    "webRequestBlocking",
    "notifications",
    "menus",
    "contextMenus",
    "downloads",
    "activeTab",
    "scripting",
];
