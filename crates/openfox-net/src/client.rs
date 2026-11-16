use crate::cookie::CookieJar;
use crate::dns::DnsCache;
use crate::h2::{self, H2Handle};
use crate::{NetError, PrivacyHeaders};
use bytes::Bytes;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

const HEAD_CAP: usize = 16 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: u8 = 8;

/// The request as the browser core issues it. `top_frame` is the page URL the
/// request belongs to — the cookie jar and privacy layer need it to make
/// first-party decisions.
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub top_frame: String,
    pub referer: Option<String>,
    pub redirect_limit: u8,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        let url = url.into();
        Request {
            method: "GET".into(),
            top_frame: url.clone(),
            url,
            headers: Vec::new(),
            body: None,
            referer: None,
            redirect_limit: MAX_REDIRECTS,
        }
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    pub elapsed: Duration,
    pub via: &'static str,
    pub final_url: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

/// A live connection to an origin: either TCP for plain http or the rustls
/// stream for https. Implements both halves of tokio IO so one HTTP/1.1 code
/// path serves both.
enum Conn {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for Conn {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Conn {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_flush(cx),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Conn::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

type PoolKey = (String, u16);

/// The browser's fetch pipeline: DNS cache, TLS with rustls, ALPN-negotiated
/// HTTP/2 with connection reuse, HTTP/1.1 with a keep-alive pool, redirect
/// policy, first-party cookies, privacy headers.
pub struct Client {
    tls: Arc<rustls::ClientConfig>,
    dns: Arc<DnsCache>,
    conn_pool: Arc<std::sync::Mutex<HashMap<PoolKey, Vec<Conn>>>>,
    h2_pool: Arc<std::sync::Mutex<HashMap<PoolKey, H2Handle>>>,
    cookies: Arc<Mutex<CookieJar>>,
    privacy: PrivacyHeaders,
}

impl Client {
    pub fn new(privacy: PrivacyHeaders, session_cookies_only: bool) -> Self {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let mut config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Client {
            tls: Arc::new(config),
            dns: Arc::new(DnsCache::new()),
            conn_pool: Arc::new(std::sync::Mutex::new(HashMap::new())),
            h2_pool: Arc::new(std::sync::Mutex::new(HashMap::new())),
            cookies: Arc::new(Mutex::new(CookieJar::new(session_cookies_only))),
            privacy,
        }
    }

    pub fn cookie_jar(&self) -> &Arc<Mutex<CookieJar>> {
        &self.cookies
    }

    /// Fetch with redirects, cookies and privacy headers. Every hop re-runs
    /// the full pipeline; nothing is short-circuited on trust.
    pub async fn fetch(&self, mut req: Request) -> Result<Response, NetError> {
        let started = Instant::now();
        for _ in 0..req.redirect_limit {
            let url = url::Url::parse(&req.url)?;
            let (status, headers, body, via) = match tokio::time::timeout(
                REQUEST_TIMEOUT,
                self.execute(&url, &req),
            )
            .await
            {
                Ok(result) => result?,
                Err(_) => return Err(NetError::Timeout(REQUEST_TIMEOUT)),
            };

            for (_, value) in headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case("set-cookie")) {
                let Some(host) = url.host_str() else { continue };
                let top = url::Url::parse(&req.top_frame)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .unwrap_or_else(|| host.to_owned());
                self.cookies.lock().await.store(value, host, &top);
            }

            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let location = headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("location"))
                    .map(|(_, v)| v.clone());
                if let Some(location) = location {
                    req.url = url.join(&location)?.as_str().to_owned();
                    if status == 303 {
                        req.method = "GET".into();
                        req.body = None;
                    }
                    continue;
                }
            }

            return Ok(Response {
                status,
                headers,
                body,
                elapsed: started.elapsed(),
                via,
                final_url: url.as_str().to_owned(),
            });
        }
        Err(NetError::TooManyRedirects(req.url))
    }

