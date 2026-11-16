//! OpenFox's colour identity: one fox-orange accent over Rune's neutral
//! themes. Every accent use in the chrome (active-tab indicator, focus rings,
//! primary buttons, the shield badge, the loading sweep) goes through
//! [`accent`], so the whole browser re-skins from this file.
//!
//! Rune ships six palette accents (`tok::accent`); index 3 is an orange, but
//! at `oklch(0.66..0.74, 0.16, 58)` it reads as a UI orange. OpenFox's fox is
//! warmer and lighter in the dark chrome, deeper on paper-white in the light
//! one, so both are authored directly in OKLCH below.

use rune::{Color, oklch};

/// The fox-orange accent for one appearance.
///
/// * Dark: `oklch(0.75 0.16 55)` — a bright, warm fox orange that lifts off
///   the `oklch(0.185)` chrome without a glow.
/// * Light: `oklch(0.63 0.19 45)` — a deeper, slightly redder orange that
///   keeps ~4.5:1 contrast against text on white surfaces.
pub fn accent(dark: bool) -> Color {
    if dark { oklch(0.75, 0.16, 55.0) } else { oklch(0.63, 0.19, 45.0) }
}

/// Ink that sits **on** the accent: warm near-black on the bright dark-mode
/// orange, paper white on the deeper light-mode orange. Both pairs carry the
/// button label at readable contrast.
pub fn accent_ink(dark: bool) -> Color {
    if dark { oklch(0.17, 0.02, 55.0) } else { oklch(0.985, 0.004, 90.0) }
}

/// A translucent accent wash for hover/press overlays on accent-filled
/// controls (the circular new-tab button), where the theme's neutral hover
/// would grey the fox out.
pub fn accent_wash(dark: bool, alpha: f32) -> Color {
    accent_ink(dark).alpha(alpha)
}
