//! DNS-over-HTTPS, RFC 8484: the wire-format DNS message built and
//! parsed in-house (header, question, answer RRs with name-pointer
//! compression), posted as application/dns-message, cached by TTL.

use crate::NetError;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use url::Url;

/// The transport DoH needs: one POST, binary in, binary out. The app
/// wires this to the real HTTP client — DoH must never dial around it.
pub trait DohFetch {
    fn doh_post(&self, server: &Url, body: Vec<u8>) -> Result<Vec<u8>, NetError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordKind {
    A,
    Aaaa,
    Cname,
}

impl RecordKind {
    fn qtype(self) -> u16 {
        match self {
            RecordKind::A => 1,
            RecordKind::Aaaa => 28,
            RecordKind::Cname => 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Answer {
    pub name: String,
    pub kind: RecordKind,
    pub ttl: u32,
    pub data: IpData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpData {
    V4([u8; 4]),
    V6([u8; 16]),
    Alias(String),
}

pub struct DohResolver {
    server: Url,
    cache: HashMap<(String, RecordKind), (Vec<Answer>, Instant)>,
}

impl DohResolver {
    pub fn new(server: Url) -> Self {
        DohResolver {
            server,
            cache: HashMap::new(),
        }
    }

    /// Resolve through the cache, then the wire. TTL floors at 30s —
    /// flappy DNS is worse than stale DNS.
    pub fn resolve(&mut self, name: &str, kind: RecordKind, fetch: &dyn DohFetch) -> Result<Vec<Answer>, NetError> {
        let key = (name.to_ascii_lowercase(), kind);
        if let Some((answers, expiry)) = self.cache.get(&key) {
            if *expiry > Instant::now() {
                return Ok(answers.clone());
            }
        }
        let answers = self.query_wire(name, kind, fetch)?;
        let ttl = answers
            .iter()
            .map(|answer| answer.ttl)
            .max()
            .unwrap_or(30)
            .max(30);
        self.cache
            .insert(key, (answers.clone(), Instant::now() + Duration::from_secs(ttl as u64)));
        Ok(answers)
    }

    fn query_wire(&mut self, name: &str, kind: RecordKind, fetch: &dyn DohFetch) -> Result<Vec<Answer>, NetError> {
        let question = build_query(name, kind.qtype());
        let response = fetch.doh_post(&self.server, question)?;
        let (answers, min_ttl) = parse_response(&response)?;
        Ok(answers.into_iter().map(|mut answer| {
            answer.ttl = answer.ttl.max(min_ttl);
            answer
        }).collect())
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}

/// Build one DNS query message: id 0 (DoH serves one question per POST
/// and deduplicates itself), RD flag, one question.
fn build_query(name: &str, qtype: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() + 18);
    out.extend_from_slice(&0u16.to_be_bytes()); // id
    out.extend_from_slice(&0x0100u16.to_be_bytes()); // RD
    out.extend_from_slice(&1u16.to_be_bytes()); // qdcount
    out.extend_from_slice(&0u16.to_be_bytes()); // ancount
    out.extend_from_slice(&0u16.to_be_bytes()); // nscount
    out.extend_from_slice(&0u16.to_be_bytes()); // arcount
    for label in name.trim_end_matches('.').split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // class IN
    out
}

/// Parse a response: skip the question, walk answers with 0xC0 pointer
/// compression for names, decode A/AAAA/CNAME rdata.
fn parse_response(response: &[u8]) -> Result<(Vec<Answer>, u32), NetError> {
    if response.len() < 12 {
        return Err(NetError::Dns("short response".into()));
    }
    let rcode = response[3] & 0x0F;
    if rcode != 0 {
        return Err(NetError::Dns(format!("rcode {rcode}")));
    }
    let ancount = u16::from_be_bytes([response[6], response[7]]) as usize;
    let qdcount = u16::from_be_bytes([response[4], response[5]]) as usize;

    let mut pos = 12;
    for _ in 0..qdcount {
        pos = skip_name(response, pos)?;
        pos += 4;
    }

    let mut answers = Vec::with_capacity(ancount);
    let mut min_ttl = u32::MAX;
    for _ in 0..ancount {
        let name = read_name(response, pos)?;
        pos = skip_name(response, pos)?;
        if pos + 10 > response.len() {
            break;
        }
        let rtype = u16::from_be_bytes([response[pos], response[pos + 1]]);
        pos += 4; // type + class
        let ttl = u32::from_be_bytes([
            response[pos],
            response[pos + 1],
            response[pos + 2],
            response[pos + 3],
        ]);
        pos += 4;
        let rdlength = u16::from_be_bytes([response[pos], response[pos + 1]]) as usize;
        pos += 2;
        let rdata_start = pos;
        pos += rdlength;

        let (kind, data) = match (rtype, rdlength) {
            (1, 4) => (
                RecordKind::A,
                IpData::V4([
                    response[rdata_start],
                    response[rdata_start + 1],
                    response[rdata_start + 2],
                    response[rdata_start + 3],
                ]),
            ),
            (28, 16) => {
                let mut bytes = [0u8; 16];
                bytes.copy_from_slice(&response[rdata_start..rdata_start + 16]);
                (RecordKind::Aaaa, IpData::V6(bytes))
            }
            (5, _) => {
                let alias = read_name(response, rdata_start)?;
                (RecordKind::Cname, IpData::Alias(alias))
            }
            _ => continue,
        };
        min_ttl = min_ttl.min(ttl);
        answers.push(Answer {
            name,
            kind,
            ttl,
            data,
        });
    }
    if min_ttl == u32::MAX {
        min_ttl = 30;
    }
    Ok((answers, min_ttl))
}

/// Read a possibly-compressed name. Pointers (0xC0 | high) jump back;
/// the guard stops a hostile loop.
fn read_name(bytes: &[u8], start: usize) -> Result<String, NetError> {
    let mut labels = Vec::new();
    let mut pos = start;
    let mut jumps = 0;
    loop {
        let length = *bytes
            .get(pos)
            .ok_or_else(|| NetError::Dns("name out of bounds".into()))?;
        if length == 0 {
            break;
        }
        if length & 0xC0 == 0xC0 {
            let hi = (length & 0x3F) as usize;
            let lo = *bytes
                .get(pos + 1)
                .ok_or_else(|| NetError::Dns("short pointer".into()))? as usize;
            pos = (hi << 8) | lo;
            jumps += 1;
            if jumps > 8 {
                return Err(NetError::Dns("pointer loop".into()));
            }
            continue;
        }
        let end = pos + 1 + length as usize;
        let label = bytes
            .get(pos + 1..end)
            .ok_or_else(|| NetError::Dns("label out of bounds".into()))?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        pos = end;
    }
    Ok(labels.join("."))
}

fn skip_name(bytes: &[u8], start: usize) -> Result<usize, NetError> {
    // The contract: the offset just past the name IN THE ORIGINAL STREAM.
    // A pointer consumes two bytes there; the labels it points at may
    // even end in another pointer. Track the original end, walk the rest.
    let mut pos = start;
    let mut end: Option<usize> = None;
    let mut jumps = 0;
    loop {
        let length = *bytes
            .get(pos)
            .ok_or_else(|| NetError::Dns("name out of bounds".into()))?;
        if length == 0 {
            return Ok(end.unwrap_or(pos + 1));
        }
        if length & 0xC0 == 0xC0 {
            let hi = (length & 0x3F) as usize;
            let lo = *bytes
                .get(pos + 1)
                .ok_or_else(|| NetError::Dns("short pointer".into()))? as usize;
            if end.is_none() {
                end = Some(pos + 2);
            }
            pos = (hi << 8) | lo;
            jumps += 1;
            if jumps > 8 {
                return Err(NetError::Dns("pointer loop".into()));
            }
            continue;
        }
        pos += 1 + length as usize;
    }
}
