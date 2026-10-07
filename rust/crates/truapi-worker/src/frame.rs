//! The wire envelope a Worker product speaks: `[requestId: SCALE str][trait:
//! u8][method: u8][message_type: u8][payload...]`, the shape
//! `truapi::frame::ProtocolMessage` encodes on the host side.

use parity_scale_codec::{Decode, Encode};

/// One wire envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Per-message identifier shared by both legs of an exchange.
    pub request_id: String,
    /// Trait discriminant.
    pub trait_id: u8,
    /// Method discriminant within the trait.
    pub method_id: u8,
    /// Which leg this frame carries.
    pub message_type: u8,
    /// The leg's own SCALE-encoded versioned wrapper, inlined.
    pub payload: Vec<u8>,
}

impl Frame {
    /// Encode as `[requestId][trait][method][type][payload...]`.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.request_id.encode();
        bytes.push(self.trait_id);
        bytes.push(self.method_id);
        bytes.push(self.message_type);
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    /// Decode one transport frame; the payload runs to the end of the bytes.
    pub fn decode(mut bytes: &[u8]) -> Option<Self> {
        let request_id = String::decode(&mut bytes).ok()?;
        let [trait_id, method_id, message_type, payload @ ..] = bytes else {
            return None;
        };
        Some(Self {
            request_id,
            trait_id: *trait_id,
            method_id: *method_id,
            message_type: *message_type,
            payload: payload.to_vec(),
        })
    }

    /// A frame addressed to the method `ids`.
    pub fn new(request_id: String, ids: (u8, u8), message_type: u8, payload: Vec<u8>) -> Self {
        Self {
            request_id,
            trait_id: ids.0,
            method_id: ids.1,
            message_type,
            payload,
        }
    }
}
