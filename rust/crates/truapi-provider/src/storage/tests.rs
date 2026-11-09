use super::*;

#[test]
fn a_blob_without_chain_information_is_not_worth_storing() {
	assert!(!carries_chain_information(r#"{"chain":null}"#));
	assert!(!carries_chain_information("{}"));
	assert!(!carries_chain_information("not json"));
	assert!(!carries_chain_information("<too-large>"));
	assert!(!carries_chain_information(""));
	assert!(carries_chain_information(
		r#"{"chain":{"finalized_block_header":"0x00"},"genesisHash":"0x01"}"#
	));
}

#[test]
fn a_blob_without_runtime_code_never_replaces_one_that_has_it() {
	const WITH_CODE: &str = r#"{"chain":{"a":1},"runtimeCode":"AAAA"}"#;
	const WITHOUT_CODE: &str = r#"{"chain":{"a":1}}"#;

	assert!(
		is_worth_storing(WITHOUT_CODE, None),
		"a blob with no runtime code still beats nothing stored"
	);
	assert!(is_worth_storing(WITH_CODE, Some(false)), "gaining the runtime code is an improvement");
	assert!(
		!is_worth_storing(WITHOUT_CODE, Some(true)),
		"losing the runtime code would cost a runtime download every run"
	);
	assert!(
		is_worth_storing(WITH_CODE, Some(true)),
		"a fresher blob of the same quality is still worth storing"
	);
	assert!(
		!is_worth_storing(r#"{"genesisHash":"0x01"}"#, None),
		"a blob with no chain information is worth nothing"
	);
}

/// A generation is an opaque string, so one holding a line break must not be
/// able to end the header early and pass its tail off as the blob.
#[test]
fn a_sealed_blob_opens_only_for_its_generation() {
	let blob = r#"{"chain":{"a":1}}"#;
	let sealed = seal(Some("one\ntwo"), blob);

	let opened =
		[Some("one\ntwo"), Some("one"), None].map(|generation| unseal(generation, sealed.clone()));

	assert_eq!(opened, [Some(blob.to_owned()), None, None]);
}

/// A blob written without the header is not one this provider stored for any
/// generation, so it is not handed to the light client.
#[test]
fn a_blob_without_a_generation_line_is_not_opened() {
	assert_eq!(unseal(None, r#"{"chain":{"a":1}}"#.to_owned()), None);
}
