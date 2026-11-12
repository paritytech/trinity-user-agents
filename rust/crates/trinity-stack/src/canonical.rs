//! One rendering of a descriptor, so publishers agree on when its content changed.

use crate::{Document, Error, ijson};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// A plain descriptor rendered with sorted keys, indented, and ending in a newline, unknown
/// members kept. Two publishes of the same content are byte-identical, so a reader never sees new
/// bytes under an old `sequence`.
pub fn canonical(descriptor: &[u8]) -> Result<Vec<u8>, Error> {
	let Document::Plain(_) = Document::parse(descriptor)? else {
		return Err(Error::Malformed("canonical takes a plain descriptor, not an envelope".into()));
	};
	let mut bytes = serde_json::to_vec_pretty(&sorted(ijson::parse(descriptor)?))
		.expect("a JSON value always serialises");
	bytes.push(b'\n');
	Ok(bytes)
}

/// SHA-256 of the canonical form with `sequence` set to 0: equal exactly when two descriptors
/// publish the same bytes but for `sequence`. Unknown fields and service kinds count, so an
/// addition only newer readers understand still raises `sequence`.
pub fn content_hash(descriptor: &[u8]) -> Result<[u8; 32], Error> {
	let mut document = ijson::parse(&canonical(descriptor)?)?;
	document["sequence"] = 0.into();
	let bytes = serde_json::to_vec(&sorted(document)).expect("a JSON value always serialises");
	Ok(Sha256::digest(bytes).into())
}

/// Rebuilds every object with its keys in order, whichever order serde_json keeps.
fn sorted(value: Value) -> Value {
	match value {
		Value::Object(members) => {
			let mut members: Vec<_> = members.into_iter().collect();
			members.sort_by(|(left, _), (right, _)| left.cmp(right));
			Value::Object(
				members
					.into_iter()
					.map(|(name, value)| (name, sorted(value)))
					.collect::<Map<_, _>>(),
			)
		},
		Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
		other => other,
	}
}

#[cfg(test)]
mod tests;
