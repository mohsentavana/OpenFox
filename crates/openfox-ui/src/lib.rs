pub mod chrome;
pub mod context_menu;
pub mod dialogs;
pub mod downloads_ui;
pub mod findbar;
pub mod icons;
pub mod library;
pub mod newtab;
pub mod reader_ui;
pub mod settings_ui;
pub mod shield;
pub mod tabstrip;
pub mod theme;
pub mod toolbar;
pub mod widgets;

pub use chrome::{view, ChromeIntent, ChromePanel, ChromeState, TabView};
pub use context_menu::{ContextMenu, HitTarget};
pub use dialogs::{Dialog, DialogModel};
pub use downloads_ui::{DownloadRow, DownloadsPanel};
pub use findbar::FindBar;
pub use library::{LibraryAction, LibrarySidebar};
pub use newtab::{NewTabState, QuickLink};
pub use reader_ui::ReaderSurface;
pub use settings_ui::{SettingRow, SettingsSurface};
pub use theme::{accent, accent_ink};

use openfox_core::BrowserEvent;

/// Reduces a browser event into chrome state. The UI never sees the browser
/// directly — it sees events, applies them, and Rune re-renders. This is also
/// the headless path: agents drive the same state machine.
pub fn reduce(state: &mut ChromeState, event: &BrowserEvent) {
    match event {
        BrowserEvent::TabOpened { tab } => {
            state.tabs.push(TabView::loading(tab.0));
            state.active = state.tabs.len() - 1;
        }
        BrowserEvent::TabClosed { tab } => {
            state.tabs.retain(|view| view.id != tab.0);
            state.active = state.active.min(state.tabs.len().saturating_sub(1));
        }
        BrowserEvent::NavigationStarted { tab, url } => {
            if let Some(view) = state.tabs.iter_mut().find(|view| view.id == tab.0) {
                view.loading = true;
                view.address = url.clone();
            }
            state.loading = true;
            state.address = url.clone();
        }
        BrowserEvent::NavigationCompleted { tab, url, status, .. } => {
            if let Some(view) = state.tabs.iter_mut().find(|view| view.id == tab.0) {
                view.loading = false;
                view.address = url.clone();
                view.status = Some(*status);
            }
            state.loading = false;
            state.address = url.clone();
        }
        BrowserEvent::NavigationFailed { tab, url, code } => {
            if let Some(view) = state.tabs.iter_mut().find(|view| view.id == tab.0) {
                view.loading = false;
                view.address = url.clone();
                view.status = Some(*code);
            }
            state.loading = false;
        }
        BrowserEvent::TitleChanged { tab, title } => {
            if let Some(view) = state.tabs.iter_mut().find(|view| view.id == tab.0) {
                view.title = title.clone();
            }
        }
        BrowserEvent::FaviconChanged { .. } => {
            // Favicon repaint rides the next Rune frame; no chrome state
            // changes here.
        }
        BrowserEvent::ShieldUpdated { counters, .. } => {
            state.shield.ads = counters.ads_blocked;
            state.shield.trackers = counters.trackers_blocked;
            state.shield.saved_kb = (counters.bytes_saved / 1024) as u32;
            state.shield.requests = counters.requests_saved;
        }
        BrowserEvent::ZoomChanged { level, .. } => {
            state.zoom = *level;
        }
        BrowserEvent::ReaderAvailabilityChanged { available, .. } => {
            state.reader_available = *available;
        }
        // Console and network traffic stream to devtools and the shield
        // ledger; the chrome itself has nothing to render for them.
        BrowserEvent::Console { .. } | BrowserEvent::NetworkRequest { .. } => {}
        // Downloads render through their own panel subscription.
        BrowserEvent::DownloadProgress { .. } => {}
    }
}
