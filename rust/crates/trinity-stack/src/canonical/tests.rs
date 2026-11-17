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
	let extra = String::from_utf8(plain.clone())
		.unwrap()
		.replacen('{', r#"{"futureField":1,"#, 1);
	assert_ne!(content_hash(&plain), content_hash(extra.as_bytes()));
	assert_ne!(content_hash(&plain), content_hash(&descriptor("", "B")));
}
