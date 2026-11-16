//! Screenshots: capture from the display list, annotate with the same
//! primitives the page paints with, share local-first. Nothing leaves
//! the device unless the user says so, and every share expires.

pub mod annotate;
pub mod capture;
pub mod share;

pub use annotate::{Annotation, AnnotatedImage};
pub use capture::{CaptureRequest, CapturedImage};
pub use share::{SharePolicy, ShareTarget};

#[derive(Debug, thiserror::Error)]
pub enum ShotError {
    #[error("region {w}x{h} outside the {width}x{height} page")]
    RegionOutside {
        w: u32,
        h: u32,
        width: u32,
        height: u32,
    },
    #[error("encode: {0}")]
    Encode(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
