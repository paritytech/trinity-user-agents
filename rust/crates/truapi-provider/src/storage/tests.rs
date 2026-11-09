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

/// The provider reads back only what it stored for its own stack and
/// generation, so a store that ignores the stack cannot hand a fork its
/// source's state. Both are opaque strings, so one holding a line break must
/// not be able to end the header early.
#[test]
fn a_sealed_blob_opens_only_for_its_stack_and_generation() {
	let blob = r#"{"chain":{"a":1}}"#;
	let sealed = seal("stack", "one\ntwo", blob);

	let opened = [("stack", "one\ntwo"), ("stack", "one"), ("fork", "one\ntwo")]
		.map(|(stack, generation)| unseal(stack, generation, sealed.clone()));

	assert_eq!(opened, [Some(blob.to_owned()), None, None]);
}

/// A blob stored without the header was not stored by a stack's provider.
#[test]
fn a_blob_without_a_header_is_not_opened() {
	assert_eq!(unseal("stack", "one", r#"{"chain":{"a":1}}"#.to_owned()), None);
}
