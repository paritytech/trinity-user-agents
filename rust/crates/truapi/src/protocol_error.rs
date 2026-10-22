//! Tolerant decoding of the protocol-error frame payload.

use parity_scale_codec::{Decode, Error as CodecError};
use scale_info::{TypeDef, TypeInfo};

use crate::frame::{ProtocolErrorV1, VersionedProtocolError};

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
/// `V1(UnsupportedMessage)` or `V1(UnsupportedVersion)` is corruption, not a
/// newer peer, and still fails.
pub fn decode_protocol_error_payload(
	payload: &[u8],
) -> Result<Option<VersionedProtocolError>, CodecError> {
	match (payload.first(), payload.get(1)) {
		(None, _) => Err("protocol error payload is empty".into()),
		// A version from a later protocol.
		(Some(version), _) if *version != 0 => Ok(None),
		// A `V1` variant from a later protocol.
		(Some(_), Some(variant)) if !is_known_v1_variant(*variant) => Ok(None),
		// A `V1` variant this build knows.
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

/// Whether `index` encodes a [`ProtocolErrorV1`] variant this build knows, read
/// from the enum itself so a new variant is known the moment it is declared.
fn is_known_v1_variant(index: u8) -> bool {
	matches!(
		ProtocolErrorV1::type_info().type_def,
		TypeDef::Variant(def) if def.variants.iter().any(|variant| variant.index == index)
	)
}
