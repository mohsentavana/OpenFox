/// Privacy headers applied to every outbound request. The cheap, honest half
/// of fingerprint farbling, learned from Brave: signals on, referer trimmed
/// to origin, UA stable per profile instead of per request.
pub struct PrivacyHeaders {
    pub gpc: bool,
    pub dnt: bool,
    pub trim_referer: bool,
    pub user_agent: String,
}

pub const PLATFORM_UA: &str = if cfg!(target_os = "android") {
    "Mozilla/5.0 (Android 14; Mobile) OpenFox/0.1"
} else if cfg!(target_os = "windows") {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) OpenFox/0.1"
} else {
    "Mozilla/5.0 (X11; Linux x86_64) OpenFox/0.1"
};

impl PrivacyHeaders {
    /// Applies policy headers to a prepared request header list. Order:
    /// privacy signals first, then referer, then UA — so any list built for
    /// matching stays deterministic.
    pub fn apply(
        &self,
        headers: &mut Vec<(String, String)>,
        referer: Option<&url::Url>,
    ) {
        if self.gpc {
            headers.push(("Sec-GPC".into(), "1".into()));
        }
        if self.dnt {
            headers.push(("DNT".into(), "1".into()));
        }
        if self.trim_referer {
            if let Some(url) = referer {
                let origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));
                headers.push(("Referer".into(), origin));
            }
        } else if let Some(url) = referer {
            headers.push(("Referer".into(), url.as_str().to_owned()));
        }
        headers.push(("User-Agent".into(), self.user_agent.clone()));
    }
}

impl Default for PrivacyHeaders {
    fn default() -> Self {
        PrivacyHeaders { gpc: true, dnt: true, trim_referer: true, user_agent: PLATFORM_UA.into() }
    }
}
