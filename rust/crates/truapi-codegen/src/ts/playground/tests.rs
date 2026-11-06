use super::*;

#[test]
fn name_to_kebab_id_handles_simple() {
	assert_eq!(name_to_kebab_id("HostAccountGetRequest"), "host-account-get-request");
	assert_eq!(name_to_kebab_id("JsonRpcSubscription"), "json-rpc-subscription");
}

#[test]
fn name_to_kebab_id_acronym_boundary() {
	assert_eq!(name_to_kebab_id("URLPath"), "url-path");
	assert_eq!(name_to_kebab_id("IOError"), "io-error");
}

#[test]
fn data_type_id_returns_none_for_non_named() {
	assert_eq!(data_type_id_from_ts("undefined"), None);
	assert_eq!(data_type_id_from_ts("void"), None);
	assert_eq!(data_type_id_from_ts("T.Foo[]"), None);
	assert_eq!(data_type_id_from_ts("string"), None);
	assert_eq!(data_type_id_from_ts(""), None);
}

#[test]
fn validate_example_docs_accepts_ts_fence() {
	let docs = "Summary line.\n\n```ts\nconst x = 1;\n```";
	assert!(validate_example_docs("CoinPayment", "create_purse", Some(docs)).is_ok());
}

#[test]
fn validate_example_docs_rejects_missing_example() {
	let docs = "Summary line with no example.";
	let err = validate_example_docs("CoinPayment", "create_purse", Some(docs)).unwrap_err();
	assert!(err.to_string().contains("no ```ts example"));
}

#[test]
fn validate_example_docs_rejects_missing_docs() {
	let err = validate_example_docs("CoinPayment", "create_purse", None).unwrap_err();
	assert!(err.to_string().contains("no ```ts example"));
}

#[test]
fn validate_example_docs_rejects_unrecognized_label() {
	let docs = "Summary.\n\n```truapi-client-example\nconst x = 1;\n```";
	let err = validate_example_docs("CoinPayment", "create_purse", Some(docs)).unwrap_err();
	assert!(err.to_string().contains("code fence left in its description"));
}

#[test]
fn data_type_id_kebabs_named_types() {
	assert_eq!(
		data_type_id_from_ts("T.HostAccountGetRequest"),
		Some("host-account-get-request".to_string())
	);
	assert_eq!(
		data_type_id_from_ts("HostAccountGetResponse"),
		Some("host-account-get-response".to_string())
	);
}
