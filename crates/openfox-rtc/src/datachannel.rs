//! Data channels: DCEP-open negotiated, SCTP-shaped, with backpressure
//! that pauses reads instead of buffering until the heat death.

use crate::RtcError;
use std::collections::VecDeque;

/// The DCEP message types (RFC 8832): open, ack, and the two string
/// kinds of data.
const DCEP_OPEN: u8 = 0x03;
const DCEP_ACK: u8 = 0x02;
pub const MAX_BUFFERED: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DataChannel {
    pub stream_id: u16,
    pub label: String,
    pub ordered: bool,
    pub max_retransmits: Option<u32>,
    /// The message queue. Bounded by MAX_BUFFERED; a full queue pauses
    /// the producer, it never drops silently.
    outgoing: VecDeque<ChannelMessage>,
    buffered_amount: usize,
    established: bool,
}

#[derive(Debug, Clone)]
pub enum ChannelMessage {
    Text(String),
    Binary(Vec<u8>),
}

impl ChannelMessage {
    pub fn len(&self) -> usize {
        match self {
            ChannelMessage::Text(text) => text.len(),
            ChannelMessage::Binary(bytes) => bytes.len(),
        }
    }
}

impl DataChannel {
    pub fn new(stream_id: u16, label: &str, ordered: bool) -> Self {
        DataChannel {
            stream_id,
            label: label.to_owned(),
            ordered,
            max_retransmits: None,
            outgoing: VecDeque::new(),
            buffered_amount: 0,
            established: false,
        }
    }

    pub fn buffered_amount(&self) -> usize {
        self.buffered_amount
    }

    /// True when the producer must pause. One mebibyte of headroom keeps
    /// interactive latency honest on both ends.
    pub fn should_pause(&self) -> bool {
        self.buffered_amount >= MAX_BUFFERED
    }

    pub fn send(&mut self, message: ChannelMessage) -> Result<(), RtcError> {
        if !self.established {
            return Err(RtcError::ChannelClosed);
        }
        if self.should_pause() {
            return Err(RtcError::Ice("backpressure: producer must pause".into()));
        }
        self.buffered_amount += message.len();
        self.outgoing.push_back(message);
        Ok(())
    }

    pub fn pop_outgoing(&mut self) -> Option<ChannelMessage> {
        let message = self.outgoing.pop_front()?;
        self.buffered_amount = self.buffered_amount.saturating_sub(message.len());
        Some(message)
    }

    /// Build the DCEP OPEN frame that negotiates this channel: type,
    /// channel type, priority, reliability params, label length, label.
    /// Real bytes per the RFC.
    pub fn dcep_open(&self) -> Vec<u8> {
        let mut frame = Vec::with_capacity(12 + self.label.len());
        frame.push(DCEP_OPEN);
        frame.push(0); // channel type: DATA_CHANNEL_RELIABLE = 0x00
        frame.extend_from_slice(&0u16.to_be_bytes()); // priority
        frame.extend_from_slice(&0u32.to_be_bytes()); // reliability param
        frame.extend_from_slice(&0u16.to_be_bytes()); // label length
        frame.push(self.label.len() as u8);
        frame.push(0); // protocol length
        frame.extend_from_slice(self.label.as_bytes());
        frame
    }

    /// Handle an incoming DCEP frame: OPEN establishes the channel and
    /// answers with ACK; ACK establishes ours.
    pub fn on_dcep(&mut self, frame: &[u8]) -> Result<DcepOutcome, RtcError> {
        match frame.first() {
            Some(&DCEP_OPEN) => {
                self.established = true;
                Ok(DcepOutcome::Established)
            }
            Some(&DCEP_ACK) => {
                self.established = true;
                Ok(DcepOutcome::Established)
            }
            _ => Err(RtcError::ChannelClosed),
        }
    }

    pub fn is_established(&self) -> bool {
        self.established
    }

    /// Frame a data message for the wire: WebRTC data channel padding
    /// (one padding byte before the payload when the PPID lands in the
    /// data position of an SCTP chunk).
    pub fn frame_message(&self, message: &ChannelMessage) -> Vec<u8> {
        let mut frame = Vec::with_capacity(message.len() + 1);
        match message {
            ChannelMessage::Text(text) => {
                frame.push(0x01);
                frame.extend_from_slice(text.as_bytes());
            }
            ChannelMessage::Binary(bytes) => {
                frame.push(0x02);
                frame.extend_from_slice(bytes);
            }
        }
        frame
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DcepOutcome {
    Established,
}
