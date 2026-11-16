//! Reader typography: themes, font stacks, and the layout math that keeps
//! prose comfortable at any size.

use serde::{Deserialize, Serialize};

/// Reader themes as concrete color triples. Dark is not inverted Light —
/// pure white on black haloes on every panel technology shipped this decade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReaderTheme {
    Light,
    Sepia,
    Dark,
}

impl ReaderTheme {
    /// (foreground, background, link): the link tint must read as a link
    /// on the background without competing with body text.
    pub const fn palette(self) -> (&'static str, &'static str, &'static str) {
        match self {
            ReaderTheme::Light => ("#1a1a1a", "#ffffff", "#0a58ca"),
            ReaderTheme::Sepia => ("#43372a", "#f8f1e3", "#8a6d3b"),
            ReaderTheme::Dark => ("#d8d8d8", "#181818", "#8ab4f8"),
        }
    }
}

/// Reader font choices with the stacks the UI installs. Platform fonts
/// first, generic fallback last — reader mode downloads no webfonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReaderFont {
    Serif,
    Sans,
    Mono,
}

impl ReaderFont {
    pub const fn stack(self) -> &'static str {
        match self {
            ReaderFont::Serif => "Georgia, 'Source Serif 4', 'Iowan Old Style', serif",
            ReaderFont::Sans => "system-ui, 'Segoe UI', 'Helvetica Neue', sans-serif",
            ReaderFont::Mono => "'Cascadia Mono', 'JetBrains Mono', Menlo, monospace",
        }
    }
}

/// Reader typography settings, clamped to the ranges that keep prose
/// readable: sizes outside 15–21px and measures past ~66ch measurably
/// hurt reading speed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReaderTypography {
    pub theme: ReaderTheme,
    pub font: ReaderFont,
    pub font_px: u8,
    pub measure_ch: u8,
    pub line_height: f32,
}

impl Default for ReaderTypography {
    /// Sepia serif at 17px: the least fatiguing combination across the
    /// long-form corpus reader mode was tuned against.
    fn default() -> Self {
        ReaderTypography {
            theme: ReaderTheme::Sepia,
            font: ReaderFont::Serif,
            font_px: 17,
            measure_ch: 62,
            line_height: 1.6,
        }
    }
}

/// Concrete layout numbers handed to the display layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LayoutParams {
    pub font_px: f32,
    pub line_height_px: f32,
    pub measure_ch: u8,
    pub fg: &'static str,
    pub bg: &'static str,
    pub link: &'static str,
}

impl ReaderTypography {
    /// Construct with clamps, not errors: user config round-trips through
    /// here and a stale file must not break the pane.
    pub fn new(theme: ReaderTheme, font: ReaderFont, font_px: u8, measure_ch: u8, line_height: f32) -> Self {
        ReaderTypography {
            theme,
            font,
            font_px: font_px.clamp(15, 21),
            measure_ch: measure_ch.clamp(30, 66),
            line_height: line_height.clamp(1.4, 2.0),
        }
    }

    /// One step bigger inside the clamp — the + key.
    pub fn grow(&mut self) {
        self.font_px = self.font_px.saturating_add(1).clamp(15, 21);
    }

    /// One step smaller inside the clamp — the - key.
    pub fn shrink(&mut self) {
        self.font_px = self.font_px.saturating_sub(1).max(15);
    }

    /// The numbers the pane paints with: font size, derived line height,
    /// measure in characters, and the theme's three colors.
    pub fn layout_params(&self) -> LayoutParams {
        let (fg, bg, link) = self.theme.palette();
        LayoutParams {
            font_px: self.font_px as f32,
            line_height_px: self.font_px as f32 * self.line_height,
            measure_ch: self.measure_ch,
            fg,
            bg,
            link,
        }
    }
}
