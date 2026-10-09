//! Tolerant decoding of the protocol-error frame payload.

use parity_scale_codec::{Decode, Error as CodecError};

use crate::frame::VersionedProtocolError;

/// Decode a [`crate::frame::PROTOCOL_ERROR_KEY`] frame's payload.
///
/// `Ok(None)` is a protocol error this build does not know: a version or a
/// variant index it has never heard of, from a peer built against a later
/// protocol. That must not fail the frame. This is the one address every peer
/// answers on, so a peer that rejects an unrecognised payload here cannot be
/// told anything new without breaking the connection, and the channel would be
/// frozen at whatever shape shipped first. A caller that gets `None` should
/// settle the correlated call as an unspecified protocol failure and carry on.
///
/// A payload this build does recognise stays strict: a truncated or over-long
/// `V1(UnsupportedMessage)` is corruption, not a newer peer, and still fails.
pub fn decode_protocol_error_payload(
	payload: &[u8],
) -> Result<Option<VersionedProtocolError>, CodecError> {
	match (payload.first(), payload.get(1)) {
		(None, _) => Err("protocol error payload is empty".into()),
		// A version from a later protocol.
		(Some(version), _) if *version != 0 => Ok(None),
		// A `V1` variant from a later protocol.
		(Some(_), Some(variant)) if *variant != 0 => Ok(None),
		// `V1(UnsupportedMessage)`, the only shape this build knows.
		_ => {
			let mut input = payload;
			let error = VersionedProtocolError::decode(&mut input)?;
			if !input.is_empty() {
				return Err("protocol error payload has trailing bytes".into());
			}
			Ok(Some(error))
		},
	}
}
