//! Codec truth tables. `can_play` answers the media element's
//! `canPlayType` question from a real table, not optimism, and
//! `hardware_decode` says which platform decoder family a codec routes to.
//! Honest table: platform decoders are declared, not emulated; `No` means
//! `No` — not "plays slowly", not "probably fine".

/// The media-element answer: probably, maybe, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playability {
    /// Decodable; the element will play it.
    Probably,
    /// The container is known but the codec claim is not; the element may
    /// play once real frames are sniffed.
    Maybe,
    /// Will not play. Not at all.
    No,
}

/// Which decode path a codec takes on the current machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareBackend {
    /// Direct3D 11 Video Acceleration — the Windows path.
    D3D11VA,
    /// Android's MediaCodec surface pipeline.
    MediaCodec,
    /// Software decode. Always works, always costs battery.
    Cpu,
}

/// The machine we are declaring decoders for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Android,
    Other,
}

/// (container, codec, verdict); an empty codec means the bare container.
/// Every row is a real, shippable answer: HEVC is `No` on patent policy,
/// HLS is `No` because there is no adaptive streaming stack in v1, and
/// `text/vtt` is `Probably` because tracks are text, not decode.
const TABLE: &[(&str, &str, Playability)] = &[
    ("video/webm", "vp9", Playability::Probably),
    ("video/webm", "vp8", Playability::Probably),
    ("video/webm", "av1", Playability::Probably),
    ("video/webm", "", Playability::Maybe),
    ("video/mp4", "avc1.42E01E", Playability::Probably),
    ("video/mp4", "avc1.4D401F", Playability::Probably),
    ("video/mp4", "hevc", Playability::No), // patent policy: no HEVC, period
    ("video/mp4", "av01.0.05M.08", Playability::Probably),
    ("video/mp4", "vp9", Playability::Probably),
    ("video/mp4", "", Playability::Maybe),
    ("video/ogg", "theora", Playability::Probably),
    ("video/ogg", "", Playability::Maybe),
    ("audio/ogg", "opus", Playability::Probably),
    ("audio/ogg", "vorbis", Playability::Probably),
    ("audio/ogg", "", Playability::Maybe),
    ("audio/webm", "opus", Playability::Probably),
    ("audio/webm", "vorbis", Playability::Probably),
    ("audio/webm", "", Playability::Maybe),
    ("audio/mpeg", "", Playability::Probably),
    ("audio/mp4", "mp4a.40.2", Playability::Probably),
    ("audio/mp4", "", Playability::Maybe),
    ("audio/flac", "", Playability::Probably),
    ("audio/wav", "", Playability::Probably),
    ("audio/aac", "", Playability::Probably),
    ("audio/opus", "", Playability::Probably),
    ("application/vnd.apple.mpegurl", "", Playability::No), // no adaptive streaming v1
    ("text/vtt", "", Playability::Probably),                // tracks, not decode
];

/// Split a MIME string into (lowercased base type, lowercased codec or
/// `None`). Quotes around the codec value are stripped; the web sends both
/// quoted and bare forms.
fn split_mime(mime: &str) -> (String, Option<String>) {
    let trimmed = mime.trim();
    let (base, params) = match trimmed.split_once(';') {
        Some((b, rest)) => (b, rest),
        None => (trimmed, ""),
    };
    let codec = params.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        if k.trim().eq_ignore_ascii_case("codecs") {
            Some(v.trim().trim_matches('"').to_ascii_lowercase())
        } else {
            None
        }
    });
    (base.trim().to_ascii_lowercase(), codec)
}

/// Answer `canPlayType` for a MIME string. Exact (container, codec) pairs
/// come from the table; a known container with an unknown codec is `Maybe`;
/// an unknown container is `No`. There is no third kind of optimism.
pub fn can_play(mime: &str) -> Playability {
    let (base, codec) = split_mime(mime);
    if !TABLE.iter().any(|(b, _, _)| *b == base) {
        return Playability::No;
    }
    match codec.as_deref() {
        None => TABLE
            .iter()
            .find(|(b, c, _)| *b == base && c.is_empty())
            .map_or(Playability::Maybe, |(_, _, v)| *v),
        Some(codec) => TABLE
            .iter()
            .find(|(b, c, _)| *b == base && c.to_ascii_lowercase() == codec)
            .map_or(Playability::Maybe, |(_, _, v)| *v),
    }
}

/// Declare the hardware decode path for a codec on a platform. The VP9,
/// AV1, and H.264 families route to D3D11VA on Windows and MediaCodec on
/// Android; everything else — including VP8, whose hardware support is a
/// museum piece — decodes on the CPU. Decoders are declared, not emulated:
/// if the platform lacks the hardware, the honest answer is `Cpu`, and a
/// codec `can_play` calls `No` never reaches a decoder at all.
pub fn hardware_decode(mime: &str, platform: Platform) -> HardwareBackend {
    let (_, codec) = split_mime(mime);
    let hw = match codec.as_deref() {
        Some(c) => {
            c == "vp9"
                || c.starts_with("vp9.")
                || c == "av1"
                || c.starts_with("av01")
                || c.starts_with("avc1")
                || c.starts_with("avc3")
                || c == "h264"
        }
        None => false,
    };
    if hw {
        match platform {
            Platform::Windows => HardwareBackend::D3D11VA,
            Platform::Android => HardwareBackend::MediaCodec,
            Platform::Other => HardwareBackend::Cpu,
        }
    } else {
        HardwareBackend::Cpu
    }
}
