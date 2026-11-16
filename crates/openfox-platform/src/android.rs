use crate::{PlatformError, PlatformServices};
use android_activity::AndroidApp;
use std::sync::OnceLock;

/// The activity installs itself here from the generated `android_main`
/// entry, before any browser code asks for platform services.
static APP: OnceLock<AndroidApp> = OnceLock::new();

pub fn install(app: AndroidApp) {
    let _ = APP.set(app);
}

pub fn app() -> Option<&'static AndroidApp> {
    APP.get()
}

const FONT_ASSET: &str = "fonts/OpenFox.ttf";

/// Android platform services: the font ships inside the APK's assets (no
/// storage permission, no download), loaded through the activity's asset
/// manager. `fonts/OpenFox.ttf` is a build input of the Gradle packaging
/// step, documented in `docs/BUILD.md`.
pub fn services(app: &AndroidApp) -> Result<PlatformServices, PlatformError> {
    let manager = app.asset_manager();
    let font = manager
        .open(FONT_ASSET)
        .map_err(|_| PlatformError::NoFont)?
        .get_buffer()
        .map_err(|_| PlatformError::NoFont)?
        .to_vec();
    Ok(PlatformServices { font_bytes: font, platform: "android" })
}

/// The external files directory for the profile, scoped to the app: no
/// broad storage access is requested, ever.
pub fn profile_root(app: &AndroidApp) -> std::path::PathBuf {
    let base = app
        .external_data_dir()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir());
    base.join("profile")
}

/// Android lifecycle focus changes arrive as a notification, not a poll:
/// pause saves the session, resume restores it. Session restore is crash-only
/// design, so a pause is just a scheduled crash you asked for nicely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Resumed,
    Paused,
}

impl Lifecycle {
    pub fn from_focus(has_focus: bool) -> Lifecycle {
        if has_focus {
            Lifecycle::Resumed
        } else {
            Lifecycle::Paused
        }
    }

    pub fn should_save_session(&self) -> bool {
        matches!(self, Lifecycle::Paused)
    }
}
