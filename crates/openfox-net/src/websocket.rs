//! RFC 6455 WebSockets over any tokio IO stream: handshake with the
//! SHA-1 accept check, frame codec with all three length forms, masking,
//! fragmentation, close semantics. Real protocol code, byte by byte.

use crate::NetError;
use sha1::{Digest, Sha1};
use std::future::Future;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_FRAME: usize = 16 * 1024 * 1024;
const MAX_CONTROL: usize = 125;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    fn from_u8(value: u8) -> Option<Opcode> {
        match value {
            0x0 => Some(Opcode::Continuation),
            0x1 => Some(Opcode::Text),
            0x2 => Some(Opcode::Binary),
            0x8 => Some(Opcode::Close),
            0x9 => Some(Opcode::Ping),
            0xA => Some(Opcode::Pong),
            _ => None,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close { code: u16, reason: String },
}

/// The server's accept token: base64(sha1(key + GUID)). The handshake is
/// only complete when this matches — an HTTP 101 without the right token
/// is a downgrade attack with better manners.
pub fn accept_token(key: &str) -> String {
    let digest = Sha1::digest(format!("{key}{GUID}").as_bytes());
    base64(&digest)
}

pub struct WebSocket<S> {
    stream: S,
    fragmented: Option<(Opcode, Vec<u8>)>,
    closed: bool,
}

impl<S> WebSocket<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Client handshake: send the upgrade request, read the 101 and
    /// verify the accept token.
    pub async fn client_handshake(
        mut stream: S,
        host: &str,
        path: &str,
    ) -> Result<WebSocket<S>, NetError> {
        let mut key_bytes = [0u8; 16];
        fastrand_fill(&mut key_bytes);
        let key = base64(&key_bytes);

        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await?;
        stream.flush().await?;

        let mut response = Vec::with_capacity(1024);
        let mut byte = [0u8; 1];
        while response.len() < 16 * 1024 {
            stream.read_exact(&mut byte).await?;
            response.push(byte[0]);
            if response.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&response);
        if !text.starts_with("HTTP/1.1 101") && !text.starts_with("HTTP/1.0 101") {
            return Err(NetError::MalformedHead);
        }
        let expected = accept_token(&key);
        let accepted = text
            .lines()
            .any(|line| line.eq_ignore_ascii_case(&format!("sec-websocket-accept: {expected}")));
        if !accepted {
            return Err(NetError::MalformedHead);
        }

        Ok(WebSocket {
            stream,
            fragmented: None,
            closed: false,
        })
    }

    /// Read one complete message, reassembling fragments and answering
    /// pings along the way.
    pub async fn read_message(&mut self) -> Result<Message, NetError> {
        loop {
            let (fin, opcode, payload) = self.read_frame().await?;
            match opcode {
                Opcode::Ping => {
                    let reply = Message::Pong(payload);
                    self.write_message(reply).await?;
                    continue;
                }
                Opcode::Pong => continue,
                Opcode::Close => {
                    let (code, reason) = if payload.len() >= 2 {
                        (
                            u16::from_be_bytes([payload[0], payload[1]]),
                            String::from_utf8_lossy(&payload[2..]).into_owned(),
                        )
                    } else {
                        (1005, String::new())
                    };
                    self.closed = true;
                    return Ok(Message::Close { code, reason });
                }
                Opcode::Text | Opcode::Binary => {
                    if fin {
                        return Ok(finish(opcode, payload));
                    }
                    self.fragmented = Some((opcode, payload));
                }
                Opcode::Continuation => {
                    let Some((started, buffer)) = self.fragmented.take() else {
                        return Err(NetError::MalformedHead);
                    };
                    let mut buffer = buffer;
                    if buffer.len() + payload.len() > MAX_FRAME {
                        return Err(NetError::MalformedHead);
                    }
                    buffer.extend_from_slice(&payload);
                    if fin {
                        return Ok(finish(started, buffer));
                    }
                    self.fragmented = Some((started, buffer));
                }
            }
        }
    }

    /// Read one raw frame. Client frames from a server are never masked;
    /// our writes always are.
    async fn read_frame(&mut self) -> Result<(bool, Opcode, Vec<u8>), NetError> {
        let mut head = [0u8; 2];
        self.stream.read_exact(&mut head).await?;
        let fin = head[0] & 0x80 != 0;
        let opcode = Opcode::from_u8(head[0] & 0x0F).ok_or(NetError::MalformedHead)?;
        let masked = head[1] & 0x80 != 0;
        let len7 = (head[1] & 0x7F) as usize;

        let length = match len7 {
            126 => {
                let mut ext = [0u8; 2];
                self.stream.read_exact(&mut ext).await?;
                u16::from_be_bytes(ext) as usize
            }
            127 => {
                let mut ext = [0u8; 8];
                self.stream.read_exact(&mut ext).await?;
                let value = u64::from_be_bytes(ext) as usize;
                if value > MAX_FRAME {
                    return Err(NetError::MalformedHead);
                }
                value
            }
            value => value,
        };
        if matches!(opcode, Opcode::Ping | Opcode::Pong | Opcode::Close) && length > MAX_CONTROL {
            return Err(NetError::MalformedHead);
        }

        let mut mask = [0u8; 4];
        if masked {
            self.stream.read_exact(&mut mask).await?;
        }
        let mut payload = vec![0u8; length];
        self.stream.read_exact(&mut payload).await?;
        if masked {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        Ok((fin, opcode, payload))
    }

    pub async fn write_message(&mut self, message: Message) -> Result<(), NetError> {
        let (opcode, payload, fin) = match message {
            Message::Text(text) => (Opcode::Text, text.into_bytes(), true),
            Message::Binary(bytes) => (Opcode::Binary, bytes, true),
            Message::Ping(bytes) => (Opcode::Ping, bytes, true),
            Message::Pong(bytes) => (Opcode::Pong, bytes, true),
            Message::Close { code, reason } => {
                let mut bytes = code.to_be_bytes().to_vec();
                bytes.extend_from_slice(reason.as_bytes());
                (Opcode::Close, bytes, true)
            }
        };
        self.write_frame(fin, opcode, &payload).await
    }

    /// All client frames are masked with a fresh key — the RFC's
    /// requirement, and cache-poisoning insurance besides.
    async fn write_frame(&mut self, fin: bool, opcode: Opcode, payload: &[u8]) -> Result<(), NetError> {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(if fin { 0x80 } else { 0x00 } | opcode.as_u8());

        let mut mask_key = [0u8; 4];
        fastrand_fill(&mut mask_key);
        let length = payload.len();
        if length < 126 {
            frame.push(0x80 | length as u8);
        } else if length <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
        frame.extend_from_slice(&mask_key);
        for (index, byte) in payload.iter().enumerate() {
            frame.push(byte ^ mask_key[index % 4]);
        }
        self.stream.write_all(&frame).await?;
        self.stream.flush().await?;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

fn finish(opcode: Opcode, payload: Vec<u8>) -> Message {
    match opcode {
        Opcode::Text => Message::Text(String::from_utf8_lossy(&payload).into_owned()),
        _ => Message::Binary(payload),
    }
}

fn fastrand_fill(bytes: &mut [u8]) {
    // A tiny xorshift for the handshake key; the key needs uniqueness,
    // and the accept token proves the server read it.
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1;
    for byte in bytes.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 56) as u8;
    }
}

// base64, standard alphabet with padding — the one true variant for
// HTTP headers.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
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
    out
}

/// A boxed read future helper for generic transports.
pub type ReadFuture<'a> = Pin<Box<dyn Future<Output = Result<Message, NetError>> + Send + 'a>>;
