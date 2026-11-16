//! Paged media: page boxes, margins, breaks, orphans and widows. The
//! cascade already decided page-break-*; this module honors it.

use crate::PrintError;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum PageFormat {
    A4,
    Letter,
    Legal,
    A5,
}

impl PageFormat {
    /// Millimeter dimensions per ISO 216 / ANSI. Everything downstream
    /// converts to points (1 pt = 1/72 in, mm = 25.4/72 pt).
    pub fn size_mm(self) -> (f32, f32) {
        match self {
            PageFormat::A4 => (210.0, 297.0),
            PageFormat::A5 => (148.0, 210.0),
            PageFormat::Letter => (215.9, 279.4),
            PageFormat::Legal => (215.9, 355.6),
        }
    }

    pub fn size_pt(self) -> (f32, f32) {
        let (w, h) = self.size_mm();
        (mm_to_pt(w), mm_to_pt(h))
    }
}

pub fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Margins {
    pub top_pt: f32,
    pub right_pt: f32,
    pub bottom_pt: f32,
    pub left_pt: f32,
}

impl Default for Margins {
    fn default() -> Self {
        // 12.7 mm = half an inch: the default nobody complains about.
        let m = mm_to_pt(12.7);
        Margins {
            top_pt: m,
            right_pt: m,
            bottom_pt: m,
            left_pt: m,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageBox {
    pub index: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub margins: Margins,
    /// The header/footer strings an @page margin box or the print dialog
    /// asked for: title left, page number right.
    pub header: Option<String>,
    pub footer: Option<String>,
    /// Content: rectangles and glyph runs in page coordinates, already
    /// clipped to the page area.
    pub items: Vec<PageItem>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum PageItem {
    Rect { x: f32, y: f32, w: f32, h: f32, gray: f32 },
    GlyphRun { x: f32, y: f32, size: f32, font: String, text: String },
}

/// The box the paginator walks. The engine converts its layout tree into
/// these; this crate never sees the DOM.
#[derive(Debug, Clone)]
pub struct PagedBox {
    pub height_pt: f32,
    pub break_before: bool,
    pub break_after: bool,
    pub break_inside_avoid: bool,
    /// Lines within this box, for orphan/widow decisions.
    pub line_count: usize,
    pub items: Vec<PageItem>,
}

pub struct PagedMedia {
    pub format: PageFormat,
    pub margins: Margins,
    pub orphans: usize,
    pub widows: usize,
}

impl Default for PagedMedia {
    fn default() -> Self {
        // Orphans and widows default to 2 — the CSS default, kept because
        // it is the right default.
        PagedMedia {
            format: PageFormat::A4,
            margins: Margins::default(),
            orphans: 2,
            widows: 2,
        }
    }
}

impl PagedMedia {
    pub fn paginate(&self, boxes: &[PagedBox], title: &str) -> Result<Vec<PageBox>, PrintError> {
        if boxes.is_empty() {
            return Err(PrintError::EmptyDocument);
        }
        let (page_w, page_h) = self.format.size_pt();
        let content_h = page_h - self.margins.top_pt - self.margins.bottom_pt;
        let mut pages = Vec::new();
        let mut current: Vec<PageItem> = Vec::new();
        let mut used = 0.0f32;
        let mut page_index = 0u32;

        for (position, boxed) in boxes.iter().enumerate() {
            let is_last = position + 1 == boxes.len();
            let mut page_break = false;

            if boxed.break_before {
                page_break = true;
            } else if used + boxed.height_pt > content_h && !current.is_empty() {
                page_break = true;
                // Orphan/widow guard: leaving fewer than `orphans` lines
                // behind or pushing fewer than `widows` ahead both move
                // the whole box to the next page.
                let lines_left_in_page = boxed.line_count;
                if lines_left_in_page < self.orphans.max(self.widows)
                    && used + boxed.height_pt < content_h * 1.2
                {
                    page_break = boxed.break_inside_avoid;
                }
            }

            if page_break {
                pages.push(self.finish_page(page_index, page_w, page_h, title, current.clone()));
                page_index += 1;
                current.clear();
                used = 0.0;
            }

            current.extend(boxed.items.iter().cloned());
            used += boxed.height_pt;

            if boxed.break_after && !is_last {
                pages.push(self.finish_page(page_index, page_w, page_h, title, current.clone()));
                page_index += 1;
                current.clear();
                used = 0.0;
            }
        }

        if !current.is_empty() || pages.is_empty() {
            pages.push(self.finish_page(page_index, page_w, page_h, title, current));
        }
        Ok(pages)
    }

    fn finish_page(
        &self,
        index: u32,
        w: f32,
        h: f32,
        title: &str,
        items: Vec<PageItem>,
    ) -> PageBox {
        let total = index + 1;
        PageBox {
            index,
            width_pt: w,
            height_pt: h,
            margins: self.margins,
            header: if title.is_empty() {
                None
            } else {
                Some(title.to_owned())
            },
            footer: Some(format!("{}/{}", index + 1, total)),
            items,
        }
    }
}