    async fn execute(
        &self,
        url: &url::Url,
        req: &Request,
    ) -> Result<(u16, Vec<(String, String)>, Bytes, &'static str), NetError> {
        let host = url.host_str().unwrap_or_default().to_owned();
        let port = url.port_or_known_default().unwrap_or(80);
        let key = (host.clone(), port);

        if url.scheme() == "https" {
            // 1. A live H2 connection for this origin: reuse and multiplex.
            let pooled_h2 = self.h2_pool.lock().expect("h2 pool lock").get(&key).cloned();
            if let Some(handle) = pooled_h2 {
                let headers = self.build_headers(url, req).await;
                if let Ok(triple) =
                    h2::request_over(handle, &req.method, url, &headers, req.body.clone()).await
                {
                    return Ok((triple.0, triple.1, triple.2, "h2"));
                }
                self.h2_pool.lock().expect("h2 pool lock").remove(&key);
            }

            // 2. A live HTTP/1.1 connection for this origin.
            if let Some(conn) = self.take_pooled(&key) {
                if let Ok(triple) = self.http1(conn, url, req).await {
                    return Ok((triple.0, triple.1, triple.2, "http/1.1"));
                }
            }

            // 3. Fresh dial. If ALPN said h2, upgrade the connection to the
            //    multiplexer and pool it for every future request.
            let stream = self.dial_tls(&host, port).await?;
            let negotiated_h2 = stream
                .get_ref()
                .1
                .alpn_protocol()
                .is_some_and(|proto| proto == b"h2");
            if negotiated_h2 {
                let headers = self.build_headers(url, req).await;
                let handle = h2::handshake(stream).await?;
                self.h2_pool.lock().expect("h2 pool lock").insert(key, handle.clone());
                let (status, headers, body) =
                    h2::request_over(handle, &req.method, url, &headers, req.body.clone()).await?;
                return Ok((status, headers, body, "h2"));
            }
            let conn = Conn::Tls(Box::new(stream));
            let triple = self.http1(conn, url, req).await?;
            Ok((triple.0, triple.1, triple.2, "http/1.1"))
        } else {
            if let Some(conn) = self.take_pooled(&key) {
                if let Ok(triple) = self.http1(conn, url, req).await {
                    return Ok((triple.0, triple.1, triple.2, "http/1.1"));
                }
            }
            let conn = Conn::Plain(self.dial(&host, port).await?);
            let triple = self.http1(conn, url, req).await?;
            Ok((triple.0, triple.1, triple.2, "http/1.1"))
        }
    }

    fn take_pooled(&self, key: &PoolKey) -> Option<Conn> {
        let mut pool = self.conn_pool.lock().expect("conn pool lock");
        pool.get_mut(key)
            .and_then(|conns| if conns.is_empty() { None } else { conns.pop() })
    }

    async fn build_headers(&self, url: &url::Url, req: &Request) -> Vec<(String, String)> {
        let host = url.host_str().unwrap_or_default();
        let mut headers: Vec<(String, String)> = vec![("Host".into(), host.to_owned())];
        let referer = req.referer.as_deref().and_then(|r| url::Url::parse(r).ok());
        self.privacy.apply(&mut headers, referer.as_ref());
        for (name, value) in &req.headers {
            if !name.eq_ignore_ascii_case("host")
                && !name.eq_ignore_ascii_case("user-agent")
                && !name.eq_ignore_ascii_case("cookie")
            {
                headers.push((name.clone(), value.clone()));
            }
        }
        let top_host = url::Url::parse(&req.top_frame)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_else(|| host.to_owned());
        if let Some(cookie) = self.cookies.lock().await.header_for(url, &top_host) {
            headers.push(("Cookie".into(), cookie));
        }
        if let Some(body) = &req.body {
            headers.push(("Content-Length".into(), body.len().to_string()));
        }
        headers.push(("Connection".into(), "keep-alive".into()));
        headers
    }

