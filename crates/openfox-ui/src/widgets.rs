//! Shared chrome widgets: the icon button every surface reuses, the hairline
//! rule that separates rows, and the section pill the sidebars navigate with.
//! One widget, one look — the consistency pass lives here.

use crate::icons;
use crate::theme;
use rune::{Color, El, Icon, el, rule, text, tok};
use rune::tok::{motion, type_};

/// The standard 32 pt circular icon button: ghost at rest (it sits on
/// whatever surface its host paints), theme hover and press overlays, a
/// 0.94 press-scale, and a visible focus ring in the fox accent.
///
/// `enabled == false` disables it honestly: out of the Tab order, dimmed,
/// press handler dropped (Rune's `disabled` contract).
pub fn icon_button<S: 'static>(
    key: impl Into<String>,
    glyph: &'static Icon,
    label: &str,
    dark: bool,
    enabled: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let t = tok::theme(dark);
    let mut button = el()
        .key(key)
        .role("button")
        .label(label)
        .focusable(enabled)
        .size(32.0, 32.0)
        .radius(16.0)
        .fill(Color::TRANSPARENT)
        .hover(t.hover)
        .press(t.press)
        .press_scale(0.94)
        .motion(motion::SNAPPY)
        .focus_ring(2.0, 2.0, theme::accent(dark))
        .items_center()
        .justify_center()
        .child(icons::glyph(glyph, 15.0, ink_for(dark)));
    if enabled {
        button = button.on_press(on_press);
    } else {
        button = button.disabled(true).dim(0.45);
    }
    button
}

/// The accent-filled circular action button (the new-tab affordance): fox
/// orange fill, accent-ink glyph, ink-wash hover/press overlays.
pub fn accent_button<S: 'static>(
    key: impl Into<String>,
    glyph: &'static Icon,
    label: &str,
    dark: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let wash = |a: f32| theme::accent_wash(dark, a);
    el()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .size(28.0, 28.0)
        .radius(14.0)
        .fill(theme::accent(dark))
        .hover(wash(0.16))
        .press(wash(0.28))
        .press_scale(0.92)
        .motion(motion::SNAPPY)
        .focus_ring(2.0, 2.0, theme::accent(dark))
        .items_center()
        .justify_center()
        .on_press(on_press)
        .child(icons::glyph(glyph, 14.0, theme::accent_ink(dark)))
}

/// A one-device-pixel hairline in the theme's separator colour: the same rule
/// under every list row, panel section and toolbar edge.
pub fn hairline(dark: bool) -> El {
    rule(1.0, tok::theme(dark).separator)
}

/// A sidebar's section pill: rounded-full, flat while inactive with the
/// secondary ink, raised onto the card colour with a hairline border while
/// active. `role("tab")` + `checked` keeps the switch honest for screen
/// readers and scripted agents.
pub fn section_pill<S: 'static>(
    key: impl Into<String>,
    label: &str,
    active: bool,
    dark: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let t = tok::theme(dark);
    let mut pill = el()
        .key(key)
        .role("tab")
        .label(label)
        .checked(active)
        .focusable(true)
        .h(26.0)
        .px(10.0)
        .radius(13.0)
        .fill(if active { t.card } else { Color::TRANSPARENT })
        .hover(t.hover)
        .press(t.press)
        .motion(motion::SNAPPY)
        .items_center()
        .on_press(on_press)
        .child(text(label).font(type_::LABEL).color(if active { t.text } else { t.text_secondary }));
    if active {
        pill = pill.border(1.0, t.card_border);
    }
    pill
}

/// The ink an icon button's glyph draws in: the theme's secondary text, which
/// reads as chrome furniture rather than content.
fn ink_for(dark: bool) -> Color {
    tok::theme(dark).text_secondary
}

/// A small ghost text button (panel actions, dialog declines): rounded-full,
/// theme hover/press, `LABEL` ink. Chain `.default_button()` on the caller's
/// side when it owns Return.
pub fn text_button<S: 'static>(
    key: impl Into<String>,
    label: &str,
    dark: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let t = tok::theme(dark);
    el()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .h(28.0)
        .px(14.0)
        .radius(14.0)
        .fill(Color::TRANSPARENT)
        .hover(t.hover)
        .press(t.press)
        .press_scale(0.96)
        .motion(motion::SNAPPY)
        .focus_ring(2.0, 2.0, theme::accent(dark))
        .items_center()
        .justify_center()
        .on_press(on_press)
        .child(text(label).font(type_::LABEL).color(t.text))
}

/// The primary (accent-filled) button: dialog confirms and the one action a
/// surface wants taken. Fox-orange fill, accent-ink label, ink-wash overlays.
pub fn primary_button<S: 'static>(
    key: impl Into<String>,
    label: &str,
    dark: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let wash = |a: f32| theme::accent_wash(dark, a);
    el()
        .key(key)
        .role("button")
        .label(label)
        .focusable(true)
        .h(30.0)
        .px(16.0)
        .radius(15.0)
        .fill(theme::accent(dark))
        .hover(wash(0.14))
        .press(wash(0.26))
        .press_scale(0.96)
        .motion(motion::SNAPPY)
        .focus_ring(2.0, 2.0, theme::accent(dark))
        .items_center()
        .justify_center()
        .on_press(on_press)
        .child(text(label).font(type_::LABEL).color(theme::accent_ink(dark)))
}

/// The sticky section header every scrolling panel uses: a shadow only while
/// it is stuck (`el().sticky_top(0.0).stuck_shadow(sticky_header(dark))`).
pub fn sticky_header(dark: bool) -> rune::Shadow {
    rune::Shadow { y: 2.0, blur: 8.0, color: tok::theme(dark).shadow_soft, ..rune::Shadow::default() }
}
