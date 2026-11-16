//! The reader surface: theme, font, size and width controls, read-time
//! estimate, and per-site auto-enable.

use std::collections::HashSet;

/// Reader themes cycle Light -> Sepia -> Dark. Cycling instead of a
/// dropdown keeps the control one click; three themes is the whole honest
/// set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderTheme {
    Light,
    Sepia,
    Dark,
}

impl ReaderTheme {
    pub fn next(self) -> ReaderTheme {
        match self {
            ReaderTheme::Light => ReaderTheme::Sepia,
            ReaderTheme::Sepia => ReaderTheme::Dark,
            ReaderTheme::Dark => ReaderTheme::Light,
        }
    }

    pub fn css_key(self) -> &'static str {
        match self {
            ReaderTheme::Light => "reader.theme.light",
            ReaderTheme::Sepia => "reader.theme.sepia",
            ReaderTheme::Dark => "reader.theme.dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderFont {
    Serif,
    Sans,
    Mono,
}

impl ReaderFont {
    pub fn next(self) -> ReaderFont {
        match self {
            ReaderFont::Serif => ReaderFont::Sans,
            ReaderFont::Sans => ReaderFont::Mono,
            ReaderFont::Mono => ReaderFont::Serif,
        }
    }

    pub fn css_key(self) -> &'static str {
        match self {
            ReaderFont::Serif => "reader.font.serif",
            ReaderFont::Sans => "reader.font.sans",
            ReaderFont::Mono => "reader.font.mono",
        }
    }
}

/// Size steps above the base font, bounded so the - and + buttons always
/// saturate visibly instead of wrapping a layout.
pub const SIZE_MIN: u8 = 1;
pub const SIZE_MAX: u8 = 8;

/// The controls strip at the bottom of reader mode.
#[derive(Debug, Clone)]
pub struct ReaderControls {
    pub theme: ReaderTheme,
    pub font: ReaderFont,
    pub size: u8,
    pub wide: bool,
}

impl Default for ReaderControls {
    fn default() -> ReaderControls {
        // Sepia and serif are the reading defaults: long text on white is
        // glare, long text in mono is work.
        ReaderControls { theme: ReaderTheme::Sepia, font: ReaderFont::Serif, size: 3, wide: false }
    }
}

/// What a reader control asks the engine's reader mode to do. The surface
/// applies the intent to its own state immediately; the engine mirrors it
/// into the re-extracted page so the next paint already has the new
/// metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderIntent {
    CycleTheme,
    CycleFont,
    Larger,
    Smaller,
    ToggleWidth,
    SaveToList,
    ToggleAuto { origin: String },
}

/// One reader session's state.
#[derive(Debug, Clone)]
pub struct ReaderSurface {
    pub controls: ReaderControls,
    /// Estimated minutes, recomputed with every extraction.
    pub read_time_min: u32,
    /// Origins where reader mode turns itself on once the article parse
    /// succeeds. Learned from the user's own ToggleAuto, never from a
    /// remote list.
    pub auto_origins: HashSet<String>,
}

/// Words per minute for adult reading of expository web text. The estimate
/// is a floor of one minute: "0 minutes" for a 180-word page reads as a
/// lie.
pub fn read_time_estimate(words: usize) -> u32 {
    (words as f32 / 220.0).ceil() as u32
}

impl ReaderSurface {
    pub fn new(read_time_min: u32) -> ReaderSurface {
        ReaderSurface { controls: ReaderControls::default(), read_time_min, auto_origins: HashSet::new() }
    }

    /// Applies one control intent to the surface. Returns true when
    /// something changed — the caller only re-extracts the page on a real
    /// change.
    pub fn apply(&mut self, intent: &ReaderIntent) -> bool {
        match intent {
            ReaderIntent::CycleTheme => {
                self.controls.theme = self.controls.theme.next();
                true
            }
            ReaderIntent::CycleFont => {
                self.controls.font = self.controls.font.next();
                true
            }
            ReaderIntent::Larger => {
                if self.controls.size < SIZE_MAX {
                    self.controls.size += 1;
                    true
                } else {
                    false
                }
            }
            ReaderIntent::Smaller => {
                if self.controls.size > SIZE_MIN {
                    self.controls.size -= 1;
                    true
                } else {
                    false
                }
            }
            ReaderIntent::ToggleWidth => {
                self.controls.wide = !self.controls.wide;
                true
            }
            ReaderIntent::SaveToList => true,
            ReaderIntent::ToggleAuto { origin } => {
                if !self.auto_origins.remove(origin) {
                    self.auto_origins.insert(origin.clone());
                }
                true
            }
        }
    }
}
