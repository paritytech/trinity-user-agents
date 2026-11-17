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
	let restarted = render(&model(Value::Array(vec![])), "g2", Some(&again.state), 900).unwrap();
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
	assert_eq!(render(b"[]", "g1", None, 1_000).map_err(|error| error.code()), Err("malformed"));
	let no_chains = br#"{"id":"a","name":"A","network":"n","tld":"t","services":[],"chains":[]}"#;
	assert!(render(no_chains, "g1", None, 1_000).is_err());
}
