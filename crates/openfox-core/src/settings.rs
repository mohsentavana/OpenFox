/// Profile-scoped settings. Privacy options default to the strict side and
/// exist so users can name exceptions — telemetry has no field here because
/// there is no telemetry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub homepage: String,
    pub search_template: String,
    pub block_ads: bool,
    pub block_trackers: bool,
    pub cosmetic_filtering: bool,
    /// First-party cookies only. Off means session cookies stay in memory.
    pub first_party_cookies: bool,
    /// Send Global Privacy Control and Do Not Track.
    pub privacy_signals: bool,
    /// Trim the Referer header to its origin.
    pub trim_referer: bool,
    /// Per-profile user agent override; empty keeps the platform default.
    pub user_agent: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            homepage: "about:fox".into(),
            search_template: "https://duckduckgo.com/?q={}".into(),
            block_ads: true,
            block_trackers: true,
            cosmetic_filtering: true,
            first_party_cookies: true,
            privacy_signals: true,
            trim_referer: true,
            user_agent: String::new(),
        }
    }
}

/// A profile is a directory with a name. Nothing more.
pub struct Profile {
    pub name: String,
    pub dir: std::path::PathBuf,
    pub settings: Settings,
}

impl Profile {
    pub fn open(name: &str, base: &std::path::Path) -> std::io::Result<Self> {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir)?;
        let settings_path = dir.join("settings.json");
        let settings = match std::fs::read_to_string(&settings_path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Settings::default(),
        };
        Ok(Profile { name: name.into(), dir, settings })
    }
}
