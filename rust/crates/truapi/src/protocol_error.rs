//! Tolerant decoding of the protocol-error frame payload.

use parity_scale_codec::{Decode, Error as CodecError};
use scale_info::{PortableRegistry, Registry, TypeDef, TypeInfo, meta_type};

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
/// A known error that is truncated or has extra bytes is rejected as corrupt.
pub fn decode_protocol_error_payload(
	payload: &[u8],
) -> Result<Option<VersionedProtocolError>, CodecError> {
	if payload.is_empty() {
		return Err("protocol error payload is empty".into());
	}
	if !names_known_variants::<VersionedProtocolError>(payload) {
		return Ok(None);
	}
	let mut input = payload;
	let error = VersionedProtocolError::decode(&mut input)?;
	if !input.is_empty() {
		return Err("protocol error payload has trailing bytes".into());
	}
	Ok(Some(error))
}

/// Whether the variant indexes leading `payload` all name variants of `T`: the
/// version, then the error within that version, and so on while a variant
/// wraps a single enum. Read from `T`'s type information, so a version or an
/// error is known as soon as it is declared.
fn names_known_variants<T: TypeInfo + 'static>(payload: &[u8]) -> bool {
	let mut registry = Registry::new();
	let mut type_id = registry.register_type(&meta_type::<T>()).id;
	let registry = PortableRegistry::from(registry);
	for &index in payload {
		let Some(TypeDef::Variant(def)) = registry.resolve(type_id).map(|ty| &ty.type_def) else {
			return true;
		};
		let Some(variant) = def.variants.iter().find(|variant| variant.index == index) else {
			return false;
		};
		// A variant that wraps one value puts that value's index next; any
		// other variant starts its fields.
		match variant.fields.as_slice() {
			[field] if field.name.is_none() => type_id = field.ty.id,
			_ => return true,
		}
	}
	true
}

#[cfg(test)]
mod tests {
	use parity_scale_codec::Encode;

	use super::*;

	/// A later version of the protocol error needs no change to the decoder:
	/// its errors are read from the type like V1's.
	#[test]
	fn the_errors_of_every_version_are_read_from_the_type() {
		#[derive(Encode, TypeInfo)]
		enum Versioned {
			V1(First),
			V2(Second),
		}
		#[derive(Encode, TypeInfo)]
		enum First {
			Only { id: u8 },
		}
		#[derive(Encode, TypeInfo)]
		enum Second {
			Only { id: u8 },
			Added { id: u8, version: u8 },
		}

		let payloads = [
			Versioned::V1(First::Only { id: 7 }).encode(),
			Versioned::V2(Second::Only { id: 7 }).encode(),
			Versioned::V2(Second::Added { id: 7, version: 2 }).encode(),
			// An error V1 does not have.
			vec![0, 1, 7],
			// An error V2 does not have.
			vec![1, 2, 7],
			// A version that does not exist.
			vec![2, 0, 7],
		];
		assert_eq!(
			payloads.map(|payload| names_known_variants::<Versioned>(&payload)),
			[true, true, true, false, false, false],
		);
	}
}
