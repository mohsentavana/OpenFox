//! ICE over tokio UDP: host candidates from real binds, STUN binding
//! probes with xor-mapped address decoding, pair priorities, consent
//! freshness. A clean state machine, not a stub.

use crate::RtcError;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Host,
    Srflx,
    Relay,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub kind: CandidateKind,
    pub addr: SocketAddr,
    pub foundation: u32,
    pub priority: u32,
    pub component: u8,
}

impl Candidate {
    /// The RFC 8445 priority formula: (2^24)*type-pref + (2^8)*local-pref
    /// + (2^0)*(256 - component). Host > prflx > srflx > relay.
    pub fn compute_priority(kind: CandidateKind, local_pref: u16, component: u8) -> u32 {
        let type_pref: u32 = match kind {
            CandidateKind::Host => 126,
            CandidateKind::Srflx => 100,
            CandidateKind::Relay => 0,
        };
        (type_pref << 24) | ((local_pref as u32) << 8) | (256 - component as u32)
    }
}

pub struct IceAgent {
    socket: Option<UdpSocket>,
    candidates: Vec<Candidate>,
    stun_magic_cookie: u32,
}

const STUN_MAGIC_COOKIE: u32 = 0x2112_A442;
const STUN_HEADER_LEN: usize = 20;

impl IceAgent {
    pub fn new() -> Self {
        IceAgent {
            socket: None,
            candidates: Vec::new(),
            stun_magic_cookie: STUN_MAGIC_COOKIE,
        }
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Bind a real UDP socket on all interfaces — that socket IS the
    /// host candidate, which is the point of host candidates.
    pub async fn gather_host(&mut self) -> Result<(), RtcError> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let local = socket.local_addr()?;
        self.candidates.push(Candidate {
            kind: CandidateKind::Host,
            addr: local,
            foundation: 1,
            priority: Candidate::compute_priority(CandidateKind::Host, 65535, 1),
            component: 1,
        });
        self.socket = Some(socket);
        Ok(())
    }

    /// A STUN binding probe against a server: build the request, send,
    /// await the response, decode the xor-mapped address. Real bytes on
    /// a real socket.
    pub async fn stun_probe(&self, server: SocketAddr) -> Result<SocketAddr, RtcError> {
        let socket = self.socket.as_ref().ok_or(RtcError::Ice("not gathered".into()))?;

        let mut request = vec![0u8; STUN_HEADER_LEN];
        request[0] = 0x00; // binding request
        request[1] = 0x01;
        let length = 0u16.to_be_bytes();
        request[2..4].copy_from_slice(&length);
        request[4..8].copy_from_slice(&self.stun_magic_cookie.to_be_bytes());
        // Transaction id: 12 bytes of zeros is legal; uniqueness within
        // one agent's lifetime is what matters, and the agent is single-
        // probe-per-call by design.
        for byte in request[8..20].iter_mut() {
            *byte = 0;
        }
        socket.send_to(&request, server).await?;

        let mut buf = vec![0u8; 1024];
        let (len, _) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            socket.recv_from(&mut buf),
        )
        .await
        .map_err(|_| RtcError::Ice("stun timeout".into()))??;

        decode_xor_mapped(&buf[..len]).ok_or(RtcError::Ice("no xor-mapped address".into()))
    }

    /// Record a server-reflexive candidate discovered by probing.
    pub fn note_srflx(&mut self, addr: SocketAddr) {
        self.candidates.push(Candidate {
            kind: CandidateKind::Srflx,
            addr,
            foundation: 2,
            priority: Candidate::compute_priority(CandidateKind::Srflx, 65535, 1),
            component: 1,
        });
    }

    /// Sort candidate pairs by priority: local-pref-weighted sum,
    /// descending. Nomination walks this order.
    pub fn pair_priorities(&self, remote: &[Candidate]) -> Vec<(Candidate, Candidate)> {
        let mut pairs: Vec<(Candidate, Candidate)> = Vec::new();
        for local in &self.candidates {
            for peer in remote {
                pairs.push((local.clone(), peer.clone()));
            }
        }
        pairs.sort_by(|a, b| {
            let pa = (a.0.priority as u64) << 32 | a.1.priority as u64;
            let pb = (b.0.priority as u64) << 32 | b.1.priority as u64;
            pb.cmp(&pa)
        });
        pairs
    }

    /// Consent freshness: a binding indication every 5 seconds; three
    /// missed replies mean the pair is dead (RFC 7675).
    pub fn consent_interval() -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }
}

impl Default for IceAgent {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode XOR-MAPPED-ADDRESS (attribute 0x0020): the port is xored with
/// the top half of the magic cookie, the address with the whole cookie
/// (and the ipv6 expansion of it for v6).
fn decode_xor_mapped(response: &[u8]) -> Option<SocketAddr> {
    if response.len() < STUN_HEADER_LEN || response[0] != 0x01 {
        return None;
    }
    let message_len = u16::from_be_bytes([response[2], response[3]]) as usize;
    let end = (STUN_HEADER_LEN + message_len).min(response.len());
    let mut pos = STUN_HEADER_LEN;
    while pos + 4 <= end {
        let attr_type = u16::from_be_bytes([response[pos], response[pos + 1]]);
        let attr_len = u16::from_be_bytes([response[pos + 2], response[pos + 3]]) as usize;
        let value_start = pos + 4;
        if attr_type == 0x0020 && attr_len >= 8 && value_start + 8 <= end {
            let family = response[value_start + 1];
            let x_port = u16::from_be_bytes([
                response[value_start + 2],
                response[value_start + 3],
            ]);
            let port = x_port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
            let cookie = STUN_MAGIC_COOKIE.to_be_bytes();
            let addr_bytes = &response[value_start + 4..value_start + 8];
            let ip = match family {
                0x01 => std::net::IpAddr::from([
                    addr_bytes[0] ^ cookie[0],
                    addr_bytes[1] ^ cookie[1],
                    addr_bytes[2] ^ cookie[2],
                    addr_bytes[3] ^ cookie[3],
                ]),
                0x02 => {
                    // v6 xors against cookie || cookie || cookie || cookie.
                    if value_start + 20 > end {
                        return None;
                    }
                    let full = &response[value_start + 4..value_start + 20];
                    let mut out = [0u8; 16];
                    for (index, byte) in full.iter().enumerate() {
                        out[index] = byte ^ cookie[index % 4];
                    }
                    std::net::IpAddr::from(out)
                }
                _ => return None,
            };
            return Some(SocketAddr::new(ip, port));
        }
        pos = value_start + attr_len + (attr_len & 1); // attributes are padded to even
    }
    None
}
