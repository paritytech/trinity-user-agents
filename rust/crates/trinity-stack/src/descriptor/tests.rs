use super::*;

const RELAY: &str = r#"[{"role":"Relay","genesisHash":"0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92","rpc":["ws://127.0.0.1:9944"]}]"#;

#[test]
fn deserialising_directly_applies_the_same_rules() {
	let broken = format!(
		r#"{{"$v":1,"sequence":1,"generation":"g","id":"a","name":"A","network":"n","tld":".dot","services":[],"chains":{RELAY}}}"#
	);
	let error = serde_json::from_str::<Descriptor>(&broken).unwrap_err();
	assert!(error.to_string().starts_with("invalid-tld:"), "{error}");
	let valid: Descriptor = serde_json::from_str(&broken.replace(".dot", "dot")).unwrap();
	let round_trip: Descriptor =
		serde_json::from_str(&serde_json::to_string(&valid).unwrap()).unwrap();
	assert_eq!(round_trip, valid);
}

/// The parsed views return what validation decoded, so a reader does not decode the strings a
/// second time.
#[test]
fn test_parsed_views() {
	let document: serde_json::Value =
		serde_json::from_str(include_str!("../../vectors/documents/valid-dotns.json")).unwrap();
	let descriptor = Descriptor::from_json(document).unwrap();

	let people = descriptor.chain("People").unwrap();
	let mut genesis = [0; 32];
	hex::decode_to_slice(&people.genesis_hash[2..], &mut genesis).unwrap();
	assert_eq!(people.genesis(), genesis);

	let dub = descriptor.service("dub").unwrap();
	assert_eq!(dub.urls, ["https://previewnet.substrate.dev/dub/"]);
	assert_eq!(descriptor.service("web-gateway"), None);

	assert_eq!(descriptor.dotns_address("registrar"), Some([0xaa; 20]));
	assert_eq!(descriptor.dotns_address("popController"), None);
}
