use super::*;

fn config(json: &str) -> LocalProductConfig {
	serde_json::from_str(json).expect("the config parses")
}

#[test]
fn a_config_carrying_no_grants_still_reads() {
	let parsed = config(r#"{"productName":"dim2.paseo"}"#);
	assert_eq!(parsed.product_name, "dim2.paseo");
	assert!(parsed.trusted_products.is_empty());
}

#[test]
fn the_product_name_is_normalized_like_any_product_id() {
	let parsed = config(r#"{"productName":" DIM2.Paseo "}"#);
	assert_eq!(parsed.product_name, "dim2.paseo");
}

#[test]
fn a_product_name_that_is_not_a_product_id_fails_the_read_and_names_the_value() {
	let error = serde_json::from_str::<LocalProductConfig>(r#"{"productName":"example.com"}"#)
		.expect_err("example.com is not a product id");
	assert!(error.to_string().contains(r#""example.com""#), "{error}");
}

#[test]
fn fields_the_host_does_not_read_are_not_errors() {
	// The publisher owns most of this file; the host reads two fields of it.
	let parsed = config(
		r#"{"productName":"peopl.paseo","description":"d","icon":"./icon.png",
            "app":{"root":"./dist","appVersion":[1,0,0]},
            "trustedProducts":{"dim2":["storage"]}}"#,
	);
	assert_eq!(parsed.trusted_products["dim2"], vec!["storage".to_string()]);
}

#[test]
fn the_seeded_manifest_carries_the_declared_grants() {
	let parsed = config(
		r#"{"productName":"peopl.paseo","displayName":"Personhood",
            "trustedProducts":{"dim2":["storage"],"stash":["all"]}}"#,
	);
	let json = parsed.root_manifest_json();
	assert!(json.contains(r#""displayName":"Personhood""#));
	assert!(json.contains(r#""dim2":["storage"]"#));
	assert!(json.contains(r#""stash":["all"]"#));
	assert!(json.contains(r#""$v":1"#));
}

#[test]
fn a_scope_the_core_does_not_know_survives_into_the_manifest() {
	// The host does not filter the developer's values. An unrecognised
	// scope must reach the parser and be ignored there, so local behaviour
	// matches what the same document would do on chain.
	let parsed =
		config(r#"{"productName":"peopl.paseo","trustedProducts":{"dim2":["storage-write"]}}"#);
	assert!(parsed.root_manifest_json().contains(r#""dim2":["storage-write"]"#));
}

#[test]
fn the_transcript_names_the_product_and_what_it_grants() {
	let parsed = config(r#"{"productName":"peopl.paseo","trustedProducts":{"dim2":["storage"]}}"#);
	assert_eq!(parsed.transcript_line(), "peopl.paseo: dim2 -> [storage]");

	let silent = config(r#"{"productName":"dim2.paseo"}"#);
	assert_eq!(silent.transcript_line(), "dim2.paseo: grants nothing");
}
