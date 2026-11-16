//! A hand-rolled SOCKS5 CONNECT client, byte by byte: method
//! negotiation, RFC 1929 username/password auth, and the CONNECT
//! request with IPv4, IPv6 and DOMAINNAME addressing. No SOCKS crate —
//! the whole client half of the protocol is a few hundred lines, and
//! this codebase writes its protocol code in-house (see the WebSocket
//! implementation it sits beside) so the bytes on the wire are bytes
//! this tree has read.
//!
//! Supports exactly what a relay chain needs: CONNECT with no-auth or
//! username/password. BIND and UDP ASSOCIATE are refused by absence —
//! nothing in this browser should be accepting inbound anything
//! through a private circuit.

use crate::circuit::ProxyAuth;
use crate::RelayError;
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// SOCKS5 version byte.
const VERSION: u8 = 5;
/// RFC 1929 username/password auth method.
const METHOD_PASSWORD: u8 = 0x02;
/// No-auth method.
const METHOD_NONE: u8 = 0x00;
/// Server's "no acceptable methods" reply.
const METHOD_NONE_ACCEPTABLE: u8 = 0xFF;
/// CONNECT command.
const CMD_CONNECT: u8 = 0x01;
/// Address types.
const ATYP_IPV4: u8 = 0x01;
const ATYP_DOMAINNAME: u8 = 0x03;
const ATYP_IPV6: u8 = 0x04;

/// Tunnel `target_host:target_port` through `stream`, which must be a
/// fresh TCP connection to a SOCKS5 server. On success the stream is a
/// raw tunnel to the target and every later byte belongs to the target
/// protocol. The host is sent as a domain name unless it is a literal
/// IP — proxies that need to resolve the name themselves (the common
/// relay setup, and the whole point of a chain: the guard resolves, the
/// browser never does) get one.
pub async fn connect<S>(
    stream: &mut S,
    target_host: &str,
    target_port: u16,
    auth: Option<&ProxyAuth>,
) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    negotiate(stream, auth).await?;
    send_connect(stream, target_host, target_port).await?;
    read_reply(stream).await
}

/// Method negotiation: offer no-auth (always) and password auth (when
/// credentials exist), then take whichever the server picks. A server
/// that answers with a method we did not offer is broken or hostile;
/// either way the handshake ends here.
async fn negotiate<S>(stream: &mut S, auth: Option<&ProxyAuth>) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let greeting = match auth {
        Some(_) => vec![VERSION, 2, METHOD_NONE, METHOD_PASSWORD],
        None => vec![VERSION, 1, METHOD_NONE],
    };
    stream.write_all(&greeting).await?;
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await?;
    if choice[0] != VERSION {
        return Err(RelayError::Handshake(format!(
            "not a socks5 server (version byte {:#x})",
            choice[0]
        )));
    }
    match choice[1] {
        METHOD_NONE => Ok(()),
        METHOD_PASSWORD if auth.is_some() => password_auth(stream, auth).await,
        METHOD_NONE_ACCEPTABLE => Err(RelayError::UnsupportedAuth),
        other => Err(RelayError::Handshake(format!(
            "server chose method {other:#x} we did not offer"
        ))),
    }
}

/// RFC 1929 subnegotiation: version 1, length-prefixed username and
/// password, and a status byte that must come back zero. Credentials
/// cross this hop in the clear — SOCKS5 auth is unauthenticated TLS
/// only when the hop itself is TLS, which a TCP SOCKS hop never is.
/// Chains that need this should put the credential-bearing hop behind
/// a local gateway, and the doc comment here is the warning label.
async fn password_auth<S>(stream: &mut S, auth: Option<&ProxyAuth>) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(auth) = auth else {
        return Err(RelayError::UnsupportedAuth);
    };
    let mut request = Vec::with_capacity(3 + auth.username.len() + auth.password.len());
    request.push(0x01);
    request.push(auth.username.len() as u8);
    request.extend_from_slice(auth.username.as_bytes());
    request.push(auth.password.len() as u8);
    request.extend_from_slice(auth.password.as_bytes());
    stream.write_all(&request).await?;

    let mut verdict = [0u8; 2];
    stream.read_exact(&mut verdict).await?;
    if verdict[1] != 0x00 {
        return Err(RelayError::UnsupportedAuth);
    }
    Ok(())
}

/// The CONNECT request. DOMAINNAME addressing keeps hostnames off this
/// process's DNS — through a relay, resolving locally would hand the
/// guard's exit list to the local resolver, which is precisely the
/// traffic pattern private windows exist to avoid.
async fn send_connect<S>(
    stream: &mut S,
    host: &str,
    port: u16,
) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut request = Vec::with_capacity(host.len() + 10);
    request.extend_from_slice(&[VERSION, CMD_CONNECT, 0x00]);
    if let Ok(v4) = host.parse::<Ipv4Addr>() {
        request.push(ATYP_IPV4);
        request.extend_from_slice(&v4.octets());
    } else if let Ok(v6) = host.parse::<Ipv6Addr>() {
        request.push(ATYP_IPV6);
        request.extend_from_slice(&v6.octets());
    } else {
        let name = host.as_bytes();
        if name.is_empty() || name.len() > 255 {
            return Err(RelayError::Handshake("target host is not a sendable name".into()));
        }
        request.push(ATYP_DOMAINNAME);
        request.push(name.len() as u8);
        request.extend_from_slice(name);
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request).await?;
    stream.flush().await?;
    Ok(())
}

/// Read the CONNECT reply: 4-byte head, then the bound address in the
/// advertised shape, then the bound port. The bound address of a relay
/// is nobody's business but the relay's — it is read and discarded, on
/// the principle that parsing it is mandatory but remembering it is
/// not.
async fn read_reply<S>(stream: &mut S) -> Result<(), RelayError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    if head[0] != VERSION {
        return Err(RelayError::Handshake(format!(
            "reply version byte {:#x} is not socks5",
            head[0]
        )));
    }
    if head[1] != 0x00 {
        return Err(RelayError::SocksRefused { code: head[1] });
    }
    match head[3] {
        ATYP_IPV4 => {
            let mut tail = [0u8; 6];
            stream.read_exact(&mut tail).await?;
        }
        ATYP_IPV6 => {
            let mut tail = [0u8; 18];
            stream.read_exact(&mut tail).await?;
        }
        ATYP_DOMAINNAME => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut tail = vec![0u8; len[0] as usize + 2];
            stream.read_exact(&mut tail).await?;
        }
        other => {
            return Err(RelayError::Handshake(format!("reply address type {other:#x} unknown")));
        }
    }
    Ok(())
}
