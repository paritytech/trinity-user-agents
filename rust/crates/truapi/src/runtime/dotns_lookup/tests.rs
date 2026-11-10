use super::*;

/// An identity lookup names what it reads and nothing about whose account
/// it reads, so the follow id built from it carries no user identifier
/// into the logs that name a connection's follows.
#[test]
fn an_identity_lookup_is_named_without_an_account() {
	assert_eq!(DotnsLookupKind::Identity.to_string(), "identity");
}

/// A manifest lookup still names its product, which identifies what was
/// opened rather than who opened it.
#[test]
fn a_manifest_lookup_is_named_by_its_product() {
	let product_id = ProductId::for_test("shop.paseo");
	assert_eq!(
		DotnsLookupKind::Manifest(&product_id).to_string(),
		format!("manifest:{product_id}"),
	);
}
