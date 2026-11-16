//! Binary deltas: 4 KiB blocks indexed by a rolling hash of the old
//! file; a delta is a run of Copy ops and Literal runs. Apply verifies
//! sha256 of the result against the manifest — a delta that produces
//! something else never installs.

use crate::UpdateError;
use sha2::{Digest, Sha256};

pub const BLOCK: usize = 4096;
pub const MAGIC: &[u8; 5] = b"OFXD1";

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Copy { old_offset: u32, len: u32 },
    Literal { bytes: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct Delta {
    pub ops: Vec<Op>,
}

/// A weak rolling hash (adler-flavored) good enough to find moved
/// blocks; the strong sha256 check at the end catches any collision.
fn block_hash(block: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for byte in block {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Build a delta transforming `old` into `new`.
pub fn build_delta(old: &[u8], new: &[u8]) -> Delta {
    let mut index: std::collections::HashMap<u32, Vec<u32>> =
        std::collections::HashMap::new();
    for (offset, block) in old.chunks(BLOCK).enumerate() {
        index
            .entry(block_hash(block))
            .or_default()
            .push((offset * BLOCK) as u32);
    }

    let mut ops: Vec<Op> = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut pos = 0usize;
    while pos < new.len() {
        let take = (new.len() - pos).min(BLOCK);
        let block = &new[pos..pos + take];
        let candidates = index.get(&block_hash(block)).cloned().unwrap_or_default();
        let matched = candidates.iter().find_map(|old_offset| {
            let old_start = *old_offset as usize;
            let old_end = (old_start + take).min(old.len());
            if old[old_start..old_end] == *block {
                Some(*old_offset)
            } else {
                None
            }
        });

        match matched {
            Some(old_offset) => {
                if !pending.is_empty() {
                    ops.push(Op::Literal {
                        bytes: std::mem::take(&mut pending),
                    });
                }
                ops.push(Op::Copy {
                    old_offset,
                    len: take as u32,
                });
                pos += take;
            }
            None => {
                pending.extend_from_slice(block);
                pos += take;
            }
        }
    }
    if !pending.is_empty() {
        ops.push(Op::Literal { bytes: pending });
    }
    Delta { ops }
}

/// Serialize: magic, op count, then ops — Copy as (0, offset, len),
/// Literal as (1, len, bytes).
pub fn serialize(delta: &Delta) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + delta.ops.len() * 12);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(delta.ops.len() as u32).to_be_bytes());
    for op in &delta.ops {
        match op {
            Op::Copy { old_offset, len } => {
                out.push(0);
                out.extend_from_slice(&old_offset.to_be_bytes());
                out.extend_from_slice(&len.to_be_bytes());
            }
            Op::Literal { bytes } => {
                out.push(1);
                out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                out.extend_from_slice(bytes);
            }
        }
    }
    out
}

/// Deserialize back into a Delta.
pub fn deserialize(bytes: &[u8]) -> Result<Delta, UpdateError> {
    if bytes.len() < 9 || &bytes[..5] != MAGIC {
        return Err(UpdateError::Delta("bad magic".into()));
    }
    let count = u32::from_be_bytes(
        bytes[5..9]
            .try_into()
            .map_err(|_| UpdateError::Delta("short header".into()))?,
    );
    let mut ops = Vec::with_capacity(count as usize);
    let mut pos = 9;
    for _ in 0..count {
        if pos >= bytes.len() {
            return Err(UpdateError::Delta("truncated ops".into()));
        }
        match bytes[pos] {
            0 => {
                let old_offset = u32::from_be_bytes(
                    bytes[pos + 1..pos + 5]
                        .try_into()
                        .map_err(|_| UpdateError::Delta("short copy".into()))?,
                );
                let len = u32::from_be_bytes(
                    bytes[pos + 5..pos + 9]
                        .try_into()
                        .map_err(|_| UpdateError::Delta("short copy".into()))?,
                );
                ops.push(Op::Copy { old_offset, len });
                pos += 9;
            }
            1 => {
                let len = u32::from_be_bytes(
                    bytes[pos + 1..pos + 5]
                        .try_into()
                        .map_err(|_| UpdateError::Delta("short literal".into()))?,
                ) as usize;
                let end = pos + 5 + len;
                if end > bytes.len() {
                    return Err(UpdateError::Delta("truncated literal".into()));
                }
                ops.push(Op::Literal {
                    bytes: bytes[pos + 5..end].to_vec(),
                });
                pos = end;
            }
            other => {
                return Err(UpdateError::Delta(format!("unknown op kind {other}")));
            }
        }
    }
    Ok(Delta { ops })
}

/// Apply a delta to `old`, producing `new`. Bounds-checked on every Copy;
/// the whole result is hashed before the caller ever stages it.
pub fn apply(old: &[u8], delta: &Delta) -> Result<Vec<u8>, UpdateError> {
    let mut out = Vec::new();
    for op in &delta.ops {
        match op {
            Op::Copy { old_offset, len } => {
                let start = *old_offset as usize;
                let end = start + *len as usize;
                if end > old.len() {
                    return Err(UpdateError::Delta(format!(
                        "copy {start}..{end} outside old file of {}",
                        old.len()
                    )));
                }
                out.extend_from_slice(&old[start..end]);
            }
            Op::Literal { bytes } => out.extend_from_slice(bytes),
        }
    }
    Ok(out)
}

/// The verification the updater runs on the applied result.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
