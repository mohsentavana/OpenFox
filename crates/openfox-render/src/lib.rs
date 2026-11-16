pub mod animation;
pub mod compositor;
pub mod glyphs;
pub mod images;
pub mod text;
pub mod video;

pub use animation::AnimationDriver;
pub use compositor::Compositor;
pub use glyphs::{GlyphAtlas, GlyphCell};
pub use images::ImageDecodePool;
pub use text::{GlyphRun, TextPipeline};
pub use video::VideoFrames;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no wgpu adapter")]
    NoAdapter,
    #[error("wgpu device request failed")]
    Device,
    #[error("font data unusable: {0}")]
    Font(String),
    #[error("png encode failed: {0}")]
    Png(String),
    #[error("render at {0}x{1} exceeds buffer limits")]
    BufferOverflow(u32, u32),
    #[error("gpu readback failed: {0}")]
    Readback(String),
}
