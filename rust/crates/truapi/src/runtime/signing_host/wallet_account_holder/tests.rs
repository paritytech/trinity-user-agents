use super::{allowance::AllowanceAllocationError, *};
use crate::{
	platform::HostInfo,
	runtime::{
		signing_host::ring_vrf::ChainRingResolver,
		statement_allowance::{
			self as allocation,
			rpc::{RpcClient, testing::ScriptedRpc},
		},
	},
	test_support::{StubPlatform, test_spawner},
};
use futures::FutureExt;
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

/// A lite-person ring holding only `signer`'s member key.
fn lite_ring(signer: &WalletPersonhoodSigner<'_>) -> RingParams {
	RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![signer.member(PersonhoodCollection::LitePeople).unwrap()],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	}
}

fn methods(scripted: &ScriptedRpc) -> Vec<String> {
	scripted.calls().into_iter().map(|(method, _)| method).collect()
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

/// A wallet replaced while a registration reads the chain cannot prove for
/// the new wallet, so nothing is submitted.
#[test]
fn wallet_replacement_during_revision_read_prevents_allowance_submission() {
	use crate::runtime::statement_allowance::{CollectionScan, PooledRegistrationParams, slot};
	use parity_scale_codec::Encode;

	let wallet = wallet("paseo");
	let session = wallet.current_session().unwrap();
	let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
	let ring = lite_ring(&signer);
	let verified = format!(r#""0x{}""#, hex::encode(([0x22u8; 32], 0u32, 1_000u64).encode()));
	let scripted = ScriptedRpc::new(["null", verified.as_str()]);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let release = scripted.pause_response(0);
	let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(scripted.clone()));
	let extrinsics = allocation::people_test_extrinsics();
	let scans =
		[CollectionScan { collection: ring.collection, selection: slot::SlotSelection::Free(0) }];
	let memberships = [ring];
	let registration = allocation::register_statement_account_pooled(
		&rpc,
		allocation::people_test_metadata(),
		allocation::people_test_target(&extrinsics),
		&signer,
		&scans,
		&memberships,
		PooledRegistrationParams {
			target: &[0x22; 32],
			period: 7,
			network_suffix: b"paseo",
			reuse_existing: true,
			allow_eviction: false,
			protected: &[],
		},
	);
	futures::pin_mut!(registration);
	assert!(registration.as_mut().now_or_never().is_none());
	wallet.install(wallet.prepare_activation(vec![7; 32], None).unwrap());
	release.send(()).unwrap();
	let result = futures::executor::block_on(registration)
		.map(|_| ())
		.map_err(|error| AuthorityError::from(AllowanceAllocationError::from(error)));
	assert_eq!(
		(result, methods(&scripted)),
		(Err(AuthorityError::Disconnected), vec!["state_getStorage".to_string()])
	);
}

/// A wallet locked while a PGAS scan reads its first batch of slots cannot
/// derive the aliases for the next batch.
#[test]
fn wallet_lock_after_a_full_pgas_batch_prevents_the_next_batch() {
	use crate::runtime::statement_allowance::slot;

	let wallet = wallet("paseo");
	let session = wallet.current_session().unwrap();
	let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
	let responses: Vec<_> = std::iter::repeat_n(r#""0x""#, 10)
		.chain(std::iter::repeat_n("null", 10))
		.collect();
	let scripted = ScriptedRpc::new(responses);
	let release = scripted.pause_response(0);
	let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(scripted.clone()));
	let scan = slot::scan_pgas_slot_excluding(
		&rpc,
		allocation::asset_hub_test_metadata(),
		PersonhoodCollection::LitePeople,
		&signer,
		b"paseo",
		7,
		&[],
	);
	futures::pin_mut!(scan);
	assert!(scan.as_mut().now_or_never().is_none());
	wallet.clear();
	release.send(()).unwrap();
	let result = futures::executor::block_on(scan)
		.map_err(|error| AuthorityError::from(AllowanceAllocationError::from(error)));
	assert_eq!(
		(result, methods(&scripted)),
		(Err(AuthorityError::Disconnected), vec!["state_queryStorageAt".to_string()])
	);
}

/// A renewal pass keeps what it registered before the wallet was replaced and
/// submits nothing more for the replaced wallet.
#[test]
fn wallet_replacement_after_the_first_renewal_submission_stops_the_pass() {
	use crate::runtime::statement_allowance::{
		renewal::{
			RenewalChainContext, ResolvedRenewalTarget, StatementRenewalOutcome,
			TargetRenewalStatus, renew_targets,
		},
		slot::StatementStoreAllowanceEntry,
	};
	use parity_scale_codec::Encode;

	fn entry(account_id: [u8; 32], seq: u32) -> String {
		let entry = StatementStoreAllowanceEntry { account_id, seq, since: 1_000 }.encode();
		format!(r#""0x{}""#, hex::encode(entry))
	}

	let wallet = wallet("paseo");
	let session = wallet.current_session().unwrap();
	let signer = futures::executor::block_on(wallet.personhood_signer(&session)).unwrap();
	let rings = [lite_ring(&signer)];
	let targets = [
		ResolvedRenewalTarget { label: "first".to_string(), account_id: [0x21; 32] },
		ResolvedRenewalTarget { label: "second".to_string(), account_id: [0x22; 32] },
	];
	let first = entry(targets[0].account_id, 0);
	let second = entry(targets[1].account_id, 1);
	let mut responses = vec!["null"; 31];
	responses.extend([first.as_str(), first.as_str()]);
	responses.extend(std::iter::repeat_n("null", 10));
	responses.push(second.as_str());
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	scripted.script_subscription([r#"{"inBlock":"0xb10d"}"#]);
	let release = scripted.pause_response(5);
	let rpc = RpcClient::new(subxt_rpcs::RpcClient::new(scripted.clone()));
	let extrinsics = allocation::people_test_extrinsics();
	let context = RenewalChainContext {
		rpc: &rpc,
		metadata: allocation::people_test_metadata(),
		target_chain: allocation::people_test_target(&extrinsics),
		network_suffix: b"paseo",
		collections: &[PersonhoodCollection::LitePeople],
		signer: &signer,
		memberships: &rings,
	};
	let lock = futures::lock::Mutex::new(());
	// Building the extrinsic waits on the extrinsic service's node, so the pass
	// runs on its own thread until it reaches the read that follows the first
	// submission.
	let report = std::thread::scope(|scope| {
		let renewal = scope
			.spawn(|| futures::executor::block_on(renew_targets(&context, 7, &targets, &lock)));
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
		while scripted.calls().len() < 6 {
			assert!(std::time::Instant::now() < deadline, "the pass never submitted");
			std::thread::sleep(std::time::Duration::from_millis(1));
		}
		wallet.install(wallet.prepare_activation(vec![8; 32], None).unwrap());
		release.send(()).unwrap();
		renewal.join().unwrap()
	});
	assert_eq!(
		(report.outcomes, methods(&scripted)),
		(
			vec![
				StatementRenewalOutcome {
					label: "first".to_string(),
					status: TargetRenewalStatus::Registered {
						seq: 0,
						block_hash: "0xb10c".to_string(),
					},
				},
				StatementRenewalOutcome {
					label: "second".to_string(),
					status: TargetRenewalStatus::Failed { reason: "Disconnected".to_string() },
				},
			],
			[
				"state_queryStorageAt",
				"state_queryStorageAt",
				"state_queryStorageAt",
				"state_getStorage",
				"author_submitAndWatchExtrinsic",
				"state_getStorage"
			]
			.map(str::to_string)
			.to_vec()
		)
	);
}
