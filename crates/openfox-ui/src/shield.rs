//! The shield: the loudest element in the chrome, and it counts something
//! real — requests that never left the machine. Zero stays quiet on purpose;
//! nothing should glow when there is nothing to say.

use crate::chrome::ChromeState;
use crate::{icons, theme, widgets};
use rune::{Color, El, col, el, row, text, tok};
use rune::tok::{motion, type_};

const PANEL_WIDTH: f32 = 320.0;
const PANEL_ROWS: usize = 6;

/// The shield button inside the URL bar: a ghost icon button whose blocked
/// counter rides its top-right corner as an accent chip. The chip only
/// exists while something was blocked — zero stays quiet.
pub fn badge(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let accent = theme::accent(s.dark);
    let blocked = s.shield.ads + s.shield.trackers;
    let counting = blocked > 0;
    let label = if counting { format!("{blocked} requests blocked") } else { "No requests blocked".to_owned() };

    let mut button = el()
        .key("shield")
        .role("button")
        .label(label)
        .focusable(true)
        .size(26.0, 26.0)
        .radius(13.0)
        .fill(Color::TRANSPARENT)
        .hover(t.hover)
        .press(t.press)
        .motion(motion::SNAPPY)
        .items_center()
        .justify_center()
        .on_press(|s: &mut ChromeState| s.shield_open = !s.shield_open)
        .child(icons::glyph(&icons::SHIELD, 15.0, if counting { accent } else { t.text_secondary }));

    if counting {
        // "99+" keeps the chip a circle even on tracker-heavy pages.
        let count = if blocked > 99 { "99+".to_owned() } else { blocked.to_string() };
        button = button.child(
            el()
                .key("shield-count")
                .pin(Some(15.0), None, None, Some(15.0))
                .min_w(15.0)
                .h(15.0)
                .px(3.0)
                .radius(8.0)
                .fill(accent)
                .items_center()
                .justify_center()
                .child(text(count).font(type_::SECTION).color(theme::accent_ink(s.dark))),
        );
    }
    button
}

/// The popover behind the badge: what was blocked, where, and the bandwidth
/// it saved, on the raised card with hairline sections. Numbers only — no
/// scarecopy, no upsell, nothing green.
pub fn panel(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let accent = theme::accent(s.dark);
    let shield = &s.shield;
    let dark = s.dark;

    let mut panel = col()
        .key("shield-panel")
        .role("dialog")
        .label("Blocked requests on this page")
        .w(PANEL_WIDTH)
        .radius(tok::radius::LG)
        .fill(t.card)
        .border(1.0, t.card_border)
        .shadow(8.0, 24.0, t.shadow)
        .motion(motion::SNAPPY)
        .child(
            row()
                .key("shield-head")
                .px(tok::space::S3)
                .pt(tok::space::S3)
                .pb(tok::space::S2)
                .gap(tok::space::S2)
                .items_center()
                .child(icons::glyph(&icons::SHIELD, 16.0, accent))
                .child(text("Blocked on this page").font(type_::TITLE).color(t.text)),
        )
        .child(widgets::hairline(dark))
        .child(
            row()
                .key("shield-totals")
                .px(tok::space::S3)
                .py(tok::space::S2)
                .gap(tok::space::S6)
                .child(stat("Ads", shield.ads, dark))
                .child(stat("Trackers", shield.trackers, dark))
                .child(stat("Requests", shield.requests, dark)),
        );

    if shield.top_hosts.is_empty() {
        panel = panel.child(
            el()
                .key("shield-empty")
                .px(tok::space::S3)
                .pb(tok::space::S3)
                .child(text("Nothing blocked yet.").font(type_::BODY).color(t.text_secondary)),
        );
    } else {
        panel = panel.child(widgets::hairline(dark));
        for (host, count) in shield.top_hosts.iter().take(PANEL_ROWS) {
            panel = panel.child(
                row()
                    .key(format!("host-{}", host))
                    .px(tok::space::S3)
                    .py(7.0)
                    .gap(tok::space::S2)
                    .items_center()
                    .child(
                        text(host.clone())
                            .font(type_::LABEL)
                            .color(t.text)
                            .truncate()
                            .grow(1.0),
                    )
                    .child(text(count.to_string()).font(type_::MONO_CAPTION).color(accent)),
            );
        }
    }

    panel
        .child(widgets::hairline(dark))
        .child(
            el()
                .key("shield-foot")
                .px(tok::space::S3)
                .py(tok::space::S2)
                .child(
                    text(format!("{} KB saved across {} requests", shield.saved_kb, shield.requests))
                        .font(type_::MONO_CAPTION)
                        .color(t.text_secondary),
                ),
        )
}

/// One number with its caption, stacked tight.
fn stat(caption: &str, value: u32, dark: bool) -> El {
    let t = tok::theme(dark);
    col()
        .key(format!("stat-{}", caption.to_lowercase()))
        .gap(2.0)
        .child(text(value.to_string()).font(type_::TITLE).color(t.text))
        .child(text(caption).font(type_::SECTION).color(t.text_secondary))
}
