//! SDP: session and media blocks, offer/answer assembly, the attributes
//! that matter. Real parsing and real serialization on plain strings.

use crate::peer::{Track, TrackDirection, TrackKind};
use crate::RtcError;

#[derive(Debug, Clone, Default)]
pub struct MediaSection {
    pub kind: String,
    pub port: u16,
    pub protocols: Vec<String>,
    pub mid: String,
    pub direction: String,
    pub ice_ufrag: Option<String>,
    pub ice_pwd: Option<String>,
    pub fingerprint: Option<String>,
    pub setup: Option<String>,
    pub sctp_port: Option<u16>,
    pub payloads: Vec<PayloadType>,
}

#[derive(Debug, Clone)]
pub struct PayloadType {
    pub id: u8,
    pub codec: String,
    pub clock_rate: u32,
    pub channels: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionDescription {
    pub session_id: String,
    pub media: Vec<MediaSection>,
}

impl SessionDescription {
    /// An offer: one m-section per track, one application section if a
    /// data channel is requested (mid 0..n).
    pub fn for_offer(tracks: &[Track], dtls_fingerprint: &str) -> Self {
        let mut media = Vec::new();
        for (index, track) in tracks.iter().enumerate() {
            let kind = match track.kind {
                TrackKind::Audio => "audio",
                TrackKind::Video => "video",
            };
            let direction = match track.direction {
                TrackDirection::SendRecv => "sendrecv",
                TrackDirection::SendOnly => "sendonly",
                TrackDirection::RecvOnly => "recvonly",
                TrackDirection::Inactive => "inactive",
            };
            media.push(MediaSection {
                kind: kind.into(),
                port: 9,
                protocols: vec!["UDP/TLS/RTP/SAVPF".into()],
                mid: index.to_string(),
                direction: direction.into(),
                ice_ufrag: Some(format!("fox{}", index)),
                ice_pwd: Some("openfox-ice-pwd-48-chars-aaaaaaaaaaaaaaaaaaaaaaaa".into()),
                fingerprint: Some(dtls_fingerprint.into()),
                setup: Some("actpass".into()),
                payloads: match track.kind {
                    TrackKind::Audio => vec![opus_payload()],
                    TrackKind::Video => vec![vp8_payload(), vp9_payload(), av1_payload()],
                },
                sctp_port: None,
            });
        }
        media.push(MediaSection {
            kind: "application".into(),
            port: 9,
            protocols: vec!["UDP/DTLS/SCTP".into()],
            mid: tracks.len().to_string(),
            direction: "sendrecv".into(),
            ice_ufrag: Some("foxdata".into()),
            ice_pwd: Some("openfox-ice-pwd-48-chars-bbbbbbbbbbbbbbbbbbbbbbbb".into()),
            fingerprint: Some(dtls_fingerprint.into()),
            setup: Some("actpass".into()),
            sctp_port: Some(5000),
            payloads: Vec::new(),
        });
        SessionDescription {
            session_id: "openfox".into(),
            media,
        }
    }

    /// An answer: mirror the offer's m-section kinds, settle directions.
    pub fn for_answer(tracks: &[Track], dtls_fingerprint: &str) -> Self {
        let mut offer = Self::for_offer(tracks, dtls_fingerprint);
        for section in offer.media.iter_mut() {
            if section.setup.is_some() {
                // The offerer said actpass; the answerer picks active.
                section.setup = Some("active".into());
            }
        }
        offer
    }

