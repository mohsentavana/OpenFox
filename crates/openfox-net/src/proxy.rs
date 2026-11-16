//! Proxy configuration: system, direct, manual, PAC-URL. The CONNECT
//! tunnel builder lives here; PAC scripts are honored as configuration
//! and evaluated in phase 2 (documented, not pretended).

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyConfig {
    System,
    Direct,
    Manual {
        http: Option<String>,
        https: Option<String>,
        socks5: Option<String>,
        no_proxy: Vec<String>,
    },
    PacUrl(String),
}

impl Default for ProxyConfig {
    fn default() -> Self {
        ProxyConfig::Direct
    }
}

impl ProxyConfig {
    /// Resolve the proxy for one URL. SOCKS5 answers both schemes; the
    /// HTTP proxy answers plain http; the https proxy answers https.
    pub fn proxy_for(&self, url: &url::Url) -> Option<String> {
        match self {
            ProxyConfig::Direct | ProxyConfig::System => None,
            ProxyConfig::PacUrl(_) => None, // PAC evaluation is phase 2
            ProxyConfig::Manual {
                http,
                https,
                socks5,
                no_proxy,
            } => {
                let host = url.host_str()?;
                if no_proxy
                    .iter()
                    .any(|skip| host == skip.as_str() || host.ends_with(&format!(".{skip}")))
                {
                    return None;
                }
                if socks5.is_some() {
                    return socks5.clone();
                }
                match url.scheme() {
                    "https" => https.clone().or_else(|| http.clone()),
                    "http" => http.clone(),
                    _ => None,
                }
            }
        }
    }

    /// Build the CONNECT tunnel request for https through an http proxy:
    /// the one request a proxy accepts before the TLS handshake begins
    /// inside the tunnel.
    pub fn connect_request(host: &str, port: u16) -> Vec<u8> {
        format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\nProxy-Connection: keep-alive\r\n\r\n").into_bytes()
    }

    /// Parse the CONNECT response: only a 2xx opens the tunnel.
    pub fn tunnel_established(response_head: &[u8]) -> bool {
        let text = String::from_utf8_lossy(response_head);
        text.starts_with("HTTP/1.1 2") || text.starts_with("HTTP/1.0 2")
    }

    /// Windows reads its proxy from the registry; Android from the
    /// wifi settings; both arrive as this shape.
    pub fn from_parts(
        http: Option<String>,
        https: Option<String>,
        socks5: Option<String>,
        no_proxy: Option<String>,
    ) -> Self {
        let no_proxy = no_proxy
            .map(|list| list.split(',').map(|s| s.trim().to_owned()).collect())
            .unwrap_or_default();
        if http.is_none() && https.is_none() && socks5.is_none() {
            return ProxyConfig::Direct;
        }
        ProxyConfig::Manual {
            http,
            https,
            socks5,
            no_proxy,
        }
    }
}
