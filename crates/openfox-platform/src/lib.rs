#[cfg(target_os = "android")]
pub mod android;
pub mod windows;

/// What the platform hands the browser at startup: bytes, not abstractions.
/// No font is bundled into the binary — each OS ships its own, and the
/// compositor's glyph atlas is built from what the user's system already has.
pub struct PlatformServices {
    pub font_bytes: Vec<u8>,
    pub platform: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("no usable system font found")]
    NoFont,
    #[error("the Android app handle was not installed at entry")]
    NotInitialized,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl PlatformServices {
    #[cfg(target_os = "android")]
    pub fn load() -> Result<PlatformServices, PlatformError> {
        let app = android::app().ok_or(PlatformError::NotInitialized)?;
        android::services(app)
    }

    #[cfg(windows)]
    pub fn load() -> Result<PlatformServices, PlatformError> {
        windows::services()
    }

    #[cfg(not(any(target_os = "android", windows)))]
    pub fn load() -> Result<PlatformServices, PlatformError> {
        // Linux and friends: the classic locations, in preference order.
        const CANDIDATES: &[&str] = &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
        ];
        for path in CANDIDATES {
            if let Ok(bytes) = std::fs::read(path) {
                return Ok(PlatformServices { font_bytes: bytes, platform: "other" });
            }
        }
        Err(PlatformError::NoFont)
    }
}

/// Where the Android back button goes. A tab with history consumes it; a
/// tab without yields to the system, which finishes the activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackRouting {
    History,
    System,
}

pub fn route_back(has_history: bool) -> BackRouting {
    if has_history {
        BackRouting::History
    } else {
        BackRouting::System
    }
}