    async fn dial(&self, host: &str, port: u16) -> Result<TcpStream, NetError> {
        let ips = self.dns.resolve(host).await.map_err(NetError::Dns)?;
        let mut last: Option<std::io::Error> = None;
        for ip in &ips {
            let addr = std::net::SocketAddr::new(*ip, port);
            match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
                Ok(Ok(stream)) => return Ok(stream),
                Ok(Err(e)) => last = Some(e),
                Err(_) => {
                    last =
                        Some(std::io::Error::new(std::io::ErrorKind::TimedOut, "connect timeout"))
                }
            }
        }
        Err(NetError::Connect {
            host: host.into(),
            port,
            source: last.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, "no addresses")
            }),
        })
    }

    async fn dial_tls(
        &self,
        host: &str,
        port: u16,
    ) -> Result<tokio_rustls::client::TlsStream<TcpStream>, NetError> {
        let tcp = self.dial(host, port).await?;
        let server_name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|e| NetError::Tls(format!("{e}")))?;
        let connector = tokio_rustls::TlsConnector::from(self.tls.clone());
        connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| NetError::Tls(format!("{e}")))
    }

    /// HTTP/1.1 over a live connection. The connection returns to the
    /// keep-alive pool only when content-length framing proved it was fully
    /// consumed and the server did not ask to close.
    async fn http1(
        &self,
        mut conn: Conn,
        url: &url::Url,
        req: &Request,
    ) -> Result<(u16, Vec<(String, String)>, Bytes), NetError> {
        let headers = self.build_headers(url, req).await;
        let mut head = String::new();
        head.push_str(&format!("{} {} HTTP/1.1\r\n", req.method, h2::path_and_query(url)));
        for (name, value) in &headers {
            head.push_str(&format!("{}: {}\r\n", name, value));
        }
        head.push_str("\r\n");
        conn.write_all(head.as_bytes()).await?;
        if let Some(body) = &req.body {
            conn.write_all(body).await?;
        }
        conn.flush().await?;

        let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
        let (status, owned, head_len) = loop {
            if buf.len() > HEAD_CAP {
                return Err(NetError::HeadOverflow);
            }
            let mut chunk = [0u8; 2048];
            let n = conn.read(&mut chunk).await?;
            if n == 0 {
                return Err(NetError::MalformedHead);
            }
            buf.extend_from_slice(&chunk[..n]);
            let mut raw = [httparse::EMPTY_HEADER; 64];
            let mut response = httparse::Response::new(&mut raw);
            match response.parse(&buf) {
                Ok(httparse::Status::Complete(offset)) => {
                    let status = response.code.unwrap_or(200);
                    let owned: Vec<(String, String)> = response
                        .headers
                        .iter()
                        .map(|h| {
                            (
                                h.name.to_ascii_lowercase(),
                                String::from_utf8_lossy(h.value).into_owned(),
                            )
                        })
                        .collect();
                    break (status, owned, offset);
                }
                Ok(httparse::Status::Partial) => continue,
                Err(_) => return Err(NetError::MalformedHead),
            }
        };

        let chunked = owned
            .iter()
            .any(|(n, v)| n == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked"));
        let content_length = owned
            .iter()
            .find(|(n, _)| n == "content-length")
            .and_then(|(_, v)| v.trim().parse::<usize>().ok());
        let wants_close = owned
            .iter()
            .any(|(n, v)| n == "connection" && v.to_ascii_lowercase().contains("close"));

        let body: Vec<u8> = if chunked {
            read_chunked(&mut conn, &mut buf, head_len).await?
        } else if let Some(len) = content_length {
            let mut body = buf[head_len..].to_vec();
            while body.len() < len {
                let mut chunk = vec![0u8; (len - body.len()).min(64 * 1024)];
                let n = conn.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..n]);
            }
            body.truncate(len);
            body
        } else {
            let mut body = buf[head_len..].to_vec();
            loop {
                let mut chunk = [0u8; 8 * 1024];
                let n = conn.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..n]);
            }
            body
        };

        let pooled_ok = !wants_close && !chunked && content_length.is_some();
        if pooled_ok {
            let key = (
                url.host_str().unwrap_or_default().to_owned(),
                url.port_or_known_default().unwrap_or(80),
            );
            self.conn_pool
                .lock()
                .expect("conn pool lock")
                .entry(key)
                .or_default()
                .push(conn);
        }

        Ok((status, owned, Bytes::from(body)))
    }
}

async fn read_chunked<S>(stream: &mut S, buf: &mut Vec<u8>, start: usize) -> Result<Vec<u8>, NetError>
where
    S: AsyncRead + Unpin,
{
    let mut pos = start;
    let mut body: Vec<u8> = Vec::new();
    loop {
        let line_end = match find_crlf(&buf[pos..]) {
            Some(i) => pos + i,
            None => {
                read_more(stream, buf, 4096).await?;
                continue;
            }
        };
        let size_line = String::from_utf8_lossy(&buf[pos..line_end]).into_owned();
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(|_| NetError::MalformedHead)?;
        pos = line_end + 2;
        if size == 0 {
            break;
        }
        while buf.len() < pos + size + 2 {
            read_more(stream, buf, 8 * 1024).await?;
        }
        body.extend_from_slice(&buf[pos..pos + size]);
        pos += size + 2;
    }
    Ok(body)
}

async fn read_more<S>(stream: &mut S, buf: &mut Vec<u8>, cap: usize) -> Result<(), NetError>
where
    S: AsyncRead + Unpin,
{
    let mut chunk = vec![0u8; cap];
    let n = stream.read(&mut chunk).await?;
    if n == 0 {
        return Err(NetError::MalformedHead);
    }
    buf.extend_from_slice(&chunk[..n]);
    Ok(())
}

fn find_crlf(haystack: &[u8]) -> Option<usize> {
    haystack.windows(2).position(|w| w == b"\r\n")
}
