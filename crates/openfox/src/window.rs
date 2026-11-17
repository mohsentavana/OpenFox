//! The desktop window: the Rune app that mounts the chrome. The browser
//! core runs behind a static handle (Rune's `AppSpec::init` takes no
//! arguments — the OnceLock is the honest bridge); every frame the
//! chrome's intent queue drains onto a task thread, and the backend's
//! truth (tabs, shield counters) flows back into the chrome state.
//! Nothing blocks the frame: navigation runs off-thread and wakes the
//! window through the Waker `setup` handed us.

use openfox_mcp::{Backend, Direction};
use openfox_ui::{view as chrome_view, ChromeIntent, ChromeState, TabView};
use rune::{App as RuneApp, AppSpec, Cx, El, Tx, Waker};
use std::sync::{Arc, Mutex, OnceLock};

type BackendHandle = Arc<Mutex<dyn Backend>>;

static BACKEND: OnceLock<BackendHandle> = OnceLock::new();
static WAKER: OnceLock<Waker> = OnceLock::new();

/// Mounts the window. `backend` is the same handle the MCP transports
/// drive; the window is just one more client of the browser.
pub fn run(backend: BackendHandle) {
    let _ = BACKEND.set(backend);
    RuneApp::new(AppSpec {
        title: "OpenFox",
        // Fits a 1366x768 laptop with room for the title bar and taskbar;
        // the chrome is fully flexible below that down to the floor.
        size: (1080.0, 680.0),
        min_size: Some((480.0, 360.0)),
        init,
        view,
        scenes: &[],
        fonts: &[],
        families: ("Inter", "Inter"),
        setup: Some(setup),
        poll: Some(poll),
        on_key: None,
    })
    .run();
}

/// The chrome view under Rune's app-state shape: one adapter, nothing
/// remapped.
fn view(s: &ChromeState, _cx: &mut Cx<ChromeState>) -> El {
    chrome_view(s)
}

fn init() -> ChromeState {
    let mut chrome = ChromeState::default();
    if let Some(backend) = BACKEND.get() {
        if let Ok(mut browser) = backend.lock() {
            sync_from_backend(&mut chrome, &mut *browser);
        }
    }
    chrome
}

fn setup(_state: &mut ChromeState, waker: Waker) {
    let _ = WAKER.set(waker);
}

/// One frame: hand the chrome's intents to a task thread, then pull the
/// backend's truth back. Both are cheap — the tab list is a handful of
/// rows — and neither waits on the network: `navigate` itself runs on
/// the task thread and wakes us again when the page is in.
///
/// The frame thread NEVER blocks on the backend mutex. A navigation can
/// legitimately hold the backend for the duration of its network budget
/// (tens of seconds on a slow origin); if `poll` waited on that lock the
/// window would stop pumping events and the OS would flag the app
/// "not responding". `try_lock` instead: a busy backend simply skips the
/// sync this frame, and the task thread's `wake()` schedules the frame
/// that reads the finished state.
fn poll(state: &mut ChromeState, _elapsed: f64, _tx: &mut Tx) -> bool {
    let Some(backend) = BACKEND.get() else { return false };
    let mut changed = false;

    let mut intents = std::mem::take(&mut state.intents);
    if !intents.is_empty() {
        // Back, forward and address-bar navigation act on the active tab;
        // make the backend's active tab what the user is looking at
        // before the intent runs.
        let active_id = state.active_tab().map(|tab| tab.id);
        let mut navigates = false;
        let mut targeted: Vec<ChromeIntent> = Vec::with_capacity(intents.len() + 1);
        for intent in intents.drain(..) {
            let needs_target = matches!(
                intent,
                ChromeIntent::Navigate(_) | ChromeIntent::GoBack
                    | ChromeIntent::GoForward | ChromeIntent::Reload
            );
            if needs_target {
                navigates = true;
                if let Some(id) = active_id {
                    targeted.push(ChromeIntent::SelectTab(id));
                }
            }
            targeted.push(intent);
        }

        // Optimistic loading: the load line and the spinner show the
        // instant the intent is raised; the backend's truth replaces
        // them when the task thread finishes and wakes us.
        if navigates {
            state.loading = true;
            if let Some(tab) = state.active_tab_mut() {
                tab.loading = true;
            }
        }

        let backend = backend.clone();
        let waker = WAKER.get().cloned();
        changed = true;
        let _ = rune::task::spawn("openfox-intent", move || {
            if let Ok(mut browser) = backend.lock() {
                for intent in targeted {
                    run_intent(&mut *browser, intent);
                }
            }
            if let Some(wake) = waker {
                wake();
            }
        });
    }

    // Never wait on the browser: while a navigation holds the backend —
    // seconds of budgeted network — the frame loop must keep pumping
    // input. A busy lock skips the sync; the wake() after the task
    // brings poll back for the truth.
    if let Ok(mut browser) = backend.try_lock() {
        changed |= sync_from_backend(state, &mut *browser);
    }
    changed
}

