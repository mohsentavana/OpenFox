//! The browser chrome: tab strip, toolbar, loading line, and the page slot
//! the compositor's frame lands in. One column, flat surfaces, one accent.

use crate::findbar::FindBar;
use crate::newtab::NewTabState;
use crate::{findbar, shield, tabstrip, theme, toolbar};
use rune::{Color, El, LoopSpec, Transition, col, el, oklcha, text, tok};
use rune::tok::{motion, type_};

pub const CHROME_HEIGHT_TABS: f32 = 40.0;
pub const CHROME_HEIGHT_TOOLBAR: f32 = 52.0;

/// Which side surface the toolbar has raised. One enum because one toolbar
/// button cluster toggles them; the surfaces' own models stay in their
/// subsystems and render through their modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChromePanel {
    #[default]
    None,
    Downloads,
    Library,
    Settings,
}

/// What one tab looks like to the chrome. Titles arrive from navigation
/// events; an empty title shows the host, like every browser user expects.
#[derive(Debug, Clone, PartialEq)]
pub struct TabView {
    pub id: u64,
    pub title: String,
    pub address: String,
    pub loading: bool,
    pub status: Option<u16>,
}

impl TabView {
    pub fn loading(id: u64) -> TabView {
        TabView { id, title: "New tab".into(), address: String::new(), loading: false, status: None }
    }

