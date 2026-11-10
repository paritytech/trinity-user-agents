use super::*;

fn fixture_root() -> Keypair {
	derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap()
}

#[test]
fn product_subtrees_are_hard_firewalls() {
	let root = fixture_root();
	let first = derive_product_subtree_keypair(&root, &ProductId::for_test("myapp.dot")).unwrap();
	let second = derive_product_subtree_keypair(&root, &ProductId::for_test("other.dot")).unwrap();

	assert_ne!(first.public, root.public);
	assert_ne!(first.public, second.public);
}

#[test]
fn product_secret_derivation_matches_subtree_public_derivation() {
	let root = fixture_root();
	for (product_id, index) in [
		("myapp.dot", index_bytes(0)),
		("myapp.dot", index_bytes(1)),
		("localhost:3000", index_bytes(7)),
		("myapp.dot", [0xEE; 32]),
	] {
		let product_id = ProductId::for_test(product_id);
		let subtree = derive_product_subtree_keypair(&root, &product_id).unwrap();
		let keypair = derive_product_keypair(&root, &product_id, index).unwrap();
		let public = derive_product_public_key(subtree.public.to_bytes(), index).unwrap();
		let allocated =
			derive_product_keypair_from_subtree_secret(subtree.secret.to_bytes(), index).unwrap();
		assert_eq!(
			keypair.public.to_bytes(),
			public,
			"{product_id}#{index:02x?} secret vs public derivation",
		);
		assert_eq!(keypair.public, allocated.public);
	}
}

#[test]
fn root_public_key_cannot_substitute_for_product_subtree() {
	let root = fixture_root();
	let actual =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	let unsafe_soft_only =
		derive_product_public_key(root.public.to_bytes(), index_bytes(0)).unwrap();

	assert_ne!(actual.public.to_bytes(), unsafe_soft_only);
}

#[test]
fn long_product_id_derives_stably() {
	let root = fixture_root();
	let product_id = ProductId::for_test(
		"w-credentialless-staticblitz-com.local-credentialless.webcontainer-api.dot",
	);
	let subtree = derive_product_subtree_keypair(&root, &product_id).unwrap();
	let from_secret = derive_product_keypair(&root, &product_id, index_bytes(0)).unwrap();
	let from_public = derive_product_public_key(subtree.public.to_bytes(), index_bytes(0)).unwrap();

	assert_eq!(from_secret.public.to_bytes(), from_public);
}

#[test]
fn ss58_address_round_trips_to_public_key() {
	let root = fixture_root();
	let derived = derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0))
		.unwrap()
		.public
		.to_bytes();
	let address = product_public_key_to_address(derived);

	assert_eq!(public_key_from_address(&address), Some(derived));
	assert_eq!(public_key_from_address("not-an-address"), None);
}

#[test]
fn index_bytes_layout_pin() {
	let index = index_bytes(5);
	assert_eq!(&index[..4], &[5, 0, 0, 0]);
	assert_eq!(index[4..], sp_crypto_hashing::blake2_256(b"product-account-index")[..28]);
}

#[test]
fn index_bytes_matches_ios_vector() {
	assert_eq!(
		hex::encode(index_bytes(0)),
		"0000000012e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f"
	);
}

#[test]
fn derivation_index_bytes_maps_both_selector_forms() {
	use truapi::v01::DerivationIndex;

	assert_eq!(derivation_index_bytes(&DerivationIndex::Index(7)), index_bytes(7));
	assert_eq!(derivation_index_bytes(&DerivationIndex::Raw([0xEE; 32])), [0xEE; 32]);
}

#[test]
fn wire_index_derivation_matches_the_mobile_vector() {
	use truapi::v01::DerivationIndex;

	// The JS export composes these two steps over a wire `DerivationIndex`
	// so hosts never build the 32-byte chain code. Pin that composition
	// against the same vector the internal `index_bytes` path produces.
	let root = derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	let subtree = derive_product_subtree_keypair(&root, &ProductId::for_test("myapp.dot")).unwrap();
	let derived = derive_product_public_key(
		subtree.public.to_bytes(),
		derivation_index_bytes(&DerivationIndex::Index(0)),
	)
	.unwrap();

	assert_eq!(
		hex::encode(derived),
		"1c1ae478b564572f806ffa6352b4273d612beb01610b19f4e5bf444521cd5b5c"
	);
	assert_eq!(
		product_public_key_to_address(derived),
		"5ChZBnBw9eDQUMBhnXUKrGMdK5MTfGrca3T1xZZtBQhW8eis"
	);
}

