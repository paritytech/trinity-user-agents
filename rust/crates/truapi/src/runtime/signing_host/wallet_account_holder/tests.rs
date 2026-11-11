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
	let consent =
		Arc::new(ProductConsent::new(services.platform.clone(), services.permissions.clone()));
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
/// network's `uid.<suffix>` account.
#[test]
fn reserved_identities_follow_the_network_and_collection_order() {
	for suffix in ["dot", "paseo"] {
		let wallet = wallet(suffix);
		let session = wallet.current_session().unwrap();
		let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
		let members = PersonhoodCollection::ALL
			.map(|collection| (collection, signer.member(collection, 0).unwrap()));
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
fn migrated_lite_candidates_remain_primary_first_and_session_bound() {
	futures::executor::block_on(async {
		let wallet = wallet("paseo");
		wallet.install(wallet.prepare_activation(vec![7; 32], Some("alice.paseo".into())).unwrap());
		let session = wallet.current_session().unwrap();
		let signer = wallet.personhood_signer(&session).await.unwrap();
		assert_eq!(signer.key_count(PersonhoodCollection::People), 1);
		assert_eq!(signer.key_count(PersonhoodCollection::LitePeople), 2);
		assert_eq!(
			signer.member(PersonhoodCollection::LitePeople, 0).unwrap(),
			signer
				.vrf
				.member(&derive_lite_person_ring_vrf_entropy(&[7; 32], "paseo"))
				.unwrap()
		);
		assert_eq!(
			signer.member(PersonhoodCollection::LitePeople, 1).unwrap(),
			signer
				.vrf
				.member(&derive_lite_person_ring_vrf_entropy(&[7; 32], "dot"))
				.unwrap()
		);
		assert_ne!(
			signer.alias(PersonhoodCollection::LitePeople, 0, b"context").unwrap(),
			signer.alias(PersonhoodCollection::LitePeople, 1, b"context").unwrap()
		);
		wallet.clear();
		assert!(signer.member(PersonhoodCollection::LitePeople, 1).is_err());
	});
}

#[test]
fn legacy_lite_import_is_reserved_idempotent_and_root_scoped() {
	futures::executor::block_on(async {
		let wallet = wallet("paseo");
		wallet.install(wallet.prepare_activation(vec![7; 32], Some("alice.paseo".into())).unwrap());
		let owner = ProductId::for_test("peopl.paseo");
		let handle = ProductAccountId {
			dot_ns_identifier: owner.clone(),
			derivation_index: DerivationIndex::Index(1),
		};
		assert!(
			wallet
				.ensure_legacy_lite_person_provider([1; 32], &ProductId::for_test("other.paseo"))
				.await
				.is_err()
		);
		wallet.ensure_legacy_lite_person_provider([1; 32], &owner).await.unwrap();
		wallet.ensure_legacy_lite_person_provider([1; 32], &owner).await.unwrap();
		wallet.ensure_legacy_lite_person_provider([2; 32], &owner).await.unwrap();
		let session = wallet.current_session().unwrap();
		let entry = wallet
			.ring_vrf_registry
			.entry(session.public_key, &handle)
			.await
			.unwrap()
			.unwrap();
		assert_eq!(entry.rings.len(), 2);
		let vrf = vrf::load().await.unwrap();
		assert_eq!(
			entry.public_key,
			Some(vrf.member(&derive_lite_person_ring_vrf_entropy(&[7; 32], "dot")).unwrap())
		);
		wallet.install(wallet.prepare_activation(vec![8; 32], None).unwrap());
		let other = wallet.current_session().unwrap();
		assert!(
			wallet
				.ring_vrf_registry
				.entry(other.public_key, &handle)
				.await
				.unwrap()
				.is_none()
		);
		assert_eq!(
			wallet
				.personhood_signer(&other)
				.await
				.unwrap()
				.key_count(PersonhoodCollection::LitePeople),
			1
		);
	});
}
