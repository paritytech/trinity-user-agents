use super::*;

/// A request can name several products, such as a key handle and a context.
/// It passes only if the stack serves all of them, and the error names the
/// first one that fails.
#[test]
fn a_value_is_served_only_when_every_product_it_names_is() {
	let alias = |key_handle: &str, context: &str| HostAccountGetAliasRequest {
		key_handle: ProductAccountId {
			dot_ns_identifier: ProductId::for_test(key_handle),
			derivation_index: truapi::latest::DerivationIndex::Index(0),
		},
		context: crate::test_support::product_proof_context(context),
		ring_location: crate::test_support::ring_location_fixture(),
	};
	let refused = |key_handle, context| {
		serve(&crate::test_support::paseo_test_stack(), alias(key_handle, context))
			.err()
			.map(|unserved| unserved.product_id)
	};

	assert_eq!(
		[
			refused("app.paseo", "app.dot"),
			refused("app.paseo", "app.testnet"),
			refused("app.testnet", "app.fork"),
		],
		[None, Some(ProductId::for_test("app.testnet")), Some(ProductId::for_test("app.testnet"))]
	);
}
