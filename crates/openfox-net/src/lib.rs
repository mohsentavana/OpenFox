pub mod cache;
pub mod cert;
pub mod client;
pub mod cookie;
pub mod dns;
pub mod doh;
pub mod h2;
pub mod hsts;
pub mod privacy;
pub mod proxy;
pub mod tls;
pub mod tls_pinning;
pub mod upgrade;
pub mod websocket;

pub use client::{Client, Request, Response};
pub use privacy::PrivacyHeaders;
pub use tls_pinning::{PinStore, PinVerdict};
pub use upgrade::{canonical_link, deamp_url, https_upgrade, HttpsMode, Upgrade};

use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
    #[error("dns: {0}")]
    Dns(String),
    #[error("connect {host}:{port}: {source}")]
    Connect { host: String, port: u16, source: std::io::Error },
    #[error("tls: {0}")]
    Tls(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed response head")]
    MalformedHead,
    #[error("header section exceeded 16 KiB")]
    HeadOverflow,
    #[error("too many redirects for {0}")]
    TooManyRedirects(String),
    // `::h2` — the local `h2` module shadows the crate name at the root.
    #[error("h2: {0}")]
    H2(#[from] ::h2::Error),
    #[error("http: {0}")]
    Http(#[from] http::Error),
    #[error("timed out after {0:?}")]
    Timeout(Duration),
}
