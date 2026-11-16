//! HTTP/2 over the `h2` crate: one pooled `SendRequest` handle per origin,
//! requests multiplexed over a shared connection, the connection future
//! spawned in the background so the multiplexer keeps running between fetches.

use crate::NetError;
use bytes::Bytes;
use http::Method;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_rustls::client::TlsStream;
use tokio::net::TcpStream;
use url::Url;

pub(crate) type H2Handle = Arc<Mutex<h2::client::SendRequest<Bytes>>>;

/// One HTTP/2 request over a shared, pooled connection. The `SendRequest`
/// half is cloned out of the mutex, then the guard is dropped — concurrent
/// fetches multiplex the same stream instead of queueing behind it.
pub async fn request_over(
    handle: H2Handle,
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    body: Option<Vec<u8>>,
) -> Result<(u16, Vec<(String, String)>, Bytes), NetError> {
    let mut send_request = {
        let guard = handle.lock().await;
        guard.clone()
    };
    // `ready()` consumes the handle and hands it back once the connection
    // can open a new stream — the returned one is the handle to keep.
    send_request = send_request.ready().await?;

    let request = build_request(method, url, headers)?;
    let end_of_stream = body.is_none();
    let (send, mut send_stream) = send_request.send_request(request, end_of_stream)?;
    if let Some(bytes) = body {
        send_stream.send_data(Bytes::from(bytes), true)?;
    }

    // The response future resolves to the full head with the body stream
    // as its payload — `into_body()` splits them.
    let head = send.await?;
    let status = head.status().as_u16();
    let headers = head
        .headers()
        .iter()
        .map(|(n, v)| (n.as_str().to_owned(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let mut body_stream = head.into_body();

    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    while let Some(chunk) = body_stream.data().await {
        let chunk = chunk?;
        let _ = body_stream.flow_control().release_capacity(chunk.len());
        buf.extend_from_slice(&chunk);
    }
    Ok((status, headers, Bytes::from(buf)))
}

/// Establishes HTTP/2 over a freshly negotiated TLS stream and returns the
/// pooled handle. The connection task is spawned so the multiplexer runs.
pub async fn handshake(
    stream: TlsStream<TcpStream>,
) -> Result<H2Handle, NetError> {
    let (send_request, connection) = h2::client::handshake(stream).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(Arc::new(Mutex::new(send_request)))
}

fn build_request(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
) -> Result<http::Request<()>, NetError> {
    let method = Method::from_bytes(method.as_bytes()).map_err(|_| NetError::MalformedHead)?;
    // h2's client derives the :scheme and :authority pseudo-headers from
    // the request URI, so it must stay in absolute form (scheme + host +
    // path). Origin-form here is the classic "URI missing scheme and
    // authority" failure; HTTP/1.1 is where origin-form belongs.
    let mut builder = http::Request::builder()
        .method(method)
        .version(http::Version::HTTP_2)
        .uri(url.as_str());
    for (name, value) in headers {
        // HTTP/2 header rules: names arrive lowercase, and the
        // connection-specific hop-by-hop vocabulary of HTTP/1.1 is
        // dissolved into the stream itself — sending "Connection" or
        // "Host" through an h2 sender is a protocol error the peer (or
        // this crate) rejects as malformed.
        let name = name.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "connection" | "keep-alive" | "proxy-connection" | "transfer-encoding" | "upgrade" | "te" | "host"
        ) {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_str());
    }
    Ok(builder.body(())?)
}

pub(crate) fn path_and_query(url: &Url) -> String {
    let path = url.path();
    let target = if path.is_empty() { "/" } else { path };
    match url.query() {
        Some(q) => format!("{target}?{q}"),
        None => target.to_owned(),
    }
}
