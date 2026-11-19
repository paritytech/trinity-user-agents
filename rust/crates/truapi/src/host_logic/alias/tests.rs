use super::*;

/// The alias is deterministic in the entropy, product id, and index, and
/// the context is the blake2b-256 of the derivation path.
#[test]
fn alias_is_deterministic_with_expected_context() {
	let entropy = [0xABu8; 16];
	let first =
		RootEntropy::new(entropy.to_vec()).derive_product_alias(&ProductId::for_test("truapi-playground.dot"), 0).unwrap();
	let again =
		RootEntropy::new(entropy.to_vec()).derive_product_alias(&ProductId::for_test("truapi-playground.dot"), 0).unwrap();

	assert_eq!(first.context, again.context);
	assert_eq!(first.alias, again.alias);
	assert_eq!(first.context, blake2b256(b"/product/truapi-playground.dot/0"));
}

#[test]
fn alias_varies_by_product_and_index() {
	let entropy = [0xABu8; 16];
	let base = RootEntropy::new(entropy.to_vec()).derive_product_alias(&ProductId::for_test("a.dot"), 0).unwrap();
	let other_product = RootEntropy::new(entropy.to_vec()).derive_product_alias(&ProductId::for_test("b.dot"), 0).unwrap();
	let other_index = RootEntropy::new(entropy.to_vec()).derive_product_alias(&ProductId::for_test("a.dot"), 1).unwrap();

	assert_ne!(base.alias, other_product.alias);
	assert_ne!(base.alias, other_index.alias);
}
