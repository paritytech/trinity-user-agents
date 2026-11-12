use super::*;
use crate::host_logic::product_account::{
	SR25519_SIGNING_CONTEXT, derive_product_keypair, derive_root_keypair_from_entropy,
};

fn fixture() -> (Keypair, ProductId, [u8; FIELD_KEY_BINDING_LEN]) {
	let product = ProductId::for_test("market.dot");
	let root = derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let keypair = derive_product_keypair(&root, &product, [7; 32]).unwrap();
	let mut binding = [0u8; FIELD_KEY_BINDING_LEN];
	for (index, byte) in binding.iter_mut().enumerate() {
		*byte = u8::try_from(index).unwrap();
	}
	(keypair, product, binding)
}

#[test]
fn a_binding_verifies_only_for_what_it_names() {
	let (keypair, product, binding) = fixture();
	let account = keypair.public.to_bytes();
	let signature = sign_field_key_binding(&keypair, &product, &binding);

	let mut other_binding = binding;
	other_binding[20] ^= 1;
	let mut other_account = account;
	other_account[0] ^= 1;
	let mut other_signature = signature;
	other_signature[5] ^= 1;
	assert_eq!(
		[
			verify_field_key_binding(&product, &account, &binding, &signature),
			verify_field_key_binding(
				&ProductId::for_test("other.dot"),
				&account,
				&binding,
				&signature
			),
			verify_field_key_binding(&product, &account, &other_binding, &signature),
			verify_field_key_binding(&product, &other_account, &binding, &signature),
			verify_field_key_binding(&product, &account, &binding, &other_signature),
		],
		[true, false, false, false, false]
	);
}

/// A product can obtain a `substrate`-context signature over any short bytes
/// of its own account, for example through `signPayload` with no extensions.
/// Such a signature over the exact binding message must not pass, and a
/// binding must not pass as such a signature.
#[test]
fn the_signing_context_keeps_product_signatures_out() {
	let (keypair, product, binding) = fixture();
	let account = keypair.public.to_bytes();
	let message = binding_message(&product, &account, &binding);
	let product_signature = keypair
		.secret
		.sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
		.to_bytes();
	let binding_signature =
		Signature::from_bytes(&sign_field_key_binding(&keypair, &product, &binding)).unwrap();

	assert_eq!(
		(
			verify_field_key_binding(&product, &account, &binding, &product_signature),
			keypair
				.public
				.verify_simple(SR25519_SIGNING_CONTEXT, &message, &binding_signature)
				.is_ok(),
		),
		(false, false)
	);
}