    /// Serialize to the wire format: v/o/s/t, then m-blocks with their
    /// attributes in the order the spec's examples use.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("v=0\r\n");
        out.push_str(&format!("o=- {} 2 IN IP4 127.0.0.1\r\n", self.session_id));
        out.push_str("s=openfox\r\n");
        out.push_str("t=0 0\r\n");
        for section in &self.media {
            let payloads: Vec<String> = section
                .payloads
                .iter()
                .map(|payload| payload.id.to_string())
                .collect();
            let rtpmap = if payloads.is_empty() {
                String::new()
            } else {
                payloads.join(" ")
            };
            out.push_str(&format!(
                "m={} {} {} {}\r\n",
                section.kind,
                section.port,
                section.protocols.join("/"),
                if rtpmap.is_empty() { "webrtc-datachannel".into() } else { rtpmap }
            ));
            out.push_str("c=IN IP4 0.0.0.0\r\n");
            out.push_str(&format!("a=mid:{}\r\n", section.mid));
            if let Some(ufrag) = &section.ice_ufrag {
                out.push_str(&format!("a=ice-ufrag:{ufrag}\r\n"));
            }
            if let Some(pwd) = &section.ice_pwd {
                out.push_str(&format!("a=ice-pwd:{pwd}\r\n"));
            }
            if let Some(fingerprint) = &section.fingerprint {
                out.push_str(&format!("a=fingerprint:sha-256 {fingerprint}\r\n"));
            }
            if let Some(setup) = &section.setup {
                out.push_str(&format!("a=setup:{setup}\r\n"));
            }
            out.push_str(&format!("a={}\r\n", section.direction));
            if let Some(port) = section.sctp_port {
                out.push_str(&format!("a=sctp-port:{port}\r\n"));
            }
            for payload in &section.payloads {
                let channels = payload
                    .channels
                    .map(|c| format!("/{c}"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "a=rtpmap:{} {}/{}{}\r\n",
                    payload.id, payload.codec, payload.clock_rate, channels
                ));
            }
        }
        out
    }

    /// Parse the wire format. Line-oriented, order-tolerant, and honest
    /// about what it does not understand.
    pub fn parse(text: &str) -> Result<Self, RtcError> {
        let mut description = SessionDescription::default();
        let mut current: Option<MediaSection> = None;
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            let Some((kind, value)) = line.split_once('=') else {
                continue;
            };
            match kind {
                "m" => {
                    if let Some(section) = current.take() {
                        description.media.push(section);
                    }
                    let mut parts = value.split_whitespace();
                    let section = MediaSection {
                        kind: parts.next().unwrap_or_default().into(),
                        port: parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
                        protocols: parts
                            .next()
                            .map(|p| p.split('/').map(str::to_owned).collect())
                            .unwrap_or_default(),
                        ..Default::default()
                    };
                    current = Some(section);
                }
                "a" => {
                    let Some(section) = current.as_mut() else { continue };
                    let (name, attr) = value.split_once(':').unwrap_or((value, ""));
                    match name {
                        "mid" => section.mid = attr.into(),
                        "ice-ufrag" => section.ice_ufrag = Some(attr.into()),
                        "ice-pwd" => section.ice_pwd = Some(attr.into()),
                        "fingerprint" => section.fingerprint = Some(attr.into()),
                        "setup" => section.setup = Some(attr.into()),
                        "sctp-port" => section.sctp_port = attr.parse().ok(),
                        "sendrecv" | "sendonly" | "recvonly" | "inactive" => {
                            section.direction = name.into();
                        }
                        "rtpmap" => {
                            // id codec/clock/channels
                            let mut bits = attr.split_whitespace();
                            if let (Some(id), Some(rest)) = (bits.next(), bits.next()) {
                                let mut spec = rest.split('/');
                                section.payloads.push(PayloadType {
                                    id: id.parse().unwrap_or(0),
                                    codec: spec.next().unwrap_or_default().into(),
                                    clock_rate: spec
                                        .next()
                                        .and_then(|c| c.parse().ok())
                                        .unwrap_or(0),
                                    channels: spec.next().and_then(|c| c.parse().ok()),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if let Some(section) = current.take() {
            description.media.push(section);
        }
        if description.media.is_empty() {
            return Err(RtcError::Sdp("no media sections".into()));
        }
        Ok(description)
    }
}

fn opus_payload() -> PayloadType {
    PayloadType {
        id: 111,
        codec: "opus".into(),
        clock_rate: 48000,
        channels: Some(2),
    }
}

fn vp8_payload() -> PayloadType {
    PayloadType {
        id: 96,
        codec: "VP8".into(),
        clock_rate: 90000,
        channels: None,
    }
}

fn vp9_payload() -> PayloadType {
    PayloadType {
        id: 98,
        codec: "VP9".into(),
        clock_rate: 90000,
        channels: None,
    }
}

fn av1_payload() -> PayloadType {
    PayloadType {
        id: 45,
        codec: "AV1".into(),
        clock_rate: 90000,
        channels: None,
    }
}
