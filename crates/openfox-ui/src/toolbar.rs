//! The toolbar: back / forward / reload, the floating URL bar, and the
//! right-aligned cluster (reader, downloads, library, settings). The URL bar
//! is the hero: rounded-full on the card colour with a hairline border, and
//! a fox-orange focus ring that follows the field's own rounded corners.

use crate::chrome::{ChromeIntent, ChromePanel, ChromeState, CHROME_HEIGHT_TOOLBAR};
use crate::{icons, shield, theme, widgets};
use rune::{El, edit_access, el, icon, row, text, text_input, tok};
use rune::tok::{motion, type_};

const URL_H: f32 = 36.0;
const URL_MAX_W: f32 = 720.0;
const CLUSTER_GAP: f32 = 8.0;

pub fn view(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    row()
        .key("toolbar")
        .w_full()
        .h(CHROME_HEIGHT_TOOLBAR)
        .px(tok::space::S3)
        .gap(tok::space::S2)
        .items_center()
        .fill(t.bg)
        .border_sides([0.0, 0.0, 1.0, 0.0], t.separator)
        .child(widgets::icon_button("back", &icons::BACK, "Go back", s.dark, true, |s: &mut ChromeState| {
            s.loading = true;
            s.intents.push(ChromeIntent::GoBack);
        }))
        .child(widgets::icon_button("forward", &icons::FORWARD, "Go forward", s.dark, true, |s: &mut ChromeState| {
            s.loading = true;
            s.intents.push(ChromeIntent::GoForward);
        }))
        .child(widgets::icon_button("reload", &icons::RELOAD, "Reload", s.dark, true, |s: &mut ChromeState| {
            s.loading = true;
            s.intents.push(ChromeIntent::Reload);
        }))
        .child(url_bar(s))
        .child(el().key("toolbar-divider").w(1.0).h(24.0).fill(t.separator).a11y_hidden())
        .child(right_cluster(s))
}

/// The floating URL bar: shield badge and site-identity glyph leading, the
/// address trailing. Focus draws the accent ring just outside the pill
/// (`focus_within_ring`), so the ring is visible for pointer and keyboard
/// focus alike.
fn url_bar(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let accent = theme::accent(s.dark);
    let secure = s.address.starts_with("https://");
    let identity = if secure { &icons::LOCK } else { &icons::GLOBE };

    row()
        .key("address")
        .grow(1.0)
        .max_w(URL_MAX_W)
        .h(URL_H)
        .radius(URL_H / 2.0)
        .px(tok::space::S3)
        .gap(tok::space::S2)
        .items_center()
        .fill(t.card)
        .border(1.0, t.card_border)
        .shadow(1.0, 6.0, t.shadow_soft)
        .focus_within_ring(2.0, 2.0, accent)
        .motion(motion::SNAPPY)
        .on_press(|s: &mut ChromeState| s.shield_open = false)
        .child(shield::badge(s))
        .child(icon(identity, 13.0).color(t.text_secondary).a11y_hidden())
        .child(
            text_input(&s.url_edit, type_::BODY)
                .grow(1.0)
                .color(t.text)
                .placeholder("Search or enter address", t.text_secondary)
                .caret(accent, accent.alpha(0.3))
                .on_edit(edit_access(|s: &mut ChromeState| &mut s.url_edit))
                .on_submit(|s: &mut ChromeState, _| {
                    let query = s.url_edit.text.trim().to_owned();
                    if !query.is_empty() {
                        s.address = query.clone();
                        s.loading = true;
                        s.intents.push(ChromeIntent::Navigate(query));
                    }
                }),
        )
}

/// Reader, downloads, library, settings: 32 pt icon buttons, 8 pt gaps,
/// pressed state carried by `checked` so agents read the open panel too.
fn right_cluster(s: &ChromeState) -> El {
    let dark = s.dark;
    let reader_on = s.reader_active;
    row()
        .key("toolbar-cluster")
        .gap(CLUSTER_GAP)
        .items_center()
        .child(
            widgets::icon_button(
                "reader",
                &icons::READER,
                "Reader view",
                dark,
                s.reader_available,
                move |s: &mut ChromeState| s.reader_active = !reader_on,
            )
            .checked(reader_on),
        )
        .child(panel_button(s, "downloads", &icons::DOWNLOAD, "Downloads", ChromePanel::Downloads))
        .child(panel_button(s, "library", &icons::LIBRARY, "Library", ChromePanel::Library))
        .child(panel_button(s, "settings", &icons::SETTINGS, "Settings", ChromePanel::Settings))
}

/// A cluster button that toggles one side surface; `checked` reports whether
/// that surface is the raised one.
fn panel_button(s: &ChromeState, key: &'static str, glyph: &'static rune::Icon, label: &str, panel: ChromePanel) -> El {
    let raised = s.panel == panel;
    widgets::icon_button(key, glyph, label, s.dark, true, move |s: &mut ChromeState| {
        s.panel = if s.panel == panel { ChromePanel::None } else { panel };
    })
    .checked(raised)
}
