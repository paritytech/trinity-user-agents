//! Tolerant decoding of the protocol-error frame payload.

use std::sync::LazyLock;

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
	let (registry, type_id) = &*PROTOCOL_ERROR_TYPES;
	if !indexes_are_known(registry, *type_id, payload) {
		return Ok(None);
	}
	let mut input = payload;
	let error = VersionedProtocolError::decode(&mut input)?;
	if !input.is_empty() {
		return Err("protocol error payload has trailing bytes".into());
	}
	Ok(Some(error))
}

/// [`VersionedProtocolError`]'s type information, built on first use.
static PROTOCOL_ERROR_TYPES: LazyLock<(PortableRegistry, u32)> =
	LazyLock::new(type_registry::<VersionedProtocolError>);

/// `T`'s type information, and the id `T` is registered under.
fn type_registry<T: TypeInfo + 'static>() -> (PortableRegistry, u32) {
	let mut registry = Registry::new();
	let type_id = registry.register_type(&meta_type::<T>()).id;
	(registry.into(), type_id)
}

/// Whether `payload`'s first byte names a variant of the enum `type_id`, and,
/// when that variant wraps one value, whether the rest names that value's.
fn indexes_are_known(registry: &PortableRegistry, type_id: u32, payload: &[u8]) -> bool {
	match (payload.split_first(), registry.resolve(type_id).map(|ty| &ty.type_def)) {
		(Some((&index, rest)), Some(TypeDef::Variant(def))) => def
			.variants
			.iter()
			.find(|variant| variant.index == index)
			.is_some_and(|variant| match variant.fields.as_slice() {
				[field] if field.name.is_none() => indexes_are_known(registry, field.ty.id, rest),
				_ => true,
			}),
		// The payload ended, or fields follow: decoding checks the rest.
		_ => true,
	}
}

#[cfg(test)]
mod tests;
