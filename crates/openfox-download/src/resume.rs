//! Resume: HTTP Range requests against `.part` files, with ETag and
//! Last-Modified validation so a resumed file is never a frankenfile.

use crate::DownloadError;

#[derive(Debug, Clone)]
pub struct ResumePolicy {
    /// Validate before resuming: the server artifact must be the same one
    /// we started downloading, or we restart. Firefox made this the
    /// non-negotiable default; so is OpenFox.
    pub validate: bool,
}

impl Default for ResumePolicy {
    fn default() -> Self {
        ResumePolicy { validate: true }
    }
}

/// The `.part` sibling of a final path.
pub fn part_path(final_path: &std::path::Path) -> std::path::PathBuf {
    let mut name = final_path.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = match &mut name {
        Some(name) => {
            name.push_str(".part");
            name.clone()
        }
        None => "download.part".to_owned(),
    };
    final_path.with_file_name(name)
}

/// Build the `Range` header for a resume: everything after the bytes we
/// already hold.
pub fn range_header(received: u64) -> String {
    format!("bytes={received}-")
}

/// Decide whether a resume is safe. ETag match is strongest;
/// Last-Modified is acceptable; neither means restart — silent
/// corruption is worse than re-downloading.
pub fn may_resume(
    etag: Option<&str>,
    last_modified: Option<&str>,
    received: u64,
) -> ResumeDecision {
    if received == 0 {
        return ResumeDecision::FreshStart;
    }
    match (etag, last_modified) {
        (Some(_), _) | (_, Some(_)) => ResumeDecision::Resume,
        (None, None) => ResumeDecision::FreshStart,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResumeDecision {
    Resume,
    FreshStart,
}

/// Finalize: a `.part` becomes the final file only when the expected size
/// matches (when the size was known). Anything else stays a `.part` and
/// the state machine says Failed, not Done.
pub fn finalize(
    part: &std::path::Path,
    final_path: &std::path::Path,
    expected: Option<u64>,
    received: u64,
) -> Result<(), DownloadError> {
    if let Some(expected) = expected {
        if expected != received {
            return Err(DownloadError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("expected {expected} bytes, received {received}"),
            )));
        }
    }
    std::fs::rename(part, final_path)?;
    Ok(())
}

/// The byte-source contract the engine implements: read the next chunk of
/// the download body, honoring whatever offset the resume policy chose.
pub trait ByteSource {
    /// Read up to `buf.len()` bytes into `buf`. Returns 0 on EOF.
    fn next_chunk(&mut self, buf: &mut [u8]) -> Result<usize, DownloadError>;
}

impl DownloadError {
    /// Range unsupported mid-flight: the one error that means "start over
    /// with an offset of zero and don't feel bad about it."
    pub fn no_range() -> Self {
        DownloadError::NoRangeSupport
    }
}
