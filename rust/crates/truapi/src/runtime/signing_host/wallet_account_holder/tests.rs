use super::*;
use crate::{
	platform::HostInfo,
	runtime::signing_host::ring_vrf::ChainRingResolver,
	test_support::{StubPlatform, test_spawner},
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
		[0; 32],
		[0; 32],
		[0; 32],
		test_spawner(),
	);
	let registry = RingVrfRegistryStore::new(services.platform.clone());
	let consent = Arc::new(ProductConsent::new(services.platform.clone()));
	let ring_resolver = ChainRingResolver::new(services.chain.clone());
	let wallet = WalletAccountHolder::new(
		services,
		network_suffix.to_string(),
		consent,
		ring_resolver,
		registry,
	);
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

#[test]
fn identity_entropy_rejects_replaced_and_reinstalled_activations() {
	let wallet = wallet("paseo");
	let (original, original_account) = wallet.identity_activation().unwrap();
	assert_eq!(wallet.identity_entropy(original).unwrap().as_slice(), &[7; 32]);

	wallet.install(wallet.prepare_activation(vec![8; 32], None).unwrap());
	let (replacement, replacement_account) = wallet.identity_activation().unwrap();
	assert_ne!(original_account, replacement_account);
	assert!(matches!(wallet.identity_entropy(original), Err(AuthorityError::Disconnected)));
	assert_eq!(wallet.identity_entropy(replacement).unwrap().as_slice(), &[8; 32]);

	wallet.install(wallet.prepare_activation(vec![7; 32], None).unwrap());
	assert_eq!(wallet.identity_activation().unwrap().1, original_account);
	assert!(matches!(wallet.identity_entropy(original), Err(AuthorityError::Disconnected)));
	wallet.clear();
	assert!(matches!(wallet.identity_activation(), Err(AuthorityError::Disconnected)));
	assert!(matches!(wallet.identity_entropy(replacement), Err(AuthorityError::Disconnected)));
}

#[test]
fn identity_metadata_commit_rejects_reinstalled_and_disconnected_activations() {
	let wallet = wallet("paseo");
	let (original, _) = wallet.identity_activation().unwrap();
	wallet
		.update_identity(original, (Some("original.01".to_string()), None))
		.unwrap();
	wallet.install(wallet.prepare_activation(vec![8; 32], None).unwrap());
	let (current, _) = wallet
		.install(wallet.prepare_activation(vec![7; 32], Some("current.02".to_string())).unwrap());
	assert!(matches!(
		wallet.update_identity(original, (Some("stale.03".to_string()), None)),
		Err(AuthorityError::Disconnected),
	));
	assert_eq!(
		wallet.session_state().current().unwrap().lite_username.as_deref(),
		Some("current.02"),
	);
	wallet
		.update_identity(current, (Some("verified.04".to_string()), None))
		.unwrap();
	assert_eq!(
		wallet.session_state().current().unwrap().lite_username.as_deref(),
		Some("verified.04"),
	);
	wallet.clear();
	assert!(matches!(
		wallet.update_identity(current, (Some("stale.05".to_string()), None)),
		Err(AuthorityError::Disconnected),
	));
	assert!(wallet.session_state().current().is_none());
}
