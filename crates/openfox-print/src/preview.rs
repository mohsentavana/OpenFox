//! Print preview: what the user sees before the paper pays for a mistake.
//! Scale, page count, sheet navigation — all pure state.

use crate::paged::{PageBox, PageFormat};
use crate::PrintError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreviewScale {
    FitPage,
    ShrinkToWidth,
    Percent(f32),
}

pub struct PrintPreview {
    pages: Vec<PageBox>,
    current: usize,
    scale: PreviewScale,
    format: PageFormat,
}

impl PrintPreview {
    pub fn new(pages: Vec<PageBox>, format: PageFormat) -> Self {
        PrintPreview {
            pages,
            current: 0,
            scale: PreviewScale::FitPage,
            format,
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn current_page(&self) -> usize {
        self.current
    }

    pub fn page(&self) -> Result<&PageBox, PrintError> {
        self.pages
            .get(self.current)
            .ok_or(PrintError::PageRange(self.current as u32 + 1))
    }

    pub fn next_page(&mut self) -> bool {
        if self.current + 1 < self.pages.len() {
            self.current += 1;
            true
        } else {
            false
        }
    }

    pub fn prev_page(&mut self) -> bool {
        if self.current > 0 {
            self.current -= 1;
            true
        } else {
            false
        }
    }

    pub fn goto_page(&mut self, page: usize) -> Result<(), PrintError> {
        if page >= self.pages.len() {
            return Err(PrintError::PageRange(page as u32 + 1));
        }
        self.current = page;
        Ok(())
    }

    pub fn scale(&self) -> PreviewScale {
        self.scale
    }

    pub fn set_scale(&mut self, scale: PreviewScale) {
        if let PreviewScale::Percent(value) = scale {
            // 25%..200%: beyond that the preview stops previewing.
            self.scale = PreviewScale::Percent(value.clamp(0.25, 2.0));
        } else {
            self.scale = scale;
        }
    }

    pub fn format(&self) -> PageFormat {
        self.format
    }

    /// The render plan for preview: the visible sheet plus one on each
    /// side, so paging never shows a blank flash.
    pub fn render_plan(&self) -> Vec<usize> {
        let mut plan = vec![self.current];
        if self.current > 0 {
            plan.push(self.current - 1);
        }
        if self.current + 1 < self.pages.len() {
            plan.push(self.current + 1);
        }
        plan
    }
}
