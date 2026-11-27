use strum::VariantArray;

use super::*;

// The personhood label is the chain's `personhood::PRODUCT_NAME`, and every
// label is the root of the keys hosts derive for that product, so renaming one
// strands the accounts already derived under it.
#[test]
fn the_privileged_labels_are_the_names_reserved_for_them() {
	let labels: Vec<&str> =
		PrivilegedProduct::VARIANTS.iter().map(|product| product.label()).collect();
	assert_eq!(labels, ["peopl", "dim2", "stash"]);
}

#[test]
fn every_privileged_product_is_trusted_on_every_network() {
	// A label that product-id validation rejects or rewrites would never
	// match a normalized id, so the product would be silently untrusted.
	for &product in PrivilegedProduct::VARIANTS {
		for tld in ["dot", "paseo", "fork"] {
			let product_id = product.product_id(tld).expect("a privileged label is a product name");
			assert_eq!(product_id.to_string(), format!("{}.{tld}", product.label()));
			assert!(
				product_id.has_trusted_remote_permissions(),
				"{product_id} must hold remote permissions without a prompt"
			);
		}
	}
}

#[test]
fn a_product_id_names_its_privileged_product_only_by_the_whole_name() {
	let named = |id: &str| ProductId::for_test(id).privileged_product();

	assert_eq!(
		["peopl.dot", "dim2.paseo", "stash.testnet"].map(named),
		[
			Some(PrivilegedProduct::Personhood),
			Some(PrivilegedProduct::Game),
			Some(PrivilegedProduct::Collectibles)
		]
	);
	assert_eq!(
		["app.peopl.dot", "notdim2.dot", "fund.dot", "localhost:3000"].map(named),
		[None; 4],
		"another publisher's name or a development id must not borrow a privilege"
	);
}

#[test]
fn a_privileged_product_needs_a_network_it_can_be_named_on() {
	assert!(PrivilegedProduct::Personhood.product_id("my_tld").is_err());
}
