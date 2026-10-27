//! The publisher's side: a model rendered as a descriptor whose `sequence` rises only when its
//! content changes, so every publisher of a stack agrees on it.

use crate::{Error, VERSION, content_hash, format, ijson};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a publisher keeps between publishes. Keep it outside anything a wipe deletes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PublishState {
	/// The `sequence` last published.
	pub sequence: u32,
	/// The 0x-prefixed [`content_hash`] of what was published.
	pub content_hash: String,
}

/// A rendered descriptor and the state to keep for the next publish.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Rendered {
	/// The plain descriptor in its canonical form, ready to serve or to [`sign`](crate::sign).
	#[serde(serialize_with = "format::serialize_bytes")]
	#[cfg_attr(feature = "js", tsify(type = "Uint8Array"))]
	pub plain: Vec<u8>,
	/// The state to pass back as `previous` next time.
	pub state: PublishState,
}

/// Renders a model, a descriptor without `$v`, `sequence` and `generation`, as a checked `$v: 1`
/// descriptor in its canonical form. A model's own values for those three fields are ignored.
///
/// `sequence` stays the same while the content matches `previous`, and otherwise rises to the
/// larger of the previous one plus one and `now_unix_seconds`, so a publisher with no saved state
/// still never goes backwards.
pub fn render(
	model: &[u8],
	generation: &str,
	previous: Option<&PublishState>,
	now_unix_seconds: u64,
) -> Result<Rendered, Error> {
	let Value::Object(mut model) = ijson::parse(model)? else {
		return Err(Error::Malformed("a model is a JSON object".into()));
	};
	model.insert("$v".into(), VERSION.into());
	model.insert("generation".into(), generation.into());
	model.insert("sequence".into(), 0.into());
	let content_hash = format::to_hex(&content_hash(&to_bytes(&model))?);
	let sequence = match previous {
		Some(previous) if previous.content_hash == content_hash => u64::from(previous.sequence),
		Some(previous) => (u64::from(previous.sequence) + 1).max(now_unix_seconds),
		None => now_unix_seconds,
	};
	model.insert("sequence".into(), sequence.into());
	let plain = crate::canonical(&to_bytes(&model))?;
	let sequence = u32::try_from(sequence).expect("canonical refuses a sequence above u32");
	Ok(Rendered { plain, state: PublishState { sequence, content_hash } })
}

fn to_bytes(model: &serde_json::Map<String, Value>) -> Vec<u8> {
	serde_json::to_vec(model).expect("a JSON value always serialises")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::Document;

	fn model(services: Value) -> Vec<u8> {
		serde_json::to_vec(&serde_json::json!({
			"id": "local", "name": "Local", "network": "local", "tld": "local",
			"services": services,
			"chains": [{
				"role": "Relay",
				"genesisHash": "0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92",
				"rpc": ["ws://127.0.0.1:9944"],
			}],
		}))
		.unwrap()
	}

	#[test]
	fn sequence_holds_while_nothing_changes_and_rises_when_something_does() {
		let first = render(&model(Value::Array(vec![])), "g1", None, 1_000).unwrap();
		assert_eq!(first.state.sequence, 1_000);
		let again = render(&model(Value::Array(vec![])), "g1", Some(&first.state), 2_000).unwrap();
		assert_eq!(again, first, "republishing the same content keeps sequence and bytes");
		let restarted =
			render(&model(Value::Array(vec![])), "g2", Some(&again.state), 900).unwrap();
		assert_eq!(restarted.state.sequence, 1_001, "a clock behind the state still moves forward");
	}

	#[test]
	fn the_state_hash_is_the_content_hash() {
		let rendered = render(&model(Value::Array(vec![])), "g1", None, 1_000).unwrap();
		let expected = format::to_hex(&content_hash(&rendered.plain).unwrap());
		assert_eq!(rendered.state.content_hash, expected);
	}

	#[test]
	fn a_service_kind_only_newer_readers_know_raises_sequence() {
		let first = render(&model(Value::Array(vec![])), "g1", None, 1_000).unwrap();
		let future =
			model(serde_json::json!([{ "kind": "future-kind", "urls": ["https://a.example/"] }]));
		assert_eq!(render(&future, "g1", Some(&first.state), 900).unwrap().state.sequence, 1_001);
	}

	#[test]
	fn a_model_cannot_set_the_fields_publish_owns() {
		let mut stale: serde_json::Map<String, Value> =
			serde_json::from_slice(&model(Value::Array(vec![]))).unwrap();
		stale.insert("sequence".into(), 7.into());
		stale.insert("generation".into(), "old".into());
		stale.insert("$v".into(), 9.into());
		let rendered = render(&to_bytes(&stale), "g1", None, 1_000).unwrap();
		let parsed = Document::parse(&rendered.plain).unwrap();
		assert_eq!(
			(parsed.descriptor().sequence, parsed.descriptor().generation.as_str()),
			(1_000, "g1")
		);
	}

	#[test]
	fn a_model_that_is_not_an_object_or_breaks_a_rule_is_refused() {
		assert_eq!(
			render(b"[]", "g1", None, 1_000).map_err(|error| error.code()),
			Err("malformed")
		);
		let no_chains =
			br#"{"id":"a","name":"A","network":"n","tld":"t","services":[],"chains":[]}"#;
		assert!(render(no_chains, "g1", None, 1_000).is_err());
	}
}
