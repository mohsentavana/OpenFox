use crate::{PlatformError, PlatformServices};

const FONT_CANDIDATES: &[&str] = &[
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\Windows\\Fonts\\tahoma.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
];

/// Windows platform services: the UI font from the system directory.
/// Rune owns windowing, wgpu owns the surface, and the browser never
/// ships a font of its own.
pub fn services() -> Result<PlatformServices, PlatformError> {
    for path in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return Ok(PlatformServices { font_bytes: bytes, platform: "windows" });
        }
    }
    Err(PlatformError::NoFont)
}

/// The profile root on Windows, under `%LOCALAPPDATA%`.
pub fn profile_root() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("OpenFox")
}