#[test]
fn reserved_product_ids_cover_every_supported_network() {
	let product_ids: Vec<ProductId> = crate::dotns_name::DOTNS_TLDS
		.iter()
		.map(|suffix| personhood_product_id(suffix))
		.collect();
	assert_eq!(
		product_ids,
		[
			ProductId::for_test("peopl.dot"),
			ProductId::for_test("peopl.paseo"),
			ProductId::for_test("peopl.testnet"),
		]
	);
}

#[test]
fn person_ring_vrf_entropy_matches_ios_vectors() {
	let root_entropy: Vec<u8> = (1..=32).collect();
	assert_eq!(
		hex::encode(blake2b256_keyed(&root_entropy, RING_VRF_ROOT_KEY)),
		"372b08255c7798fe3193756296005adc4c44adb9f3986fb718aa98a48b4bf725"
	);
	assert_eq!(
		hex::encode(derive_full_person_ring_vrf_entropy(&root_entropy, "dot")),
		"c47086f94a7f4c05b7afd9f2339d3fea168f3823b5424ba1f7b31043d8ef60af"
	);
	assert_eq!(
		hex::encode(derive_lite_person_ring_vrf_entropy(&root_entropy, "dot")),
		"8d7f5e1510a7e8d813887e100f5a260ec9de60e68695477b93360ee7e3d16a9f"
	);
}

#[test]
fn person_ring_vrf_entropy_follows_the_network_suffix() {
	// Same seed, one person per network. The vectors come from an
	// independent RFC-0022 implementation (`@web3-citizenship/accounts`
	// `fullPersonRingVrfEntropy(entropy, tld)`), which is also what the
	// iOS host derives; a `.dot` key is never a `.paseo` key.
	let root_entropy: Vec<u8> = (1..=32).collect();
	let vectors = [
		(
			"paseo",
			"50a2adfe7b557a72521789d3961795e71619ac8a19f9cb42b581f3fb703d7453",
			"a0e869e303f9828ccfd006682798ab0e91c29ce3f1ce3d93c640449be4a157dc",
		),
		(
			"testnet",
			"b46833f2492571d719072e0a04478433cc8f836fc626c0690bc13e00bd7d1547",
			"574bdaa99061a6a88442c3d90207de487a753fc7da34f960e305e9d9152bb060",
		),
	];
	for (suffix, full, lite) in vectors {
		assert_eq!(
			hex::encode(derive_full_person_ring_vrf_entropy(&root_entropy, suffix)),
			full,
			"full person key under peopl.{suffix}"
		);
		assert_eq!(
			hex::encode(derive_lite_person_ring_vrf_entropy(&root_entropy, suffix)),
			lite,
			"light person key under peopl.{suffix}"
		);
		// The reserved keys are exactly what a `peopl.<suffix>` product
		// registers through RFC-0024 at indexes 0 and 1: no special case.
		let product_id = personhood_product_id(suffix);
		for (index, expected) in [(0u32, full), (1, lite)] {
			assert_eq!(
				hex::encode(
					derive_ring_vrf_entropy(
						&root_entropy,
						&product_id,
						&truapi::v01::DerivationIndex::Index(index),
					)
					.unwrap()
				),
				expected
			);
		}
	}
}

#[test]
fn ring_vrf_domain_entropy_derives_the_same_registered_key_as_the_root() {
	use truapi::v01::DerivationIndex;

	let root_entropy: Vec<u8> = (1..=32).collect();
	let domain =
		derive_ring_vrf_domain_entropy(&root_entropy, &ProductId::for_test("people-provider.dot"))
			.unwrap();
	for index in [DerivationIndex::Index(7), DerivationIndex::Raw([0xEE; 32])] {
		assert_eq!(
			derive_ring_vrf_entropy_from_domain(&domain, &index),
			derive_ring_vrf_entropy(
				&root_entropy,
				&ProductId::for_test("people-provider.dot"),
				&index
			)
			.unwrap()
		);
	}
}