/// Runs one intent against the backend. The MCP tools and the window
/// share this path: what an agent drives and what the user drives cannot
/// drift apart, because there is one surface.
fn run_intent(browser: &mut dyn Backend, intent: ChromeIntent) {
    match intent {
        ChromeIntent::Navigate(input) => {
            let _ = browser.navigate(&input);
        }
        ChromeIntent::GoBack => {
            let _ = browser.go(Direction::Back);
        }
        ChromeIntent::GoForward => {
            let _ = browser.go(Direction::Forward);
        }
        ChromeIntent::Reload => {
            let _ = browser.go(Direction::Reload);
        }
        ChromeIntent::NewTab => {
            let _ = browser.tabs_new(None);
        }
        ChromeIntent::CloseTab(id) => {
            let _ = browser.tabs_close(id);
        }
        ChromeIntent::SelectTab(id) => {
            let _ = browser.tabs_select(id);
        }
    }
}

/// Pulls the backend's truth into the chrome: tabs, the shield counters,
/// the loading line, and the address bar (unless the user is editing).
fn sync_from_backend(chrome: &mut ChromeState, browser: &mut dyn Backend) -> bool {
    let mut changed = false;

    if let Ok(list) = browser.tabs_list() {
        let mut tabs: Vec<TabView> = Vec::new();
        if let Some(entries) = list.get("tabs").and_then(|t| t.as_array()) {
            for entry in entries {
                let id = entry.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                let title = entry
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                let address = entry
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                let loading = entry.get("state").and_then(|v| v.as_bool()).unwrap_or(false);
                tabs.push(TabView { id, title, address, loading, status: None });
            }
        }
        // Keep the user's selection on the same tab id when it survives.
        let selected_id = chrome.active_tab().map(|tab| tab.id);
        let next_active = match selected_id {
            Some(id) => tabs.iter().position(|tab| tab.id == id).unwrap_or(0),
            None => chrome.active.min(tabs.len().saturating_sub(1)),
        };
        if tabs != chrome.tabs || next_active != chrome.active {
            chrome.tabs = tabs;
            chrome.active = next_active;
            changed = true;
        }
    }

    if let Ok(stats) = browser.adblock_stats() {
        let ads = stats.get("adsBlocked").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let trackers = stats.get("trackersBlocked").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let requests = stats.get("requestsSaved").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let saved_kb = stats.get("savedBytesEstimate").and_then(|v| v.as_u64()).unwrap_or(0) as u32 / 1024;
        let top_hosts: Vec<(String, u32)> = stats
            .get("topHosts")
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| {
                        (
                            entry.get("host").and_then(|v| v.as_str()).unwrap_or_default().to_owned(),
                            entry.get("blocked").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        if chrome.shield.ads != ads
            || chrome.shield.trackers != trackers
            || chrome.shield.requests != requests
            || chrome.shield.saved_kb != saved_kb
        {
            chrome.shield.ads = ads;
            chrome.shield.trackers = trackers;
            chrome.shield.requests = requests;
            chrome.shield.saved_kb = saved_kb;
            chrome.shield.top_hosts = top_hosts;
            changed = true;
        }
    }

    // The loading line and the address mirror the active tab; the editor
    // keeps whatever the user is typing until it loses focus.
    let (loading, address) = chrome
        .active_tab()
        .map(|tab| (tab.loading, tab.address.clone()))
        .unwrap_or((false, String::new()));
    if chrome.loading != loading {
        chrome.loading = loading;
        changed = true;
    }
    if chrome.address != address {
        chrome.address = address.clone();
        if !chrome.url_edit.focused {
            chrome.url_edit.replace_text(address);
        }
        changed = true;
    } else if !chrome.url_edit.focused && chrome.url_edit.text != address {
        chrome.url_edit.replace_text(address);
        changed = true;
    }

    changed
}
