//! The tab strip: Brave-style detached rounded tabs. Active tabs are raised
//! onto the card colour with a hairline edge and a soft shadow; inactive tabs
//! sit flat on the chrome and lighten on hover. The close button belongs to
//! the tab's hover group, so it appears only while the tab is hovered or
//! keyboard-focused — no state, no timers, Rune's own hover tracking.

use crate::chrome::{ChromeIntent, ChromeState, TabView, CHROME_HEIGHT_TABS};
use crate::{icons, theme, widgets};
use rune::{Color, El, el, icon, row, spacer, text, tok};
use rune::tok::{motion, type_};

const TAB_MIN_W: f32 = 120.0;
const TAB_MAX_W: f32 = 240.0;
const TAB_H: f32 = 30.0;
const TAB_PAD_X: f32 = 10.0;
const CLOSE_SIZE: f32 = 18.0;

/// The strip itself: one row of flexing tabs (min 120, max 240 pt), the
/// circular new-tab button beside them, and the empty tail doubling as a
/// window-drag band — the tab strip is the title bar.
pub fn view(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let count = s.tabs.len();
    let mut strip = row()
        .key("tabstrip")
        .w_full()
        .h(CHROME_HEIGHT_TABS)
        .px(tok::space::S2)
        .gap(6.0)
        .items_center()
        .fill(t.bg);

    for (index, tab) in s.tabs.iter().enumerate() {
        strip = strip.child(tab_view(tab, index == s.active, index, count, s.dark));
    }

    strip
        .child(new_tab_button(s))
        .child(spacer().key("tabstrip-tail").window_drag().a11y_hidden())
}

/// One detached tab pill.
fn tab_view(tab: &TabView, active: bool, index: usize, count: usize, dark: bool) -> El {
    let t = tok::theme(dark);
    let accent = theme::accent(dark);
    let tab_id = tab.id;
    let title = tab.display_title();

    let title_ink = if active { t.text } else { t.text_secondary };
    let close = el()
        .key("tab-close")
        .role("button")
        .label(format!("Close {title}"))
        .focusable(true)
        .size(CLOSE_SIZE, CLOSE_SIZE)
        .radius(CLOSE_SIZE / 2.0)
        .fill(Color::TRANSPARENT)
        // Rests invisible; the tab's hover group (and keyboard focus on it)
        // brings it to full opacity.
        .group_opacity(0.0, 1.0)
        .hover(t.hover)
        .press(t.press)
        .motion(motion::SNAPPY)
        .items_center()
        .justify_center()
        .on_press(move |s: &mut ChromeState| {
            // Local rows drop instantly; the browser closes the real tab
            // from the intent, and the next sync confirms it.
            s.tabs.retain(|view| view.id != tab_id);
            if s.active >= s.tabs.len() {
                s.active = s.tabs.len().saturating_sub(1);
            }
            s.intents.push(ChromeIntent::CloseTab(tab_id));
        })
        .child(icon(&icons::CLOSE, 9.0).color(title_ink).a11y_hidden());

    let mut shell = row()
        .key(format!("tab-{}", tab.id))
        .role("tab")
        .label(title.clone())
        .checked(active)
        .focusable(true)
        .position_in_set(index + 1, count)
        .min_w(TAB_MIN_W)
        .max_w(TAB_MAX_W)
        .grow(1.0)
        .h(TAB_H)
        .radius(tok::radius::MD)
        .px(TAB_PAD_X)
        .gap(tok::space::S2)
        .items_center()
        // The hover group lights the close button and this tab's hover fill.
        .hover_group()
        .fill(if active { t.card } else { Color::TRANSPARENT })
        .hover(if active { Color::TRANSPARENT } else { t.hover })
        .press(t.press)
        .motion(motion::SNAPPY)
        .on_press(move |s: &mut ChromeState| {
            s.active = index.min(s.tabs.len().saturating_sub(1));
            if let Some(tab) = s.tabs.get(s.active) {
                let id = tab.id;
                s.intents.push(ChromeIntent::SelectTab(id));
            }
        })
        .child(text(title).font(type_::LABEL).color(title_ink).truncate())
        .child(spacer().a11y_hidden())
        .child(close);

    if active {
        // Raised card: hairline edge, soft drop shadow, and the 2 pt accent
        // indicator riding the bottom edge, inset like Brave's.
        shell = shell
            .border(1.0, t.card_border)
            .shadow(1.0, 6.0, t.shadow_soft)
            .child(
                el()
                    .key("tab-indicator")
                    .h(2.0)
                    .pin(Some(tok::space::S3), None, Some(tok::space::S3), Some(0.0))
                    .fill(accent),
            );
    }
    shell
}

/// The new-tab affordance: a circular accent button, the one saturated
/// element on an otherwise neutral strip.
fn new_tab_button(s: &ChromeState) -> El {
    let next_id = s.next_tab_id();
    widgets::accent_button("new-tab", &icons::PLUS, "New tab", s.dark, move |s: &mut ChromeState| {
        s.tabs.push(TabView::loading(next_id));
        s.active = s.tabs.len() - 1;
        s.intents.push(ChromeIntent::NewTab);
    })
}
