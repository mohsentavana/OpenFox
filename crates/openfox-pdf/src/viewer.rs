//! The viewer: page navigation, zoom, find, and a progressive render
//! plan. The viewer owns none of the pixels — it decides what gets
//! rendered next and at what scale.

use crate::PdfError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomMode {
    FitWidth,
    FitPage,
    Custom(f32),
}

#[derive(Debug, Clone)]
pub struct FindMatch {
    pub page: u32,
    pub index: usize,
}

pub struct PdfViewer {
    pages: u32,
    current: u32,
    zoom: ZoomMode,
}

impl PdfViewer {
    pub fn new(pages: u32) -> Result<Self, PdfError> {
        if pages == 0 {
            return Err(PdfError::PageBounds(0));
        }
        Ok(PdfViewer {
            pages,
            current: 1,
            zoom: ZoomMode::FitWidth,
        })
    }

    pub fn page(&self) -> u32 {
        self.current
    }

    pub fn zoom(&self) -> ZoomMode {
        self.zoom
    }

    pub fn set_zoom(&mut self, zoom: ZoomMode) {
        if let ZoomMode::Custom(scale) = zoom {
            // Clamp to the 10%..640% band Firefox offers — beyond that the
            // display list explodes quadratically for no reading value.
            self.zoom = ZoomMode::Custom(scale.clamp(0.1, 6.4));
        } else {
            self.zoom = zoom;
        }
    }

    pub fn next(&mut self) -> bool {
        if self.current < self.pages {
            self.current += 1;
            true
        } else {
            false
        }
    }

    pub fn prev(&mut self) -> bool {
        if self.current > 1 {
            self.current -= 1;
            true
        } else {
            false
        }
    }

    pub fn goto(&mut self, page: u32) -> Result<(), PdfError> {
        if page == 0 || page > self.pages {
            return Err(PdfError::PageBounds(page));
        }
        self.current = page;
        Ok(())
    }

    /// Which pages to render right now: the visible one and the one
    /// after it. Pre-rendering further ahead wastes memory on documents
    /// nobody finishes reading.
    pub fn render_plan(&self) -> Vec<u32> {
        let mut plan = vec![self.current];
        if self.current < self.pages {
            plan.push(self.current + 1);
        }
        plan
    }

    /// Case-insensitive ASCII find over per-page extracted text. Real
    /// Unicode folding happens in the engine's text layer; PDFs are the
    /// ASCII-heavy corner of the web.
    pub fn find_in_document(&self, query: &str, page_text: &[String]) -> Vec<FindMatch> {
        let needle = query.to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        for (index, text) in page_text.iter().enumerate() {
            let haystack = text.to_ascii_lowercase();
            let mut from = 0;
            while let Some(found) = haystack[from..].find(&needle) {
                matches.push(FindMatch {
                    page: index as u32 + 1,
                    index: from + found,
                });
                from += found + needle.len().max(1);
            }
        }
        matches
    }
}
