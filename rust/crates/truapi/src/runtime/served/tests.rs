use super::*;

/// A request can name several products, such as a key handle and a context.
/// It passes only if the stack serves all of them, and the error names the
/// first one that fails. What passes names each product by the id the stack
/// serves it under, so the account holder never sees a `dot` id on a stack
/// with another TLD.
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
	let served = |key_handle, context| {
		serve(&crate::test_support::paseo_test_stack(), alias(key_handle, context))
			.map(|request| {
				let request = request.into_inner();
				[request.key_handle.dot_ns_identifier, request.context.product_id].map(String::from)
			})
			.map_err(|unserved| String::from(unserved.product_id))
	};

	assert_eq!(
		[
			served("app.paseo", "app.dot"),
			served("app.dot", "localhost:3000"),
			served("app.paseo", "app.testnet"),
			served("app.testnet", "app.fork"),
		],
		[
			Ok(["app.paseo".to_string(), "app.paseo".to_string()]),
			Ok(["app.paseo".to_string(), "localhost:3000".to_string()]),
			Err("app.testnet".to_string()),
			Err("app.testnet".to_string()),
		]
	);
}
