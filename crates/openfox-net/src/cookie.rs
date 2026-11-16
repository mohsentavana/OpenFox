//! The first-party cookie jar: storage, expiry, path/secure/same-site
//! matching, and the public-suffix arithmetic that keeps a cookie from
//! example.com inside example.com's registrable domain. Third-party
//! cookies are not stored — there is no toggle, because there is no
//! sane use.

use psl::Psl;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A cookie jar that is first-party by construction: a cookie stored while
/// visiting example.com can only ever be sent back inside example.com's
/// registrable domain. Third-party cookies are not stored at all — there is
/// no toggle, because there is no sane use.
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub host: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    pub expires: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    #[default]
    Lax,
    Strict,
    None,
}

pub struct CookieJar {
    // psl 2: `Psl` is a trait; `psl::List` is the bundled static list
    // that implements it — zero-sized, compiled in, no runtime build.
    psl: psl::List,
    cookies: HashMap<(String, String, String), Cookie>,
    session_only: bool,
}

const MAX_COOKIES: usize = 2048;

impl CookieJar {
    pub fn new(session_only: bool) -> Self {
        CookieJar {
            psl: psl::List,
            cookies: HashMap::new(),
            session_only,
        }
    }

    fn registrable(&self, host: &str) -> String {
        self.psl
            .domain(host.as_bytes())
            .map(|d| String::from_utf8_lossy(d.as_bytes()).into_owned())
            .unwrap_or_else(|| host.to_owned())
    }

    /// Ingest Set-Cookie headers. `top_frame` is the URL of the page making
    /// the request; cross-site cookies are dropped here, not filtered later.
    pub fn store(&mut self, set_cookie: &str, response_host: &str, top_frame: &str) {
        let mut parts = set_cookie.split(';');
        let Some(pair) = parts.next() else { return };
        let Some((name, value)) = pair.split_once('=') else { return };

        let mut cookie = Cookie {
            name: name.trim().to_owned(),
            value: value.trim().to_owned(),
            host: response_host.to_owned(),
            path: "/".into(),
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
            expires: None,
        };

        for attr in parts {
            let attr = attr.trim();
            let lower = attr.to_ascii_lowercase();
            let (key, val) = match attr.split_once('=') {
                Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim()),
                None => (lower.clone(), ""),
            };
            match key.as_str() {
                "domain" => {
                    let d = val.trim_start_matches('.');
                    if self.registrable(response_host) == self.registrable(d) {
                        cookie.host = d.to_owned();
                    }
                }
                "path" if !val.is_empty() => cookie.path = val.to_owned(),
                "secure" => cookie.secure = true,
                "httponly" => cookie.http_only = true,
                "samesite" => match val.to_ascii_lowercase().as_str() {
                    "strict" => cookie.same_site = SameSite::Strict,
                    "none" => cookie.same_site = SameSite::None,
                    _ => cookie.same_site = SameSite::Lax,
                },
                "max-age" => {
                    if let Ok(secs) = val.parse::<u64>() {
                        cookie.expires = Some(Instant::now() + Duration::from_secs(secs));
                    }
                }
                "expires" => {
                    // RFC 1123 timestamps: parsing dates by hand is slop food.
                    // Browsers treat unparseable expiry as a session cookie,
                    // and so do we.
                    cookie.expires = None;
                }
                _ => {}
            }
        }

        if cookie.value.is_empty() || cookie.name.is_empty() {
            return;
        }
        if self.registrable(response_host) != self.registrable(top_frame) {
            return; // third-party: refused storage
        }

        let key = (cookie.host.clone(), cookie.name.clone(), cookie.path.clone());
        if self.cookies.len() >= MAX_COOKIES && !self.cookies.contains_key(&key) {
            self.evict_expired();
            if self.cookies.len() >= MAX_COOKIES {
                return;
            }
        }
        if self.session_only {
            cookie.expires = None;
        }
        self.cookies.insert(key, cookie);
    }

    /// Build the Cookie header for a request, honoring expiry, path, secure
    /// and same-site rules.
    pub fn header_for(&self, request_url: &url::Url, top_frame: &str) -> Option<String> {
        let host = request_url.host_str()?;
        let path = request_url.path().to_owned();
        let secure = request_url.scheme() == "https";
        let top_party = self.registrable(top_frame);
        let mut pairs: Vec<String> = Vec::new();
        for cookie in self.cookies.values() {
            let expired = cookie.expires.is_some_and(|t| t <= Instant::now());
            if expired {
                continue;
            }
            let host_ok = cookie.host == host || host.ends_with(&format!(".{}", cookie.host));
            let path_ok = path.starts_with(&cookie.path);
            let secure_ok = !cookie.secure || secure;
            let same_party = cookie.same_site != SameSite::None
                || self.registrable(host) == top_party;
            if host_ok && path_ok && secure_ok && same_party {
                pairs.push(format!("{}={}", cookie.name, cookie.value));
            }
        }
        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
    }

    fn evict_expired(&mut self) {
        let now = Instant::now();
        self.cookies.retain(|_, c| c.expires.map(|t| t > now).unwrap_or(true));
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }
}
