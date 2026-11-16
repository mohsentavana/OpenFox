//! The PDF viewer: an in-house object model — xref, objects, content
//! streams — feeding the same display list the HTML engine paints. No
//! out-of-process plugin, no third-party runtime.
//!
//! Glyph runs leave this crate as (font, size, matrix, text); shaping and
//! rasterization belong to `openfox-render`, which already does both for
//! HTML.

pub mod outline;
pub mod render;
pub mod viewer;

pub use outline::{Destination, LinkAnnotation, OutlineNode};
pub use render::{parse_xref, DisplayItem, Obj, XrefEntry};
pub use viewer::{FindMatch, PdfViewer, ZoomMode};

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("not a pdf: header missing")]
    NotAPdf,
    #[error("malformed xref at byte {0}")]
    BadXref(usize),
    #[error("malformed object at byte {0}")]
    BadObject(usize),
    #[error("content stream: {0}")]
    Content(String),
    #[error("page {0} out of bounds")]
    PageBounds(u32),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
