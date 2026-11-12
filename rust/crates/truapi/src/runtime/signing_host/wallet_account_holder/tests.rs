use super::*;
use crate::{
	Spawner,
	platform::HostInfo,
	runtime::signing_host::ring_vrf::ChainRingResolver,
	test_support::{StubPlatform, test_stack},
};
use truapi::latest::DerivationIndex;

/// An unlocked wallet over `platform`, activated from `entropy` repeated.
pub fn unlocked_wallet(
	platform: StubPlatform,
	network_suffix: &str,
	entropy: u8,
) -> WalletAccountHolder {
	let services = RuntimeServices::new(
		Arc::new(platform),
		HostInfo {
			name: "wallet test".to_string(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		test_stack(network_suffix, [0; 32], [0; 32], None),
		Spawner::for_test(),
	);
	let registry = RingVrfRegistryStore::new(services.platform.clone());
	let consent = Arc::new(ProductConsent::new(services.platform.clone()));
	let ring_resolver = ChainRingResolver::new(services.chain.clone());
	let wallet = WalletAccountHolder::new(services, consent, ring_resolver, registry);
	wallet.install(wallet.prepare_activation(vec![entropy; 32], None).unwrap());
	wallet
}

fn wallet(network_suffix: &str) -> WalletAccountHolder {
	unlocked_wallet(StubPlatform::default(), network_suffix, 7)
}

/// Wallet-internal allowance proofs use the reserved `peopl.<suffix>` keys the
/// mobile hosts derive on the same network, full person first so a full person
/// spends its wider slot budget first, and the session's identity is the
/// network's `peopl.<suffix>` account 0.
#[test]
fn reserved_identities_follow_the_network_and_collection_order() {
	for suffix in ["dot", "paseo"] {
		let wallet = wallet(suffix);
		let session = wallet.current_session().unwrap();
		let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
		let members = PersonhoodCollection::ALL
			.map(|collection| (collection, signer.member(collection).unwrap()));
		let expected = [(PersonhoodCollection::People, 0), (PersonhoodCollection::LitePeople, 1)]
			.map(|(collection, index)| {
				let entropy = derive_ring_vrf_entropy(
					&[7; 32],
					&ProductId::for_test(&format!("peopl.{suffix}")),
					&DerivationIndex::Index(index),
				)
				.unwrap();
				(collection, signer.vrf.member(&entropy).unwrap())
			});
		assert_eq!(members, expected);
		let identity = derive_identity_keypair(&[7; 32], suffix).unwrap();
		assert_eq!(session.identity_account_id, Some(identity.public.to_bytes()));
	}
}

/// Every product key the wallet derives passes one refusal, so a name that
/// only parses gets none, whichever caller or request named it.
#[test]
fn no_key_is_derived_for_a_product_the_stack_does_not_serve() {
	let keys = WalletKeys::new(vec![7; 32], "paseo".to_string());
	let derives = |product: &str| {
		let product_id = ProductId::for_test(product);
		let account = ProductAccountId {
			dot_ns_identifier: product_id.clone(),
			derivation_index: DerivationIndex::Index(0),
		};
		[
			keys.product_subtree_keypair(&product_id).is_ok(),
			keys.product_keypair(&account).is_ok(),
			keys.ring_vrf_entropy(&account).is_ok(),
			keys.ring_vrf_domain_entropy(&product_id).is_ok(),
		]
	};

	assert_eq!(derives("myapp.paseo"), [true; 4]);
	assert_eq!(derives("myapp.dot"), [true; 4]);
	assert_eq!(derives("myapp.testnet"), [false; 4]);
	assert_eq!(derives("example.com"), [false; 4]);
}
