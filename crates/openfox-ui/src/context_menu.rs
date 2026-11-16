//! The context menu: a hit-test result becomes a list of typed menu items
//! with l10n keys and single-letter mnemonics. No English strings live here.

/// What the pointer hit, from the engine's hit test. Every variant carries
/// exactly what its menu items need — no more.
#[derive(Debug, Clone)]
pub enum HitTarget {
    Link { url: String, text: String },
    Image { url: String, alt: String },
    Media { url: String, kind: MediaKind, paused: bool },
    Selection { text: String },
    Editable { has_selection: bool },
    Page { x: f32, y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
}

/// What choosing a menu item asks the browser to do. One flat enum: the
/// command bus carries it, the engine executes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuIntent {
    OpenLinkNewTab,
    CopyLink,
    SaveImage,
    CopyImage,
    TogglePlay,
    ToggleMute,
    Copy,
    Cut,
    Paste,
    SearchSelection,
    Back,
    Forward,
    Reload,
    Inspect,
}

/// One menu row. `key` is the l10n key the renderer resolves; `mnemonic`
/// is the Latin letter underlined in that string.
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub key: &'static str,
    pub mnemonic: char,
    pub intent: MenuIntent,
    pub enabled: bool,
    pub submenu: Option<Submenu>,
}

/// A nested menu (audio/video qualities, engine spellings). One level only:
/// nested submenus inside a context menu are a usability bug, not a feature.
#[derive(Debug, Clone)]
pub struct Submenu {
    pub key: &'static str,
    pub items: Vec<MenuItem>,
}

/// Where the menu anchors, in page space.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub at: (f32, f32),
    pub items: Vec<MenuItem>,
}

/// Back/forward availability, so history items gray out honestly instead of
/// no-opping when clicked.
#[derive(Debug, Clone, Copy)]
pub struct NavState {
    pub can_back: bool,
    pub can_forward: bool,
}

impl ContextMenu {
    /// Builds the menu for one hit target. Inspect rides along on every
    /// target because devtools must always be one right-click away.
    pub fn build(target: &HitTarget, at: (f32, f32), nav: NavState) -> ContextMenu {
        let mut items = Vec::new();
        match target {
            HitTarget::Link { .. } => {
                items.push(item("ctx.open_new_tab", 'u', MenuIntent::OpenLinkNewTab, true));
                items.push(item("ctx.copy_link", 'l', MenuIntent::CopyLink, true));
            }
            HitTarget::Image { .. } => {
                items.push(item("ctx.save_image", 'd', MenuIntent::SaveImage, true));
                items.push(item("ctx.copy_image", 'g', MenuIntent::CopyImage, true));
            }
            HitTarget::Media { paused, .. } => {
                let play_key = if *paused { "ctx.play" } else { "ctx.pause" };
                items.push(item(play_key, 'p', MenuIntent::TogglePlay, true));
                items.push(item("ctx.mute", 'm', MenuIntent::ToggleMute, true));
                items.push(item("ctx.copy_link", 'l', MenuIntent::CopyLink, true));
            }
            HitTarget::Selection { text } => {
                let has = !text.is_empty();
                items.push(item("ctx.copy", 'c', MenuIntent::Copy, has));
                items.push(item("ctx.search_selection", 's', MenuIntent::SearchSelection, has));
            }
            HitTarget::Editable { has_selection } => {
                items.push(item("ctx.copy", 'c', MenuIntent::Copy, *has_selection));
                items.push(item("ctx.cut", 't', MenuIntent::Cut, *has_selection));
                items.push(item("ctx.paste", 'v', MenuIntent::Paste, true));
            }
            HitTarget::Page { .. } => {
                items.push(item("ctx.back", 'b', MenuIntent::Back, nav.can_back));
                items.push(item("ctx.forward", 'f', MenuIntent::Forward, nav.can_forward));
                items.push(item("ctx.reload", 'r', MenuIntent::Reload, true));
            }
        }
        items.push(item("ctx.inspect", 'i', MenuIntent::Inspect, true));
        ContextMenu { at, items }
    }
}

fn item(key: &'static str, mnemonic: char, intent: MenuIntent, enabled: bool) -> MenuItem {
    MenuItem { key, mnemonic, intent, enabled, submenu: None }
}