#[test]
fn identity_is_peopl_dot_default_product_account_and_signs() {
	let entropy = [0xAB; 16];
	let identity = derive_identity_keypair(&entropy, "dot").unwrap();
	let root = derive_root_keypair_from_entropy(&entropy).unwrap();
	let peopl_subtree =
		derive_hard_path_from_keypair(root, &[PRODUCT_JUNCTION, "peopl.dot"]).unwrap();
	let expected = peopl_subtree.derived_key_simple(ChainCode(index_bytes(0)), []).0;
	assert_eq!(identity.public, expected.public);
	assert_eq!(
		product_public_key_to_address(identity.public.to_bytes()),
		"5FhRx3HstiFQ8vx3fJhUHQpTJoEwG8RG6XSv48haMuzxrUGT"
	);

	let message = b"RFC-0028 identity signing vector";
	let signature = identity.secret.sign_simple(SR25519_SIGNING_CONTEXT, message, &identity.public);
	assert!(
		identity
			.public
			.verify_simple(SR25519_SIGNING_CONTEXT, message, &signature)
			.is_ok()
	);
}

#[test]
fn identity_account_follows_the_network_suffix() {
	// `//product//peopl.<suffix>/index_bytes(0)`, one ordinary product
	// account per network: the personhood product's account 0, which owns
	// the person's usernames and is the wallet's main account.
	let entropy = [0xAB; 16];
	for (suffix, address) in [
		("paseo", "5CM1iRJQs1fceofARxxpvmUbaS7kU3WtKPj5QS6ihwWWtYBG"),
		("testnet", "5DWk4cZyixLSTtudEUVjKYmJb5cjAjj5Vq7QpQSGaurN3FNf"),
	] {
		let identity = derive_identity_keypair(&entropy, suffix).unwrap();
		assert_eq!(
			product_public_key_to_address(identity.public.to_bytes()),
			address,
			"identity account under peopl.{suffix}"
		);
		let subtree = derive_product_subtree_keypair(
			&derive_root_keypair_from_entropy(&entropy).unwrap(),
			&personhood_product_id(suffix),
		)
		.unwrap();
		assert_eq!(
			derive_product_public_key(subtree.public.to_bytes(), index_bytes(0)).unwrap(),
			identity.public.to_bytes(),
			"the identity account is the product account at index 0"
		);
	}
}

#[test]
fn raw_index_space_is_disjoint_from_plain_indexes() {
	// A raw all-zero index must not collide with plain index 0: the magic
	// keeps the two spaces separate.
	let subtree =
		derive_product_subtree_keypair(&fixture_root(), &ProductId::for_test("myapp.dot")).unwrap();
	let indexed = derive_product_public_key(subtree.public.to_bytes(), index_bytes(0)).unwrap();
	let raw = derive_product_public_key(subtree.public.to_bytes(), [0u8; 32]).unwrap();
	assert_ne!(indexed, raw);
}

#[test]
fn root_keypair_from_entropy_regression_pin() {
	// Regression pin for the entropy -> mini-secret -> sr25519 root path
	// (substrate-bip39 + schnorrkel Ed25519 expansion). This guards
	// against an accidental change to that path (dep bump, expansion mode)
	// that the pub-vs-secret self-consistency test cannot catch, since it
	// derives both sides from the same root.
	//
	// NOTE: this is a self-computed regression value, NOT yet cross-checked
	// against a polkadot-app-ios-v2 `deriveAccount` vector. Replace with an
	// iOS-sourced value once available to make it a true interop anchor.
	let root = derive_root_keypair_from_entropy(&[0xAB; 16]).unwrap();
	assert_eq!(
		hex::encode(root.public.to_bytes()),
		"0062ba8ae929ea64bc2ad6f21359e96a29e236a41d376d1c5ba76491da94fc72",
	);
}

#[test]
fn product_secret_signs_verifiably() {
	let root = derive_root_keypair_from_entropy(&[0xABu8; 16]).unwrap();
	let keypair =
		derive_product_keypair(&root, &ProductId::for_test("myapp.dot"), index_bytes(0)).unwrap();
	let message = b"<Bytes>hello</Bytes>";
	let signature = keypair.secret.sign_simple(b"substrate", message, &keypair.public);
	assert!(keypair.public.verify_simple(b"substrate", message, &signature).is_ok());
}

#[test]
fn chain_code_matches_dotli_encoding_rules() {
	let product = create_chain_code("product").unwrap();
	assert_eq!(&product[..8], &[0x1c, b'p', b'r', b'o', b'd', b'u', b'c', b't']);

	let zero = create_chain_code("0").unwrap();
	assert_eq!(&zero[..8], &[0; 8]);

	let long = create_chain_code(
		"w-credentialless-staticblitz-com.local-credentialless.webcontainer-api.io",
	)
	.unwrap();
	assert_ne!(&long[..8], &[0; 8]);
}