    pub fn display_title(&self) -> String {
        if self.title.is_empty() {
            let host = self.address.trim_start_matches("https://").trim_start_matches("http://");
            let host = host.split('/').next().unwrap_or("New tab");
            host.to_owned()
        } else {
            self.title.clone()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShieldState {
    pub ads: u32,
    pub trackers: u32,
    pub requests: u32,
    pub saved_kb: u32,
    pub top_hosts: Vec<(String, u32)>,
}

/// What the chrome asks the browser to do. The window drains this queue
/// every frame and runs each intent against the backend; headless runs
/// assert on it directly. UI-local state (panel raised, find text) never
/// becomes an intent — only things the browser itself must do.
#[derive(Debug, Clone, PartialEq)]
pub enum ChromeIntent {
    /// Address-bar or quick-link navigation: a URL or a search query.
    Navigate(String),
    GoBack,
    GoForward,
    Reload,
    NewTab,
    CloseTab(u64),
    /// The user switched tabs: the browser's notion of the active tab
    /// follows, so Back/Forward/navigate act on what the user sees.
    SelectTab(u64),
}

#[derive(Debug, Clone)]
pub struct ChromeState {
    pub tabs: Vec<TabView>,
    pub active: usize,
    pub address: String,
    pub loading: bool,
    pub shield: ShieldState,
    pub shield_open: bool,
    /// Per-tab zoom of the active tab, for the toolbar indicator.
    pub zoom: f32,
    /// Whether the active page can enter reader mode.
    pub reader_available: bool,
    /// Whether the active page is shown in reader mode.
    pub reader_active: bool,
    /// Dark or light appearance. One field, whole-chrome re-skin.
    pub dark: bool,
    /// The active tab's find bar: the chrome's honest mirror of the engine's
    /// search, driven through `FindBar`'s own intents.
    pub find: FindBar,
    /// Which side surface the toolbar has raised.
    pub panel: ChromePanel,
    /// The new-tab page's own state: the search field and the quick links.
    pub newtab: NewTabState,
    /// The address bar's editor. Its text mirrors the active tab's URL
    /// while the user isn't editing; a submit becomes a `Navigate` intent.
    pub url_edit: rune::TextEdit,
    /// The intents raised since the last drain. The window owns draining.
    pub intents: Vec<ChromeIntent>,
}

impl Default for ChromeState {
    fn default() -> ChromeState {
        // OpenFox ships a dark chrome first; light is one field away.
        ChromeState {
            dark: true,
            find: FindBar::default(),
            panel: ChromePanel::None,
            newtab: NewTabState::default(),
            tabs: Vec::new(),
            active: 0,
            address: String::new(),
            loading: false,
            shield: ShieldState::default(),
            shield_open: false,
            zoom: 1.0,
            reader_available: false,
            reader_active: false,
            url_edit: rune::TextEdit::new(""),
            intents: Vec::new(),
        }
    }
}

impl ChromeState {
    pub fn active_tab(&self) -> Option<&TabView> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut TabView> {
        self.tabs.get_mut(self.active)
    }

    /// The next tab id: one past the highest seen, so keys never collide
    /// after closes.
    pub fn next_tab_id(&self) -> u64 {
        self.tabs.iter().map(|tab| tab.id).max().unwrap_or(0) + 1
    }
}

/// The whole browser chrome: tab strip, toolbar, loading line, page area.
/// Focus, roles and labels are real so Rune's headless accessibility tree
/// and agent input drive the same elements a user clicks.
pub fn view(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    col()
        .key("chrome")
        .w_full()
        .h_full()
        .fill(t.bg)
        .child(tabstrip::view(s))
        .child(toolbar::view(s))
        .child(loading_line(s))
        .child(page_area(s))
}

/// Where the page goes: the new-tab page while the tab is fresh, the
/// compositor's texture once the platform layer hands it over, and the
/// floating overlays (find bar, shield popover) pinned over it.
fn page_area(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let mut area = col()
        .key("page-area")
        .w_full()
        .grow(1.0)
        .fill(t.bg)
        .child(page_slot(s));
    if s.find.visible {
        area = area.child(findbar::view(s));
    }
    if s.shield_open {
        area = area.child(shield_panel_mounted(s));
    }
    area
}

fn page_slot(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let fresh = s.active_tab().map_or(true, |tab| tab.address.is_empty());
    if fresh {
        // The new-tab page is the page: same slot, own module.
        return crate::newtab::view(s);
    }
    let slot = el()
        .key("page")
        .role("region")
        .label("Page content")
        .w_full()
        .grow(1.0)
        .fill(if s.loading { t.control_track } else { t.bg });
    if s.loading {
        slot.child(
            el()
                .key("page-loading")
                .w_full()
                .grow(1.0)
                .items_center()
                .justify_center()
                .child(text("Loading").font(type_::BODY).color(t.text_secondary)),
        )
    } else {
        slot
    }
}

/// The 2 pt indeterminate progress line under the toolbar while a tab
/// loads: an accent segment sweeping a neutral track on Rune's shared loop
/// clock (`LoopSpec::slide`), fully declarative — no timers, and Reduce
/// Motion parks it at the sweep's midpoint.
fn loading_line(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let line = el().key("progress").w_full().h(2.0).fill(t.control_track);
    if s.loading {
        // The segment is 30% of the track; the sweep travels from fully off
        // the left edge to fully off the right in fractions of the segment's
        // own width (track = 10/3 segment widths).
        line.child(
            el()
                .key("progress-segment")
                .h(2.0)
                .w_frac(0.3)
                .fill(theme::accent(s.dark))
                .looping(LoopSpec::slide(1.4, [-1.0, 0.0], [3.4, 0.0])),
        )
    } else {
        line
    }
}

/// The shield popover, mounted over the page when open: a scrim that closes
/// on press, and the panel itself pinned under the toolbar's left edge —
/// anchored to the shield badge inside the URL bar, the way Brave anchors
/// its shields popover.
pub fn shield_panel_mounted(s: &ChromeState) -> El {
    let scrim = el()
        .key("shield-scrim")
        .role("button")
        .label("Close shield panel")
        .w_full()
        .h_full()
        .fill(oklcha(0.0, 0.0, 0.0, 0.32))
        .on_press(|s: &mut ChromeState| s.shield_open = false);
    el()
        .key("shield-mount")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .fill(Color::TRANSPARENT)
        .child(scrim)
        .child(
            shield::panel(s)
                .pin(Some(tok::space::S3), Some(tok::space::S1), None, None)
                .raise()
                .transition(Transition::offset(0.0, -4.0).combined(Transition::opacity()))
                .motion(motion::SNAPPY),
        )
}
