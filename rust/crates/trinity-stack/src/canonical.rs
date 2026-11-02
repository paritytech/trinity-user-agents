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
mod tests {
	use super::*;

	fn descriptor(extra: &str, name: &str) -> Vec<u8> {
		format!(
			r#"{{"$v":1,"sequence":1,"generation":"g","id":"a","name":"{name}","network":"n","tld":"t","services":[{extra}],"chains":[{{"role":"Relay","genesisHash":"0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92","rpc":["ws://127.0.0.1:9944"]}}]}}"#
		)
		.into_bytes()
	}

	#[test]
	fn key_order_and_sequence_do_not_change_the_hash() {
		let one = descriptor("", "A");
		let reordered = br#"{"chains":[{"rpc":["ws://127.0.0.1:9944"],"genesisHash":"0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92","role":"Relay"}],"tld":"t","services":[],"sequence":1,"network":"n","name":"A","id":"a","generation":"g","$v":1}"#;
		assert_eq!(canonical(&one), canonical(reordered), "same content renders the same bytes");
		let later = String::from_utf8(one.clone())
			.unwrap()
			.replace(r#""sequence":1"#, r#""sequence":9"#);
		assert_eq!(content_hash(&one), content_hash(later.as_bytes()));
	}

	#[test]
	fn an_unknown_service_kind_or_field_is_content() {
		let plain = descriptor("", "A");
		let future = descriptor(r#"{"kind":"future-kind","urls":["https://a.example/"]}"#, "A");
		assert_ne!(content_hash(&plain), content_hash(&future));
		let extra =
			String::from_utf8(plain.clone())
				.unwrap()
				.replacen('{', r#"{"futureField":1,"#, 1);
		assert_ne!(content_hash(&plain), content_hash(extra.as_bytes()));
		assert_ne!(content_hash(&plain), content_hash(&descriptor("", "B")));
	}
}
