//! Realtime: peer sessions, SDP, ICE over tokio UDP, data channels. The
//! media path hands frames to `openfox-render`; the protocol path lives
//! here, in the open.

pub mod datachannel;
pub mod ice;
pub mod peer;
pub mod sdp;

pub use datachannel::DataChannel;
pub use ice::{Candidate, CandidateKind, IceAgent};
pub use peer::{PeerSession, TrackDirection, TrackKind};
pub use sdp::SessionDescription;

#[derive(Debug, thiserror::Error)]
pub enum RtcError {
    #[error("sdp: {0}")]
    Sdp(String),
    #[error("ice: {0}")]
    Ice(String),
    #[error("peer session in state {0} cannot do that")]
    WrongState(String),
    #[error("socket io: {0}")]
    Io(#[from] std::io::Error),
    #[error("data channel closed")]
    ChannelClosed,
}
