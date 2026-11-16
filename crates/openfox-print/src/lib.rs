//! Print: paged media from the same layout tree, PDF out of the same
//! glyph runs the compositor paints. Printing is rendering with page
//! boxes — one pipeline, two outputs.

pub mod output;
pub mod paged;
pub mod preview;

pub use output::{OutputTarget, PdfOutput};
pub use paged::{PageBox, PageFormat, PagedMedia};
pub use preview::PrintPreview;

#[derive(Debug, thiserror::Error)]
pub enum PrintError {
    #[error("page {0} out of range")]
    PageRange(u32),
    #[error("empty document: nothing to print")]
    EmptyDocument,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
