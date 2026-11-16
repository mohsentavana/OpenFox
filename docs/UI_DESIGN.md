# OpenFox design system (Rune chrome)

One accent, two appearances, a fixed spacing and radius scale — the chrome is
skinned from `crates/openfox-ui/src/theme.rs` and the widgets in
`crates/openfox-ui/src/widgets.rs`. Nothing picks a color by hand.

## Colour identity

The fox-orange accent is authored directly in OKLCH (`theme.rs`):

| Role | Dark chrome | Light chrome |
|---|---|---|
| Accent (fox orange) | `oklch(0.75 0.16 55)` | `oklch(0.63 0.19 45)` |
| Ink on accent | `oklch(0.17 0.02 55)` | `oklch(0.985 0.004 90)` |
| Neutrals | `tok::theme(true)` | `tok::theme(false)` |

Dark mode's orange is brighter (it must lift off a `oklch(0.185)` chrome);
light mode's is deeper and redder to hold ~4.5:1 text contrast on white. Every
accent use in the browser routes through `theme::accent(dark)` — active-tab
indicator, URL-bar focus ring, primary buttons, shield badge, loading sweep —
so a re-skin is a two-line change.

Neutrals (bg, card, card_border, text, text_secondary, separator, hover,
press, control_track, knob, shadows) come from Rune's `tok::theme(dark)`,
which keeps OpenFox's chrome in continuous calibration with the UI framework
its panels are built from.

## Scales

- **Space**: `tok::space` — S1 4 / S2 8 / S3 12 / S6 24 pt. Panels pad S3,
  rows pad S2, grids gap S2.
- **Radius**: `tok::radius` — SM 6 (menu items) / MD 8 (panels, rows) /
  LG 12 (floating surfaces). The URL bar and pills use their full height as
  radius (rounded-full).
- **Type**: `tok::type_` — TITLE (wordmark, tab titles on active), BODY
  (URL, row text), LABEL (dense rows), SECTION (captions, sidebar), MONO
  CAPTION (counts, speeds, match counters).
- **Motion**: `tok::motion::SNAPPY` for chrome state changes (hover, press,
  tab switches, panel slides); nothing in the chrome loops or oscillates.

## Chrome anatomy (top to bottom)

1. **Tab strip** (36 pt) — Brave-style detached rounded tabs on the chrome
   background. Active tab: card colour, hairline edge, soft shadow, 2 pt
   accent indicator underneath; inactive: flat, lightens on hover. Width
   flexes 120–240 pt with the window; the empty tail is the window-drag band
   (the strip is the title bar). Close button appears on hover/focus only.
   New-tab button: circular, ghost, accent-hover.
2. **Toolbar** (44 pt) — back/forward/reload icon buttons (32 pt circular,
   ghost), then the floating URL bar (36 pt tall, max 720 pt, rounded-full,
   card colour, hairline border, fox-orange focus ring that follows the
   corner radius). Right cluster: reader, downloads, library, settings.
3. **Loading line** — a 2 pt accent sweep directly under the toolbar while
   the active tab loads; indeterminate; gone when loaded.
4. **Content** — the page (or the new-tab page).
5. **Find bar** — bottom-floating rounded panel; match counter in MONO
   CAPTION; prev/next/close icon buttons; slides in on `SNAPPY`.

## The new-tab page (`newtab.rs`)

Wordmark in TITLE, a real Rune `TextEdit` search field (scripted input
reaches the actual editor), and eight quick-link tiles with two-letter
monograms, each tile tinted with its own OKLCH hue so the shelf reads as
distinct sites before any favicon loads. State is a plain `NewTabState`
owned by the chrome — pure views, headless-checkable.

## Panels

Library, settings, downloads, and reader share the same skeleton: sidebar
with section pills (`widgets::section_pill`), rows with hairline separators
(`widgets::hairline`), sticky headers, 12 pt padding. The shield panel
(`shield.rs`) shows the blocked-count badge in accent when non-zero.

## Accessibility rules

- Every interactive element is `.focusable(true)` with a visible focus ring.
- Every icon button carries a `.label(...)` for screen readers.
- Decorative dividers are `a11y_hidden`.
- Hover and press states are mandatory on anything pressable (`theme`
  hover/press overlays via `widgets::icon_button`).

## Icons (`icons.rs`)

A hand-drawn 24-unit stroke icon set (back, forward, reload, close, download,
reader, library, settings, shield, search, plus) — one file, `&'static Icon`
consts, sized per use with `icons::glyph(icon, size, color)`.
