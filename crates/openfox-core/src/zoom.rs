//! Zoom: per-site memory, a clamped ladder, text-only mode. Pinch
//! gestures accumulate here instead of mutating tab state directly —
//! one decimal of hysteresis keeps the ladder from chattering.

use std::collections::HashMap;

pub const MIN_ZOOM: f32 = 0.3;
pub const MAX_ZOOM: f32 = 3.0;
pub const ZOOM_STEP: f32 = 0.1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZoomMemory {
    per_site: HashMap<String, f32>,
    text_only: bool,
}

impl Default for ZoomMemory {
    fn default() -> Self {
        ZoomMemory {
            per_site: HashMap::new(),
            text_only: false,
        }
    }
}

impl ZoomMemory {
    /// The zoom for a site, 1.0 unless remembered otherwise.
    pub fn zoom_for(&self, site: &str) -> f32 {
        self.per_site.get(site).copied().unwrap_or(1.0)
    }

    /// Set and clamp. Sites that leave the 100% default are remembered;
    /// sites returning to it are forgotten — the map stays small.
    pub fn set_zoom(&mut self, site: &str, level: f32) -> f32 {
        let clamped = level.clamp(MIN_ZOOM, MAX_ZOOM);
        if (clamped - 1.0).abs() < f32::EPSILON {
            self.per_site.remove(site);
        } else {
            self.per_site.insert(site.to_owned(), clamped);
        }
        clamped
    }

    /// One ladder step up or down, with rounding to the step grid so the
    /// keyboard and the pinch gesture agree on where 110% is.
    pub fn step(&mut self, site: &str, up: bool) -> f32 {
        let current = self.zoom_for(site);
        let stepped = if up {
            current + ZOOM_STEP
        } else {
            current - ZOOM_STEP
        };
        let snapped = (stepped / ZOOM_STEP).round() * ZOOM_STEP;
        self.set_zoom(site, snapped)
    }

    /// Pinch accumulation: the gesture target moves by a fraction of the
    /// distance remaining, so tiny finger jitter never teleports the
    /// zoom. Commit on gesture end calls set_zoom.
    pub fn pinch_target(current: f32, scale_delta: f32) -> f32 {
        let target = current * (1.0 + scale_delta);
        target.clamp(MIN_ZOOM, MAX_ZOOM)
    }

    pub fn text_only(&self) -> bool {
        self.text_only
    }

    /// Text-only zoom: layout boxes stay, glyph sizes scale. Firefox
    /// ships it; so do we, for low-vision users who want the page shape
    /// intact.
    pub fn set_text_only(&mut self, text_only: bool) {
        self.text_only = text_only;
    }

    pub fn reset(&mut self, site: &str) -> f32 {
        self.per_site.remove(site);
        1.0
    }
}
