//! HTTP CONNECT tunneling: the request a proxy accepts before the
//! stream stops being HTTP and starts being a pipe. One request line,
//! one Host, optional Basic credentials, a blank line — then the
//! status line that decides everything. Hand-parsed on purpose: the
//! response to a CONNECT is a status line plus headers nobody reads,
//! and dragging `httparse` in for eleven bytes would be the dependency
//! doing the depending.
//!
//! The head is read into a `BytesMut` because `read_buf` grows it
//! without the copy a fresh `Vec` per chunk would cost — the same
//! pattern the net stack's H2 path leans on `bytes` for.

use crate::circuit::ProxyAuth;
use crate::RelayError;
use bytes::{BufMut, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// A CONNECT response head larger than this is not a proxy, it is a
/// website answering the wrong question.
const MAX_HEAD: usize = 8 * 1024;

/// Tunnel `host:port` through `stream`, a fresh TCP connection to an
/// HTTP proxy. Only a 2xx opens the tunnel; anything else is a refusal
/// with the status code preserved for the error surface.
pub async fn connect<S>(
    stream: &mut S,
    host: &str,
    port: u16,
    auth: Option<&ProxyAuth>,
) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let authority = format!("{host}:{port}");
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
    if let Some(auth) = auth {
        let credentials = format!("{}:{}", auth.username, auth.password);
        request.push_str("Proxy-Authorization: Basic ");
        base64_into(&credentials, &mut request);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;

    let head = read_head(stream).await?;
    let status = status_code(&head)?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(RelayError::HttpRefused { status })
    }
}

/// Read bytes until the blank line ends the response head, capped at
/// [`MAX_HEAD`]. A server that never sends `\r\n\r\n` gets the
/// overflow error, not an unbounded read.
async fn read_head<S>(stream: &mut S) -> Result<String, RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut head = BytesMut::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        if head.len() > MAX_HEAD {
            return Err(RelayError::HeadOverflow(MAX_HEAD));
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(RelayError::MalformedHead);
        }
        let previous = head.len();
        head.put_slice(&chunk[..read]);
        if head[previous..].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

/// Parse the status code off `HTTP/1.x NNN`. The reason phrase is
/// irrelevant to a tunnel decision and is left unread.
fn status_code(head: &str) -> Result<u16, RelayError> {
    let first_line = head.lines().next().unwrap_or_default();
    let mut parts = first_line.split_ascii_whitespace();
    let protocol = parts.next().unwrap_or_default();
    if !(protocol == "HTTP/1.1" || protocol == "HTTP/1.0") {
        return Err(RelayError::Handshake(format!(
            "{protocol:?} is not an HTTP status line"
        )));
    }
    parts
        .next()
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| RelayError::MalformedHead)
}

/// base64 (standard alphabet, padding) appended to `out` — the third
/// in-house copy of forty lines in this tree, each beside the protocol
/// that needs it. Cheaper than the dependency, again.
fn base64_into(bytes: &str, out: &mut String) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = bytes.as_bytes();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
}
