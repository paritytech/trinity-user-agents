use super::*;
use crate::{
	Spawner,
	platform::{HostInfo, ProductContext},
	runtime::{
		authority::{AccountCaller, AccountInvocation},
		signing_host::ring_vrf::ChainRingResolver,
	},
	test_support::{StubPlatform, ring_location_fixture, test_stack},
};
use truapi::{
	CallContext,
	latest::{DerivationIndex, HostAccountRingVrfSignRequest},
};

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
		test_stack(network_suffix, [0; 32], [0; 32], [0xcc; 32]),
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

/// A registry entry whose key this wallet does not derive is refused, so the
/// wallet never signs under a public key it does not hold.
#[test]
fn direct_signing_rejects_registry_public_key_mismatched_with_wallet() {
	let wallet = wallet("dot");
	let session = wallet.current_session().unwrap();
	let product = ProductContext::new(ProductId::for_test("myapp.dot"));
	let handle = ProductAccountId {
		dot_ns_identifier: product.product_id.clone(),
		derivation_index: DerivationIndex::Index(8),
	};
	futures::executor::block_on(wallet.ring_vrf_registry.register(
		session.public_key,
		handle.clone(),
		ring_location_fixture(),
		[0xFF; 32],
	))
	.expect("synthetic registry entry persists");

	let error = futures::executor::block_on(wallet.ring_vrf_sign(
		AccountInvocation {
			call: &CallContext::default(),
			session: &session,
			caller: AccountCaller::Local { product: &product },
		},
		HostAccountRingVrfSignRequest {
			key_handle: handle,
			message: b"reject mismatched registry state".to_vec(),
		},
	))
	.unwrap_err();

	assert!(matches!(
		error,
		RingVrfError::Unknown { reason } if reason.contains("does not match the active wallet")
	));
}
