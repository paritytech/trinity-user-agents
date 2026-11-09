use subxt_rpcs::RpcClient as HostRpcClient;

use super::{rpc::testing::ScriptedRpc, slot::testing::lite_people, *};

/// Fixture metadata captured from paseo-next-v2 (raw `RuntimeMetadataPrefixed`).
const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");

fn allowance(
	remained_size: u64,
	remained_transactions: u32,
	expires_in: u32,
) -> BulletinAllowanceInfo {
	BulletinAllowanceInfo { remained_size, remained_transactions, expires_in, fetched_at: 10 }
}

#[test]
fn bulletin_authorization_covers_the_whole_submission() {
	let results = [
		allowance(128, 1, 11),
		allowance(127, 1, 11),
		allowance(128, 0, 11),
		allowance(128, 1, 10),
	]
	.map(|info| info.can_store(128));
	assert_eq!(results, [true, false, false, false]);
}

#[test]
fn bulletin_refresh_accepts_available_state_when_baseline_was_unusable() {
	let exhausted_by_size = allowance(0, 4, 100);
	let refreshed_same_transactions = allowance(4096, 4, 100);

	assert!(!exhausted_by_size.available());
	assert!(authorization_refreshed(
		refreshed_same_transactions,
		Some(exhausted_by_size).filter(|info| info.available()),
	));
}

#[test]
fn bulletin_refresh_accepts_size_only_increase() {
	let baseline = allowance(128, 4, 100);
	let refreshed = allowance(4096, 4, 100);

	assert!(authorization_refreshed(refreshed, Some(baseline)));
}

#[test]
fn bulletin_refresh_rejects_unchanged_available_state() {
	let baseline = allowance(128, 4, 100);

	assert!(!authorization_refreshed(baseline, Some(baseline)));
}

/// Wait for an extension of a 16-byte authorization that the chain shows
/// holding `remaining` bytes. The script answers exactly one read.
fn wait_for_extension(
	remaining: u64,
	size: u64,
) -> Result<BulletinAllowanceInfo, StatementAllowanceError> {
	let entry = BulletinAuthorization {
		transactions: 0,
		transactions_allowance: 1,
		bytes: 0,
		bytes_permanent: 0,
		bytes_allowance: remaining,
		expiration: 100,
	}
	.encode();
	let storage = format!(r#""0x{}""#, hex::encode(entry));
	let scripted = ScriptedRpc::new([storage.as_str(), r#"{"number":"0x1"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));
	futures::executor::block_on(wait_bulletin_authorization(
		&rpc,
		&[0x22; 32],
		Some(allowance(16, 1, 100)),
		size,
		Duration::from_secs(60),
	))
}

#[test]
fn an_extension_that_holds_the_payload_is_returned() {
	assert_eq!(
		wait_for_extension(256, 128).ok(),
		Some(BulletinAllowanceInfo {
			remained_size: 256,
			remained_transactions: 1,
			expires_in: 100,
			fetched_at: 1,
		}),
	);
}

/// One claim may add fewer bytes than a large payload needs. Polling for
/// more would only run out the submit deadline.
#[test]
fn an_extension_too_small_for_the_payload_fails_at_once() {
	let result = wait_for_extension(64, 128);

	assert!(
		matches!(
			result,
			Err(StatementAllowanceError::BulletinAuthorizationTooSmall {
				size: 128,
				remained_size: 64,
			})
		),
		"{result:?}"
	);
}

#[test]
fn banned_submissions_are_not_classified_as_duplicates() {
	let classified: Vec<bool> = [
		"Priority is too low: (100 vs 100)",
		"Transaction Already Imported",
		"Transaction is temporarily banned",
		"Invalid Transaction",
	]
	.into_iter()
	.map(duplicate_submit_error)
	.collect();

	assert_eq!(classified, vec![true, true, false, false]);
}

#[test]
fn bulletin_account_scope_matches_runtime_enum_codec() {
	let scope = BulletinAuthorizationScope::Account([0x42; 32]);
	let encoded = scope.encode();

	assert_eq!(encoded, [vec![0x00], vec![0x42; 32]].concat());
	assert_eq!(BulletinAuthorizationScope::decode(&mut encoded.as_slice()).unwrap(), scope);
}

/// A `state_getRuntimeVersion` result for `spec_version`.
fn runtime_version(spec_version: u32) -> String {
	format!(r#"{{"specVersion":{spec_version},"transactionVersion":1}}"#)
}

/// The fixture metadata as a `state_getMetadata` hex result.
fn metadata_result() -> String {
	format!(r#""0x{}""#, hex::encode(FIXTURE))
}

/// `Metadata_metadata_at_version(16)` answering `None`, so the caller falls
/// back to `state_getMetadata`. These tests are about caching, not about
/// which metadata version a runtime serves.
fn metadata_version_unavailable() -> String {
	r#""0x00""#.to_string()
}

/// A `chain_getBlockHash(0)` result for `genesis_hash`.
fn genesis_result(genesis_hash: [u8; 32]) -> String {
	format!(r#""0x{}""#, hex::encode(genesis_hash))
}

/// Method names the scripted transport saw, in order.
fn methods(scripted: &ScriptedRpc) -> Vec<String> {
	scripted.calls().into_iter().map(|(method, _)| method).collect()
}

/// The requests one cache miss makes, in order. The two validation reads
/// are issued together, so both happen whether or not the entry is reused.
const MISS: [&str; 4] = [
	"state_getRuntimeVersion",
	"chain_getBlockHash",
	// The V16 runtime call is tried first; these scripts answer it as absent,
	// so the legacy fetch follows.
	"state_call",
	"state_getMetadata",
];
/// The requests one cache hit makes: validation only, no metadata download.
const HIT: [&str; 2] = ["state_getRuntimeVersion", "chain_getBlockHash"];

/// One call's worth of scripted responses: the two validation reads plus,
/// when the entry has to be built, the metadata download.
fn call_script(spec_version: u32, reported: [u8; 32], downloads: bool) -> Vec<String> {
	let mut script = vec![runtime_version(spec_version), genesis_result(reported)];
	if downloads {
		script.push(metadata_version_unavailable());
		script.push(metadata_result());
	}
	script
}

/// Drive `ChainContextCache::get` over `responses`, fetching each entry in
/// `chains` in turn, and report the methods the transport saw.
fn scripted_cache_run(responses: &[String], chains: &[[u8; 32]]) -> Vec<String> {
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str));
	let cache = ChainContextCache::default();

	futures::executor::block_on(async {
		for genesis_hash in chains {
			let client = ChainClient::new(
				RpcClient::new(HostRpcClient::new(scripted.clone())),
				*genesis_hash,
			);
			cache.get(&client).await.expect("scripted chain context fetch succeeds");
		}
	});
	methods(&scripted)
}

/// Transport that answers by method rather than in order, and yields inside
/// every request so concurrent cache misses genuinely interleave. Counts the
/// metadata downloads, which is the cost the cache exists to avoid.
#[derive(Clone, Default)]
struct CountingRpc(std::sync::Arc<CountingState>);

#[derive(Default)]
struct CountingState {
	metadata_downloads: std::sync::atomic::AtomicUsize,
}

impl CountingRpc {
	fn metadata_downloads(&self) -> usize {
		self.0.metadata_downloads.load(std::sync::atomic::Ordering::Relaxed)
	}
}

impl subxt_rpcs::client::RpcClientT for CountingRpc {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		_params: Option<Box<subxt_rpcs::client::RawValue>>,
	) -> subxt_rpcs::client::RawRpcFuture<'a, Box<subxt_rpcs::client::RawValue>> {
		let body = match method {
			"state_getRuntimeVersion" => runtime_version(1_000_000),
			"chain_getBlockHash" => genesis_result([0xaa; 32]),
			"state_call" => metadata_version_unavailable(),
			"state_getMetadata" => {
				self.0.metadata_downloads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
				metadata_result()
			},
			other => panic!("unexpected request `{other}`"),
		};
		Box::pin(async move {
			// Yield so a concurrently polled caller reaches the same point.
			let mut yielded = false;
			futures::future::poll_fn(move |cx| {
				if yielded {
					core::task::Poll::Ready(())
				} else {
					yielded = true;
					cx.waker().wake_by_ref();
					core::task::Poll::Pending
				}
			})
			.await;
			Ok(subxt_rpcs::client::RawValue::from_string(body)
				.expect("scripted response is valid JSON"))
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		_sub: &'a str,
		_params: Option<Box<subxt_rpcs::client::RawValue>>,
		_unsub: &'a str,
	) -> subxt_rpcs::client::RawRpcFuture<'a, subxt_rpcs::client::RawRpcSubscription> {
		unreachable!("the chain context cache does not subscribe")
	}
}

#[test]
fn concurrent_misses_download_the_metadata_once() {
	let counting = CountingRpc::default();
	let client = ChainClient::new(RpcClient::new(HostRpcClient::new(counting.clone())), [0xaa; 32]);
	let cache = ChainContextCache::default();

	futures::executor::block_on(async {
		let (first, second) = futures::join!(cache.get(&client), cache.get(&client));
		let first = first.expect("first read");
		let second = second.expect("second read");
		// Both callers end up on the same entry.
		assert!(Arc::ptr_eq(&first.metadata, &second.metadata));
	});

	assert_eq!(counting.metadata_downloads(), 1);
}

#[test]
fn a_chain_is_read_once_per_spec_version() {
	let seen = scripted_cache_run(
		&[call_script(1_000_000, [0xaa; 32], true), call_script(1_000_000, [0xaa; 32], false)]
			.concat(),
		&[[0xaa; 32], [0xaa; 32]],
	);

	assert_eq!(seen, [MISS.as_slice(), HIT.as_slice()].concat());
}

#[test]
fn a_wipe_that_keeps_the_spec_version_refreshes_the_entry() {
	// A chain redeployed from the same runtime keeps its spec version and
	// gets a new genesis. Validating on the spec version alone would reuse
	// the entry and sign `CheckGenesis` for the chain that no longer exists.
	let seen = scripted_cache_run(
		&[call_script(1_000_000, [0xaa; 32], true), call_script(1_000_000, [0xcc; 32], true)]
			.concat(),
		&[[0xaa; 32], [0xaa; 32]],
	);

	assert_eq!(seen, [MISS, MISS].concat());
}

#[test]
fn a_spec_version_bump_refreshes_the_entry() {
	let seen = scripted_cache_run(
		&[call_script(1_000_000, [0xaa; 32], true), call_script(1_000_001, [0xaa; 32], true)]
			.concat(),
		&[[0xaa; 32], [0xaa; 32]],
	);

	assert_eq!(seen, [MISS, MISS].concat());
}

#[test]
fn chains_do_not_share_a_cache_entry() {
	let seen = scripted_cache_run(
		&[call_script(1_000_000, [0xaa; 32], true), call_script(1_000_000, [0xbb; 32], true)]
			.concat(),
		&[[0xaa; 32], [0xbb; 32]],
	);

	assert_eq!(seen, [MISS, MISS].concat());
}

#[test]
fn a_stale_configured_genesis_still_keys_the_cache() {
	let seen = scripted_cache_run(
		&[call_script(1_000_000, [0xbb; 32], true), call_script(1_000_000, [0xbb; 32], false)]
			.concat(),
		&[[0xaa; 32], [0xaa; 32]],
	);

	assert_eq!(seen, [MISS.as_slice(), HIT.as_slice()].concat());
}

/// `StmtStoreAllowanceEntry { account_id, seq: 0, since: 0 }` as a scripted
/// JSON storage result.
/// Fixed clock for the scripted registration tests.
const NOW: u64 = 10_000_000;

fn slot_entry(account: [u8; 32]) -> String {
	slot_entry_since(account, 0)
}

/// A scripted slot entry that was set at `since`.
fn slot_entry_since(account: [u8; 32], since: u64) -> String {
	let entry = slot::StatementStoreAllowanceEntry { account_id: account, seq: 0, since }.encode();
	format!(r#""0x{}""#, hex::encode(entry))
}

/// `Timestamp.Now` as a scripted storage result: unix seconds in millis.
fn chain_clock(seconds: u64) -> String {
	format!(r#""0x{}""#, hex::encode((seconds * 1_000).encode()))
}

/// Run `register_statement_account` against a scripted chain: all ten
/// slots free, the extrinsic reaches block `0xb10c`, and the verification
/// read at that block returns `verified_entry`.
fn scripted_registration(
	verified_entry: &str,
) -> (Result<RegistrationOutcome, StatementAllowanceError>, ScriptedRpc) {
	scripted_registration_with(verified_entry, None, 10)
}

/// Run `register_statement_account` against a scripted chain: `free_slots`
/// slots answered as free, the extrinsic reaching block `0xb10c`, and the
/// verification read at that block returning `verified_entry`.
fn scripted_registration_with(
	verified_entry: &str,
	preselected: Option<Preselected>,
	free_slots: usize,
) -> (Result<RegistrationOutcome, StatementAllowanceError>, ScriptedRpc) {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ring = RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	};

	// The ring-revision read comes first (absent => revision 0), then the
	// slot scan, then the post-submit verification read.
	let mut responses = vec!["null"; free_slots + 1];
	responses.push(verified_entry);
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let outcome = futures::executor::block_on(register_statement_account(
		&rpc,
		metadata,
		target_chain,
		&signer,
		RegistrationParams {
			target: &[0x22; 32],
			period: 7,
			network_suffix: b"paseo",
			ring: &ring,
			reuse_existing: true,
			preselected,
			protected: &[],
		},
	));
	(outcome, scripted)
}

/// Both collections, People first, with a distinct entropy each.
fn pooled_memberships() -> [RingParams; 2] {
	PersonhoodCollection::ALL.map(|collection| {
		let entropy = match collection {
			PersonhoodCollection::People => [0x31; 32],
			PersonhoodCollection::LitePeople => [0x11; 32],
		};
		RingParams {
			collection,
			members: vec![proof::member_key_now(entropy)],
			exponent: 9,
			ring_index: 0,
			block_hash: "0xfinal".to_string(),
		}
	})
}

/// One occupied slot entry.
fn occupied(account: [u8; 32], since: u64) -> String {
	let entry = slot::StatementStoreAllowanceEntry { account_id: account, seq: 0, since }.encode();
	format!(r#""0x{}""#, hex::encode(entry))
}

/// Candidates matching [`pooled_memberships`], for the scan pass.
fn pooled_candidates() -> [CollectionCandidate; 2] {
	PersonhoodCollection::ALL.map(|collection| CollectionCandidate {
		collection,
		entropy: match collection {
			PersonhoodCollection::People => [0x31; 32],
			PersonhoodCollection::LitePeople => [0x11; 32],
		},
	})
}

/// Scan both collections then register from that scan, over `responses`.
fn scripted_pooled(
	responses: Vec<String>,
	target: [u8; 32],
	protected: &[(PersonhoodCollection, u32)],
	allow_eviction: bool,
) -> (Result<RegistrationOutcome, StatementAllowanceError>, ScriptedRpc) {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let memberships = pooled_memberships();
	let candidates = pooled_candidates();
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let outcome = futures::executor::block_on(async {
		let scans = scan_collections(
			&rpc,
			metadata,
			&signer,
			&PersonhoodCollection::ALL,
			CollectionScanParams {
				network_suffix: b"paseo",
				period: 7,
				target: &target,
				reuse_existing: true,
			},
		)
		.await?;
		register_statement_account_pooled(
			&rpc,
			metadata,
			target_chain,
			&signer,
			&scans,
			&memberships,
			PooledRegistrationParams {
				target: &target,
				period: 7,
				network_suffix: b"paseo",
				reuse_existing: true,
				allow_eviction,
				protected,
			},
		)
		.await
	});
	(outcome, scripted)
}

/// Scan then register, with the submission failing with `submit_error`.
fn scripted_pooled_with_submit_error(
	responses: Vec<String>,
	target: [u8; 32],
	submit_error: &str,
) -> Result<RegistrationOutcome, StatementAllowanceError> {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let memberships = pooled_memberships();
	let candidates = pooled_candidates();
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
	scripted.script_subscription_errors(submit_error, 1);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	futures::executor::block_on(async {
		let scans = scan_collections(
			&rpc,
			metadata,
			&signer,
			&PersonhoodCollection::ALL,
			CollectionScanParams {
				network_suffix: b"paseo",
				period: 7,
				target: &target,
				reuse_existing: true,
			},
		)
		.await?;
		register_statement_account_pooled(
			&rpc,
			metadata,
			target_chain,
			&signer,
			&scans,
			&memberships,
			PooledRegistrationParams {
				target: &target,
				period: 7,
				network_suffix: b"paseo",
				reuse_existing: true,
				allow_eviction: true,
				protected: &[],
			},
		)
		.await
	})
}

/// Slot tables for both collections: `people` then `lite`, each entry either
/// free or occupied.
fn tables(people: Vec<Option<String>>, lite: Vec<Option<String>>) -> Vec<String> {
	assert_eq!(people.len(), 20, "People declares 20 slots in the fixture");
	assert_eq!(lite.len(), 10, "LitePeople declares 10 slots in the fixture");
	people
		.into_iter()
		.chain(lite)
		.map(|entry| entry.unwrap_or_else(|| "null".to_string()))
		.collect()
}

/// The capacity this change exists for: a full People table must not report
/// exhaustion while LitePeople still has a free seq.
#[test]
fn a_full_people_table_falls_back_to_a_free_lite_people_slot() {
	let target = [0x22; 32];
	let mut responses = tables(
		(0..20).map(|seq| Some(occupied([0x99; 32], 1_000 + seq))).collect(),
		(0..10).map(|_| None).collect(),
	);
	// Ring revision, then the post-submit verification read.
	responses.push("null".to_string());
	responses.push(occupied(target, 5_000));

	let (outcome, _) = scripted_pooled(responses, target, &[], true);

	let RegistrationOutcome::Registered { seq, collection, .. } =
		outcome.expect("a free LitePeople slot is registrable")
	else {
		panic!("expected a registration");
	};
	assert_eq!(collection, PersonhoodCollection::LitePeople);
	assert_eq!(seq, 0);
}

/// A free People slot wins over a free LitePeople one, so a full person
/// spends the wider budget first and keeps the light one as headroom.
#[test]
fn a_free_people_slot_is_preferred_to_a_free_lite_people_slot() {
	let target = [0x22; 32];
	let mut responses = tables((0..20).map(|_| None).collect(), (0..10).map(|_| None).collect());
	responses.push("null".to_string());
	responses.push(occupied(target, 5_000));

	let (outcome, _) = scripted_pooled(responses, target, &[], true);

	let RegistrationOutcome::Registered { collection, .. } =
		outcome.expect("a free People slot is registrable")
	else {
		panic!("expected a registration");
	};
	assert_eq!(collection, PersonhoodCollection::People);
}

/// Holding a slot in one collection must not earn a second one in another,
/// even though every People slot here is free.
#[test]
fn an_allocation_in_lite_people_blocks_a_second_one_in_people() {
	let target = [0x22; 32];
	let responses = tables(
		(0..20).map(|_| None).collect(),
		(0..10).map(|seq| (seq == 3).then(|| occupied(target, 4_000))).collect(),
	);

	let (outcome, scripted) = scripted_pooled(responses, target, &[], true);

	assert!(
		matches!(
			outcome,
			Ok(RegistrationOutcome::AlreadyAllocated {
				seq: 3,
				collection: PersonhoodCollection::LitePeople
			})
		),
		"expected the existing LitePeople slot to be reported: {outcome:?}"
	);
	assert!(
		scripted
			.calls()
			.iter()
			.all(|(method, _)| method != "author_submitAndWatchExtrinsic"),
		"nothing should have been submitted"
	);
}

/// With every collection full, the victim is the globally oldest replaceable
/// slot, not the oldest inside whichever collection was scanned first.
#[test]
fn eviction_takes_the_globally_oldest_slot_across_collections() {
	let target = [0x22; 32];
	// Every People slot is newer than every LitePeople slot.
	let mut responses = tables(
		(0..20).map(|seq| Some(occupied([0x99; 32], 9_000 + seq))).collect(),
		(0..10).map(|seq| Some(occupied([0x98; 32], 1_000 + seq))).collect(),
	);
	responses.push(chain_clock(10_000_000));
	responses.push("null".to_string());
	responses.push(occupied(target, 10_000_000));

	let (outcome, _) = scripted_pooled(responses, target, &[], true);

	let RegistrationOutcome::Registered { seq, collection, .. } =
		outcome.expect("an old LitePeople slot is replaceable")
	else {
		panic!("expected a registration");
	};
	assert_eq!(collection, PersonhoodCollection::LitePeople);
	assert_eq!(seq, 0, "seq 0 is the oldest LitePeople slot");
}

/// `protected` is per collection: the same `seq` in two collections is two
/// unrelated slots, so protecting one must leave the other evictable.
#[test]
fn protection_does_not_leak_across_collections() {
	let target = [0x22; 32];
	// People holds the oldest slots, so the victim should be People seq 0.
	// LitePeople seq 0 is protected, and that must not shield People seq 0.
	let mut responses = tables(
		(0..20).map(|seq| Some(occupied([0x99; 32], 1_000 + seq))).collect(),
		(0..10).map(|seq| Some(occupied([0x98; 32], 9_000 + seq))).collect(),
	);
	responses.push(chain_clock(10_000_000));
	responses.push("null".to_string());
	responses.push(occupied(target, 10_000_000));

	let (outcome, _) =
		scripted_pooled(responses, target, &[(PersonhoodCollection::LitePeople, 0)], true);

	let RegistrationOutcome::Registered { seq, collection, .. } =
		outcome.expect("People seq 0 is still replaceable")
	else {
		panic!("expected a registration");
	};
	assert_eq!(collection, PersonhoodCollection::People);
	assert_eq!(seq, 0, "protecting LitePeople seq 0 must not protect People seq 0");
}

/// Exhaustion has to name the pooled budget. Reporting one collection's share
/// is what made a full person look capped at ten.
#[test]
fn exhaustion_names_the_summed_budget() {
	let target = [0x22; 32];
	// Everything full and everything inside its cooldown, so nothing is
	// replaceable and the error is raised.
	let mut responses = tables(
		(0..20).map(|_| Some(occupied([0x99; 32], 9_999_999))).collect(),
		(0..10).map(|_| Some(occupied([0x98; 32], 9_999_999))).collect(),
	);
	responses.push(chain_clock(10_000_000));

	let (outcome, _) = scripted_pooled(responses, target, &[], true);

	let err = outcome.expect_err("a full pool cannot register");
	assert!(
		matches!(
			err,
			StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
				period: 7,
				max: 30
			})
		),
		"expected the pooled budget of 20 + 10: {err:?}"
	);
}

/// A takeover chosen by the pooled caller still spends this call's single
/// revocation, so a refused submission has to be named rather than surfacing
/// as a raw RPC string. The pooled path is the shipping path, so testing this
/// only through a direct call with no preselection misses it entirely.
#[test]
fn a_refused_takeover_is_named_on_the_pooled_path() {
	let target = [0x22; 32];
	// Everything full and old enough to replace, so the pool evicts.
	let mut responses = tables(
		(0..20).map(|seq| Some(occupied([0x99; 32], 1_000 + seq))).collect(),
		(0..10).map(|seq| Some(occupied([0x98; 32], 5_000 + seq))).collect(),
	);
	responses.push(chain_clock(10_000_000));
	responses.push("null".to_string());

	let err = scripted_pooled_with_submit_error(
		responses,
		target,
		"User error: Invalid Transaction (1010)",
	)
	.unwrap_err();

	assert!(
		err.to_string().contains("not replaceable yet"),
		"a refused pooled takeover should be named, got: {err}"
	);
}

/// One registration revokes at most one allowance. A duplicate-submit retry
/// after a pooled takeover must not evict a second slot, because the first
/// submission may still land.
#[test]
fn a_pooled_takeover_does_not_evict_a_second_slot_on_retry() {
	let target = [0x22; 32];
	let mut responses = tables(
		(0..20).map(|seq| Some(occupied([0x99; 32], 1_000 + seq))).collect(),
		(0..10).map(|seq| Some(occupied([0x98; 32], 5_000 + seq))).collect(),
	);
	responses.push(chain_clock(10_000_000));
	responses.push("null".to_string());
	// The retry rescans this collection and finds it still full.
	responses.extend((0..20).map(|seq| occupied([0x99; 32], 1_000 + seq)));

	// A duplicate-submit rejection drives the retry.
	let err =
		scripted_pooled_with_submit_error(responses, target, "Priority is too low").unwrap_err();

	// Exhaustion, not a second eviction, and it names the pooled budget.
	assert!(
		matches!(
			err,
			StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
				period: 7,
				max: 30
			})
		),
		"a retry after a takeover should give up rather than evict again: {err:?}"
	);
}

/// A real `Members.Collections[LitePeople]` value from the live People chain.
/// Hand-encoding one means encoding `CollectionOwner` and `RingMode` too, and
/// a wrong guess would make the test pass for the wrong reason.
const LIVE_LITE_COLLECTION: &str = r#""0x000001043e000900""#;

/// The regression this guards: a device that can only prove light personhood
/// must still get its allowance when the full-personhood storage is broken.
/// Resolving every collection in one fallible pass made any People-side
/// failure abort the whole allowance path, including for lite-only devices
/// that never needed People at all.
#[test]
fn a_broken_people_collection_does_not_discard_a_lite_people_membership() {
	let metadata = test_fixtures::people();
	let candidates = pooled_candidates();
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let lite_entropy = candidates[1].entropy;
	let page = format!(r#""0x04{}""#, hex::encode(proof::member_key_now(lite_entropy)));

	let responses = [
		// People: a finalized head, then an undecodable Collections value.
		r#""0xfinal""#.to_string(),
		r#""0x00""#.to_string(),
		// LitePeople: resolves normally.
		r#""0xfinal""#.to_string(),
		LIVE_LITE_COLLECTION.to_string(),
		r#""0x00000000""#.to_string(),
		page,
		"null".to_string(),
		"null".to_string(),
	];
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let memberships = futures::executor::block_on(find_including_rings(
		&rpc,
		metadata,
		&signer,
		&PersonhoodCollection::ALL,
		u32::MAX,
	))
	.expect("a broken People collection must not fail the whole resolution");

	assert_eq!(memberships.len(), 1, "LitePeople should still resolve");
	assert_eq!(memberships[0].collection, PersonhoodCollection::LitePeople);
	assert_eq!(memberships[0].members, vec![proof::member_key_now(lite_entropy)]);
}

/// Every candidate failing is an outage, not an answer. Reporting it as "no
/// membership" would let a caller conclude the person has no personhood.
#[test]
fn every_collection_failing_is_reported_as_an_error() {
	let metadata = test_fixtures::people();
	let candidates = pooled_candidates();
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();

	let responses = [
		r#""0xfinal""#.to_string(),
		r#""0x00""#.to_string(),
		r#""0xfinal""#.to_string(),
		r#""0x00""#.to_string(),
	];
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let err = futures::executor::block_on(find_including_rings(
		&rpc,
		metadata,
		&signer,
		&PersonhoodCollection::ALL,
		u32::MAX,
	))
	.expect_err("no collection resolved, so this is a failure not an empty answer");
	assert!(matches!(err, StatementAllowanceError::Ring(_)), "unexpected error: {err:?}");
}

/// Exhaustion raised by the inner rescan has to name the device's budget on
/// the free-slot path too. Summing only the collections that reported `Full`
/// makes that figure empty here, so the retry reported a budget of zero.
#[test]
fn a_retry_from_a_free_slot_still_names_the_summed_budget() {
	let target = [0x22; 32];
	// Both tables free, so the first choice is a free People slot.
	let mut responses = tables((0..20).map(|_| None).collect(), (0..10).map(|_| None).collect());
	// Ring revision, then the rescan the duplicate-submit retry performs:
	// People is now full and every slot is inside its cooldown.
	responses.push("null".to_string());
	responses.extend((0..20).map(|_| occupied([0x99; 32], 9_999_999)));
	responses.push(chain_clock(10_000_000));

	let err =
		scripted_pooled_with_submit_error(responses, target, "Priority is too low").unwrap_err();

	assert!(
		matches!(
			err,
			StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
				period: 7,
				max: 30
			})
		),
		"expected the pooled budget of 20 + 10, not one branch's share: {err:?}"
	);
}

/// Connecting a product must not revoke another product's allowance. A full
/// period is exhaustion, and reclaiming space is the renewal pass's job.
#[test]
fn on_demand_allocation_reports_exhaustion_rather_than_evicting() {
	let target = [0x22; 32];
	// Everything full and old enough that eviction would succeed if allowed.
	let responses = tables(
		(0..20).map(|seq| Some(occupied([0x99; 32], 1_000 + seq))).collect(),
		(0..10).map(|seq| Some(occupied([0x98; 32], 1_000 + seq))).collect(),
	);

	let (outcome, scripted) = scripted_pooled(responses, target, &[], false);

	let err = outcome.expect_err("a full pool cannot allocate without evicting");
	assert!(
		matches!(
			err,
			StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
				period: 7,
				max: 30
			})
		),
		"unexpected error: {err:?}"
	);
	assert!(
		scripted
			.calls()
			.iter()
			.all(|(method, _)| method != "author_submitAndWatchExtrinsic"),
		"nothing should have been submitted"
	);
}

/// The scan pass isolates per-collection failures too. It runs before the
/// membership resolution, so a failed People read here would fail the whole
/// allocation for a device that could never hold a slot in People.
#[test]
fn a_failed_people_scan_still_finds_the_lite_people_allocation() {
	let metadata = test_fixtures::people();
	let candidates = pooled_candidates();
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let target = [0x22; 32];

	let null = || "null".to_string();
	// People: an undecodable storage value fails this collection's scan.
	let people =
		std::iter::once(r#""zz""#.to_string()).chain(std::iter::repeat_with(null).take(19));
	// LitePeople: the target holds seq 3.
	let lite = (0..10).map(|seq| if seq == 3 { occupied(target, 4_000) } else { null() });
	let responses: Vec<String> = people.chain(lite).collect();
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let scans = futures::executor::block_on(scan_collections(
		&rpc,
		metadata,
		&signer,
		&PersonhoodCollection::ALL,
		CollectionScanParams {
			network_suffix: b"paseo",
			period: 7,
			target: &target,
			reuse_existing: true,
		},
	))
	.expect("a broken People scan must not fail the pass");

	assert_eq!(allocated_in(&scans), Some((PersonhoodCollection::LitePeople, 3)),);
}

/// Storage keys the scripted transport read, one per key of a batched read.
fn storage_reads(scripted: &ScriptedRpc) -> usize {
	scripted
		.calls()
		.iter()
		.map(|(method, params)| match method.as_str() {
			"state_getStorage" => 1,
			"state_queryStorageAt" => serde_json::from_str::<serde_json::Value>(params)
				.expect("batched read params are JSON")[0]
				.as_array()
				.map_or(0, Vec::len),
			_ => 0,
		})
		.sum()
}

/// A full table is no longer a dead end: the oldest slot the runtime allows
/// taking is replaced, and the registration proceeds.
#[test]
fn a_full_table_replaces_the_oldest_replaceable_slot() {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ring = RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	};

	// Revision read, then ten occupied slots where seq 3 is the oldest, then
	// the post-submit verification read.
	let mut owned = vec!["null".to_string()];
	owned.extend((0..10u64).map(|seq| {
		let since = if seq == 3 { 1_000 } else { NOW - 1_000 };
		slot_entry_since([0x99; 32], since)
	}));
	owned.push(chain_clock(NOW));
	owned.push(slot_entry([0x22; 32]));
	let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let outcome = futures::executor::block_on(register_statement_account(
		&rpc,
		metadata,
		target_chain,
		&signer,
		RegistrationParams {
			target: &[0x22; 32],
			period: 7,
			network_suffix: b"paseo",
			ring: &ring,
			reuse_existing: true,
			preselected: None,
			protected: &[],
		},
	));

	assert!(
		matches!(outcome.unwrap(), RegistrationOutcome::Registered { seq: 3, .. }),
		"the oldest occupied slot should have been replaced",
	);
}

/// A duplicate-submit retry must not revoke a second allowance: the first
/// submission can still land, so two takeovers for one call can cost two.
#[test]
fn a_duplicate_submit_retry_does_not_take_over_a_second_slot() {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ring = RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	};

	// Revision, ten occupied slots of differing age, the chain clock. The
	// submission then fails as a duplicate, and the rescan would otherwise
	// pick a second victim.
	let mut owned = vec!["null".to_string()];
	owned.extend((0..10u64).map(|seq| slot_entry_since([0x99; 32], 1_000 + seq)));
	owned.push(chain_clock(NOW));
	// Second pass through the loop: revision is cached, so scan then clock.
	owned.extend((0..10u64).map(|seq| slot_entry_since([0x99; 32], 1_000 + seq)));
	owned.push(chain_clock(NOW));
	let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription_errors("already imported", 2);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let err = futures::executor::block_on(register_statement_account(
		&rpc,
		metadata,
		target_chain,
		&signer,
		RegistrationParams {
			target: &[0x22; 32],
			period: 7,
			network_suffix: b"paseo",
			ring: &ring,
			reuse_existing: true,
			preselected: None,
			protected: &[],
		},
	))
	.unwrap_err();

	assert!(
		err.to_string().contains("no free"),
		"a retry after a takeover should stop, not replace again: {err}"
	);
}

/// A takeover the runtime refuses is reported as such, not as a bare RPC
/// failure: the host loses the race whenever the chain's clock disagrees.
#[test]
fn a_refused_takeover_is_named() {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ring = RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	};

	let mut owned = vec!["null".to_string()];
	owned.extend((0..10u64).map(|seq| slot_entry_since([0x99; 32], 1_000 + seq)));
	owned.push(chain_clock(NOW));
	let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription_errors("User error: Invalid Transaction (1010)", 1);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let err = futures::executor::block_on(register_statement_account(
		&rpc,
		metadata,
		target_chain,
		&signer,
		RegistrationParams {
			target: &[0x22; 32],
			period: 7,
			network_suffix: b"paseo",
			ring: &ring,
			reuse_existing: true,
			preselected: None,
			protected: &[],
		},
	))
	.unwrap_err();

	assert!(err.to_string().contains("not replaceable yet"), "unexpected error: {err}");
}

/// Everything occupied and still inside the cooldown stays an error.
#[test]
fn a_full_table_within_the_cooldown_still_fails() {
	let metadata = test_fixtures::people();
	let extrinsics = test_fixtures::people_extrinsics();
	let target_chain = test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ring = RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	};

	let mut owned = vec!["null".to_string()];
	owned.extend((0..10).map(|_| slot_entry_since([0x99; 32], NOW - 10)));
	owned.push(chain_clock(NOW));
	let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
	let scripted = ScriptedRpc::new(responses);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let err = futures::executor::block_on(register_statement_account(
		&rpc,
		metadata,
		target_chain,
		&signer,
		RegistrationParams {
			target: &[0x22; 32],
			period: 7,
			network_suffix: b"paseo",
			ring: &ring,
			reuse_existing: true,
			preselected: None,
			protected: &[],
		},
	))
	.unwrap_err();

	assert!(err.to_string().contains("no free"), "unexpected error: {err}");
}

#[test]
fn registration_is_verified_at_the_included_block() {
	let (outcome, scripted) = scripted_registration(&slot_entry([0x22; 32]));

	assert!(matches!(
		outcome.unwrap(),
		RegistrationOutcome::Registered {
			block_hash,
			seq: 0,
			ring_index: 0,
			collection: PersonhoodCollection::LitePeople,
		}
			if block_hash == "0xb10c"
	));
	let (method, params) = scripted.calls().last().cloned().unwrap();
	assert_eq!(method, "state_getStorage");
	assert!(
		params.ends_with(r#","0xb10c"]"#),
		"verification read not pinned to the included block: {params}"
	);
}

#[test]
fn a_preselected_slot_is_not_rescanned() {
	// No slots are scripted as free: if the caller's selection were ignored
	// and the slots rescanned, the scripted transport would run dry.
	let (outcome, scripted) =
		scripted_registration_with(&slot_entry([0x22; 32]), Some(Preselected::Free(0)), 0);

	assert!(matches!(outcome.unwrap(), RegistrationOutcome::Registered { seq: 0, .. }));
	// The ring revision and the verification read at the included block,
	// and nothing else.
	assert_eq!(storage_reads(&scripted), 2);
}

#[test]
fn a_scan_without_a_preselection_reads_every_slot() {
	let (outcome, scripted) = scripted_registration(&slot_entry([0x22; 32]));

	assert!(outcome.is_ok());
	// The ring revision, ten slots scanned, and the verification read.
	assert_eq!(storage_reads(&scripted), 12);
}

#[test]
fn registration_fails_when_the_included_block_lacks_the_slot() {
	let (outcome, _scripted) = scripted_registration(&slot_entry([0x99; 32]));

	let err = outcome.unwrap_err();
	assert!(err.to_string().contains("0xb10c"), "unexpected error: {err}");
}

const NOTIFICATION_TARGET: [u8; 32] = [0x22; 32];
const NOTIFICATION_PERIOD: u32 = 20_756;

fn lite_entropy() -> [u8; 32] {
	[0x11; 32]
}

fn lite_ring() -> RingParams {
	RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(lite_entropy())],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	}
}

/// Claim [`NOTIFICATION_TARGET`] as a lite person over `rpc`, with `signer`.
fn run_notification_claim(
	rpc: RpcClient,
	signer: &dyn PersonhoodSigner,
	submission: ClaimSubmission,
) -> Result<NotificationAccountOutcome, StatementAllowanceError> {
	let extrinsics = test_fixtures::people_extrinsics();
	futures::executor::block_on(claim_notification_account(NotificationAccountClaim {
		rpc: &rpc,
		metadata: test_fixtures::people(),
		target_chain: test_fixtures::people_target(&extrinsics),
		signer,
		network_suffix: b"paseo",
		target: &NOTIFICATION_TARGET,
		period: NOTIFICATION_PERIOD,
		memberships: core::slice::from_ref(&lite_ring()),
		submission,
	}))
}

/// Answers for the ring revision and the scan of the fixture's two lite
/// slots of an empty period, then `tail`.
fn empty_period_then(tail: &[&str]) -> ScriptedRpc {
	let mut responses = vec!["null"; 3];
	responses.extend_from_slice(tail);
	ScriptedRpc::new(responses)
}

/// A notification claim for [`NOTIFICATION_TARGET`] by a lite person whose
/// period is empty, answering `tail` after the ring revision and the scan
/// of the fixture's two lite slots.
fn scripted_notification_claim(
	submission: ClaimSubmission,
	tail: &[&str],
) -> (Result<NotificationAccountOutcome, StatementAllowanceError>, ScriptedRpc) {
	let candidates = lite_people(lite_entropy());
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let scripted = empty_period_then(tail);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let outcome = run_notification_claim(
		RpcClient::new(HostRpcClient::new(scripted.clone())),
		&signer,
		submission,
	);
	(outcome, scripted)
}

fn submissions(scripted: &ScriptedRpc) -> usize {
	scripted
		.calls()
		.iter()
		.filter(|(method, _)| method == "author_submitAndWatchExtrinsic")
		.count()
}

/// A claim the pool already holds an equivalent of, as one from a
/// concurrent call or an earlier try for the same slot.
fn duplicate_claim(
	tail: &[&str],
) -> (Result<NotificationAccountOutcome, StatementAllowanceError>, ScriptedRpc) {
	let candidates = lite_people(lite_entropy());
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let scripted = empty_period_then(tail);
	scripted.script_subscription_errors("Priority is too low: (0 vs 0)", 1);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let outcome = run_notification_claim(
		RpcClient::new(HostRpcClient::new(scripted.clone())),
		&signer,
		ClaimSubmission::Submit,
	);
	(outcome, scripted)
}

/// The pool's answer is not the chain's: a target the chain registered
/// meanwhile is the claim's success, with no second slot spent.
#[test]
fn a_duplicate_claim_whose_target_is_registered_reports_that_registration() {
	let alias = format!(r#""0x{}""#, hex::encode([0x33; 32]));
	let held = notification_registration(NOTIFICATION_TARGET, NOTIFICATION_PERIOD, 0);
	let (outcome, scripted) = duplicate_claim(&[&alias, &held]);

	assert_eq!(
		(outcome.unwrap(), submissions(&scripted)),
		(NotificationAccountOutcome::AlreadyRegistered { seq: 0 }, 1),
	);
}

/// Neither the slot nor the target registered: the claim in the pool may
/// still land, so trying another slot could spend two.
#[test]
fn a_duplicate_claim_with_nothing_registered_is_uncertain_and_claims_no_other_slot() {
	let (outcome, scripted) = duplicate_claim(&["null", "null"]);

	assert_eq!(
		(outcome.unwrap_err().to_string(), submissions(&scripted)),
		(
			format!(
				"a notification claim for slot 0 of period {NOTIFICATION_PERIOD} may still land; not claiming another"
			),
			1,
		),
	);
}

/// The slot went to another account: this claim cannot land, so the next
/// free slot is claimed.
#[test]
fn a_duplicate_claim_whose_slot_went_elsewhere_claims_the_next_one() {
	let other = notification_registration([0x44; 32], NOTIFICATION_PERIOD, 0);
	let verified = notification_registration(NOTIFICATION_TARGET, NOTIFICATION_PERIOD, 1);
	let (outcome, scripted) = duplicate_claim(&["null", &other, &other, "null", &verified]);

	assert_eq!(
		(outcome.unwrap(), submissions(&scripted)),
		(
			NotificationAccountOutcome::Registered {
				block_hash: "0xb10c".to_string(),
				seq: 1,
				collection: PersonhoodCollection::LitePeople,
			},
			2,
		),
	);
}

/// A submission whose answer was lost may still have landed: the chain
/// decides, and nothing is submitted again.
#[test]
fn a_failed_submission_whose_target_is_registered_reports_that_registration() {
	let candidates = lite_people(lite_entropy());
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let alias = format!(r#""0x{}""#, hex::encode([0x33; 32]));
	let held = notification_registration(NOTIFICATION_TARGET, NOTIFICATION_PERIOD, 0);
	let scripted = empty_period_then(&[&alias, &held]);
	scripted.script_subscription_errors("connection reset", 1);
	let outcome = run_notification_claim(
		RpcClient::new(HostRpcClient::new(scripted.clone())),
		&signer,
		ClaimSubmission::Submit,
	);

	assert_eq!(
		(outcome.unwrap(), submissions(&scripted)),
		(NotificationAccountOutcome::AlreadyRegistered { seq: 0 }, 1),
	);
}

/// A transport that ends the signer's session while the slot scan is
/// answered, as a logout landing during that read.
struct EndsSessionOnScan {
	inner: ScriptedRpc,
	ended: Arc<std::sync::atomic::AtomicBool>,
}

impl subxt_rpcs::client::RpcClientT for EndsSessionOnScan {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<subxt_rpcs::client::RawValue>>,
	) -> subxt_rpcs::client::RawRpcFuture<'a, Box<subxt_rpcs::client::RawValue>> {
		if method == "state_queryStorageAt" {
			self.ended.store(true, std::sync::atomic::Ordering::SeqCst);
		}
		self.inner.request_raw(method, params)
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<subxt_rpcs::client::RawValue>>,
		unsub: &'a str,
	) -> subxt_rpcs::client::RawRpcFuture<'a, subxt_rpcs::client::RawRpcSubscription> {
		self.inner.subscribe_raw(sub, params, unsub)
	}
}

/// Personhood keys lent by a session, refused once `ended` is set, as a
/// wallet's keys are after its session ends.
struct SessionBoundSigner<'a> {
	keys: &'a dyn PersonhoodSigner,
	ended: Arc<std::sync::atomic::AtomicBool>,
}

impl SessionBoundSigner<'_> {
	fn still_current(&self) -> Result<(), StatementAllowanceError> {
		if self.ended.load(std::sync::atomic::Ordering::SeqCst) {
			return Err(crate::runtime::authority::AuthorityError::Disconnected.into());
		}
		Ok(())
	}
}

impl PersonhoodSigner for SessionBoundSigner<'_> {
	fn member(
		&self,
		collection: PersonhoodCollection,
	) -> Result<[u8; 32], StatementAllowanceError> {
		self.still_current()?;
		self.keys.member(collection)
	}

	fn alias(
		&self,
		collection: PersonhoodCollection,
		context: &[u8],
	) -> Result<[u8; 32], StatementAllowanceError> {
		self.still_current()?;
		self.keys.alias(collection, context)
	}

	fn prove(
		&self,
		ring: &RingParams,
		context: &[u8],
		message: &[u8],
	) -> Result<Vec<u8>, StatementAllowanceError> {
		self.still_current()?;
		self.keys.prove(ring, context, message)
	}
}

/// The keys are lent by a session; once it ends, no proof is made and no
/// claim is submitted, even for a claim already under way.
#[test]
fn a_session_that_ends_during_the_scan_submits_no_claim() {
	let candidates = lite_people(lite_entropy());
	let keys = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ended = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let signer = SessionBoundSigner { keys: &keys, ended: ended.clone() };
	let scripted = empty_period_then(&[]);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let outcome = run_notification_claim(
		RpcClient::new(HostRpcClient::new(EndsSessionOnScan { inner: scripted.clone(), ended })),
		&signer,
		ClaimSubmission::Submit,
	);

	assert_eq!(
		(outcome.unwrap_err().to_string(), submissions(&scripted)),
		("Disconnected".to_string(), 0),
	);
}

fn notification_registration(account: [u8; 32], period: u32, seq: u8) -> String {
	format!(r#""0x{}""#, hex::encode((account, period, seq).encode()))
}

/// The submitted claim must name the target and the free slot the scan
/// found, or the allowance lands on someone else's account.
#[test]
fn a_notification_claim_registers_the_target_in_the_first_free_slot() {
	let verified = notification_registration(NOTIFICATION_TARGET, NOTIFICATION_PERIOD, 0);
	let (outcome, scripted) =
		scripted_notification_claim(ClaimSubmission::Submit, &[verified.as_str()]);

	assert_eq!(
		outcome.unwrap(),
		NotificationAccountOutcome::Registered {
			block_hash: "0xb10c".to_string(),
			seq: 0,
			collection: PersonhoodCollection::LitePeople,
		},
	);
	let metadata = subxt::Metadata::decode_from(test_fixtures::PEOPLE_METADATA).unwrap();
	let call = subxt::ext::frame_decode::extrinsics::encode_call_data(
		RESOURCES_PALLET,
		"set_notification_statement_account_for_sequence",
		&extrinsic::SetNotificationAccountCallArgs {
			reference: (NOTIFICATION_PERIOD, 0),
			account_id: NOTIFICATION_TARGET,
		},
		&metadata,
		metadata.types(),
	)
	.unwrap();
	let submitted = scripted
		.calls()
		.into_iter()
		.find(|(method, _)| method == "author_submitAndWatchExtrinsic")
		.map(|(_, params)| params)
		.unwrap();
	assert!(
		submitted.ends_with(&format!("{}\"]", hex::encode(&call))),
		"the extrinsic does not end in the claim call: {submitted}"
	);
}

#[test]
fn a_validate_only_claim_submits_nothing() {
	let head = format!(r#""0x{}""#, hex::encode([0xf1; 32]));
	let (valid, scripted) =
		scripted_notification_claim(ClaimSubmission::ValidateOnly, &[&head, r#""0x00""#]);
	let (refused, _) =
		scripted_notification_claim(ClaimSubmission::ValidateOnly, &[&head, r#""0x010007e1""#]);

	let validation = scripted
		.calls()
		.into_iter()
		.find(|(method, _)| method == "state_call")
		.map(|(_, params)| serde_json::from_str::<Value>(&params).unwrap())
		.unwrap();
	let finalized = format!("0x{}", hex::encode([0xf1; 32]));
	assert_eq!(
		(
			&validation[0],
			validation[1].as_str().unwrap().ends_with(&finalized[2..]),
			&validation[2],
		),
		(&json!("TaggedTransactionQueue_validate_transaction"), true, &json!(finalized)),
		"the claim is validated at the finalized head, in the arguments and as the block"
	);
	assert_eq!(
		(
			valid.unwrap(),
			scripted
				.calls()
				.iter()
				.any(|(method, _)| method == "author_submitAndWatchExtrinsic"),
			refused.unwrap_err().to_string(),
		),
		(
			NotificationAccountOutcome::Validated {
				seq: 0,
				collection: PersonhoodCollection::LitePeople,
			},
			false,
			format!(
				"notification claim (period {NOTIFICATION_PERIOD}, seq 0) does not validate: Invalid::Custom(225)"
			),
		),
	);
}

/// The runtime registers an account once, so an account from an earlier
/// period must not look free for a new claim.
#[test]
fn a_notification_account_holds_one_period() {
	let alias = format!(r#""0x{}""#, hex::encode([0x33; 32]));
	let read = |registration: &str| {
		let scripted = ScriptedRpc::new([alias.as_str(), registration]);
		let rpc = RpcClient::new(HostRpcClient::new(scripted));
		futures::executor::block_on(notification_registration_in(
			&rpc,
			&NOTIFICATION_TARGET,
			NOTIFICATION_PERIOD,
		))
		.map_err(|err| err.to_string())
	};
	let none = {
		let rpc = RpcClient::new(HostRpcClient::new(ScriptedRpc::new(["null"])));
		futures::executor::block_on(notification_registration_in(
			&rpc,
			&NOTIFICATION_TARGET,
			NOTIFICATION_PERIOD,
		))
		.map_err(|err| err.to_string())
	};

	assert_eq!(
		[
			none,
			read(&notification_registration(NOTIFICATION_TARGET, NOTIFICATION_PERIOD, 4)),
			read(&notification_registration(NOTIFICATION_TARGET, NOTIFICATION_PERIOD - 1, 4)),
		],
		[
			Ok(None),
			Ok(Some(4)),
			Err(format!(
				"account already holds a notification registration from period {}",
				NOTIFICATION_PERIOD - 1
			)),
		],
	);
}

/// Scan `collections` for [`NOTIFICATION_TARGET`], then claim its general
/// Statement Store slot from that scan, as the signing host does.
fn run_statement_claim(
	rpc: RpcClient,
	signer: &dyn PersonhoodSigner,
	collections: &[PersonhoodCollection],
	memberships: &[RingParams],
	submission: ClaimSubmission,
) -> Result<StatementAccountOutcome, StatementAllowanceError> {
	let extrinsics = test_fixtures::people_extrinsics();
	let metadata = test_fixtures::people();
	futures::executor::block_on(async {
		let scans = scan_collections(
			&rpc,
			metadata,
			signer,
			collections,
			CollectionScanParams {
				network_suffix: b"paseo",
				period: NOTIFICATION_PERIOD,
				target: &NOTIFICATION_TARGET,
				reuse_existing: true,
			},
		)
		.await?;
		claim_statement_account(StatementAccountClaim {
			rpc: &rpc,
			metadata,
			target_chain: test_fixtures::people_target(&extrinsics),
			signer,
			network_suffix: b"paseo",
			target: &NOTIFICATION_TARGET,
			period: NOTIFICATION_PERIOD,
			collections,
			scans: &scans,
			memberships,
			submission,
		})
		.await
	})
}

/// A chain answering `answers` in order, whose first `submit_errors`
/// submissions fail with `submit_error` before one reaches block `0xb10c`.
fn statement_claim_chain(
	answers: &[&str],
	submit_error: &str,
	submit_errors: usize,
) -> ScriptedRpc {
	let scripted = ScriptedRpc::new(answers.to_vec());
	if submit_errors > 0 {
		scripted.script_subscription_errors(submit_error, submit_errors);
	}
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	scripted
}

/// The pool's answer to a copy of a claim it already holds.
const DUPLICATE: &str = "Priority is too low: (0 vs 0)";

/// Claim as a lite person only, over `answers`.
fn scripted_statement_claim(
	submission: ClaimSubmission,
	answers: &[&str],
	submit_errors: usize,
) -> (Result<StatementAccountOutcome, StatementAllowanceError>, ScriptedRpc) {
	scripted_statement_claim_failing(submission, answers, DUPLICATE, submit_errors)
}

fn scripted_statement_claim_failing(
	submission: ClaimSubmission,
	answers: &[&str],
	submit_error: &str,
	submit_errors: usize,
) -> (Result<StatementAccountOutcome, StatementAllowanceError>, ScriptedRpc) {
	let candidates = lite_people(lite_entropy());
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let scripted = statement_claim_chain(answers, submit_error, submit_errors);
	let outcome = run_statement_claim(
		RpcClient::new(HostRpcClient::new(scripted.clone())),
		&signer,
		&[PersonhoodCollection::LitePeople],
		core::slice::from_ref(&lite_ring()),
		submission,
	);
	(outcome, scripted)
}

/// Claim as a person in both collections, People first, over `answers`.
fn scripted_pooled_statement_claim(
	answers: &[&str],
	memberships: &[RingParams],
	submit_errors: usize,
) -> (Result<StatementAccountOutcome, StatementAllowanceError>, ScriptedRpc) {
	let candidates = pooled_candidates();
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let scripted = statement_claim_chain(answers, DUPLICATE, submit_errors);
	let outcome = run_statement_claim(
		RpcClient::new(HostRpcClient::new(scripted.clone())),
		&signer,
		&PersonhoodCollection::ALL,
		memberships,
		ClaimSubmission::Submit,
	);
	(outcome, scripted)
}

/// The answers to one scan of the fixture's ten lite slots.
fn slots(entries: [&str; 10]) -> Vec<&str> {
	entries.to_vec()
}

/// Whether anything past the slot scans was read: a ring revision, a proof
/// or a submission.
fn read_past_the_scan(scripted: &ScriptedRpc) -> bool {
	scripted.calls().iter().any(|(method, _)| method != "state_queryStorageAt")
}

/// The answer a scan of one table gives, `held` in slot `at` and the rest free.
fn one_held(held: &str, at: usize, len: usize) -> Vec<String> {
	(0..len)
		.map(|seq| if seq == at { held.to_string() } else { "null".to_string() })
		.collect()
}

#[test]
fn a_statement_account_takes_the_first_free_slot_verified_at_its_block() {
	let mine = slot_entry(NOTIFICATION_TARGET);
	let other = slot_entry([0x44; 32]);
	let mut answers =
		slots([&other, "null", "null", "null", "null", "null", "null", "null", "null", "null"]);
	answers.extend(["null", &mine]);
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 0);

	assert_eq!(
		(outcome.unwrap(), submissions(&scripted)),
		(
			StatementAccountOutcome::Registered {
				block_hash: "0xb10c".to_string(),
				seq: 1,
				collection: PersonhoodCollection::LitePeople,
			},
			1,
		),
	);
}

/// The submitted claim names the target and the free slot, so the
/// allowance lands on the account the host derived and on no other.
#[test]
fn a_statement_claim_submits_the_call_for_its_target_and_slot() {
	let mine = slot_entry(NOTIFICATION_TARGET);
	let mut answers = slots(["null"; 10]);
	answers.extend(["null", &mine]);
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 0);
	assert!(outcome.is_ok());
	let metadata = subxt::Metadata::decode_from(test_fixtures::PEOPLE_METADATA).unwrap();
	let call = subxt::ext::frame_decode::extrinsics::encode_call_data(
		RESOURCES_PALLET,
		"set_statement_store_account",
		&extrinsic::SetStatementStoreAccountCallArgs {
			period: NOTIFICATION_PERIOD,
			seq: 0,
			target_account: NOTIFICATION_TARGET,
		},
		&metadata,
		metadata.types(),
	)
	.unwrap();
	let submitted = scripted
		.calls()
		.into_iter()
		.find(|(method, _)| method == "author_submitAndWatchExtrinsic")
		.map(|(_, params)| params)
		.unwrap();
	assert!(
		submitted.ends_with(&format!("{}\"]", hex::encode(&call))),
		"the extrinsic does not end in the claim call: {submitted}"
	);
}

/// An account this period already funds claims nothing more, and needs no
/// ring to be reused.
#[test]
fn a_statement_account_that_holds_a_slot_is_reused_with_no_ring_read() {
	let mine = slot_entry(NOTIFICATION_TARGET);
	let answers =
		slots(["null", &mine, "null", "null", "null", "null", "null", "null", "null", "null"]);
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 0);

	assert_eq!(
		(outcome.unwrap(), read_past_the_scan(&scripted)),
		(
			StatementAccountOutcome::AlreadyAllocated {
				seq: 1,
				collection: PersonhoodCollection::LitePeople,
			},
			false,
		),
	);
}

/// A grant held from before is reused even when no ring can be proved now.
#[test]
fn a_held_slot_is_reused_without_any_membership() {
	let candidates = lite_people(lite_entropy());
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let mine = slot_entry(NOTIFICATION_TARGET);
	let scripted = statement_claim_chain(
		&slots(["null", "null", &mine, "null", "null", "null", "null", "null", "null", "null"]),
		DUPLICATE,
		0,
	);
	let outcome = run_statement_claim(
		RpcClient::new(HostRpcClient::new(scripted.clone())),
		&signer,
		&[PersonhoodCollection::LitePeople],
		&[],
		ClaimSubmission::Submit,
	);

	assert_eq!(
		(outcome.unwrap(), read_past_the_scan(&scripted)),
		(
			StatementAccountOutcome::AlreadyAllocated {
				seq: 2,
				collection: PersonhoodCollection::LitePeople,
			},
			false,
		),
	);
}

/// A slot held in one collection is found before another collection's free
/// slot is claimed: one account never gets two.
#[test]
fn a_slot_held_in_lite_people_is_reused_while_people_has_a_free_one() {
	let mine = slot_entry(NOTIFICATION_TARGET);
	let mut answers = vec!["null".to_string(); 20];
	answers.extend(one_held(&mine, 4, 10));
	let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
	let (outcome, scripted) = scripted_pooled_statement_claim(&answers, &pooled_memberships(), 0);

	assert_eq!(
		(outcome.unwrap(), read_past_the_scan(&scripted)),
		(
			StatementAccountOutcome::AlreadyAllocated {
				seq: 4,
				collection: PersonhoodCollection::LitePeople,
			},
			false,
		),
	);
}

/// Unlike an allocation for the product-wide account, a conversation's
/// account never revokes another live allowance to get one.
#[test]
fn a_full_period_is_refused_without_replacing_any_slot() {
	let other = slot_entry_since([0x44; 32], 0);
	let answers = slots([&other; 10]);
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 0);

	assert_eq!(
		(outcome.unwrap_err().to_string(), read_past_the_scan(&scripted)),
		(format!("no free StatementStore slot in period {NOTIFICATION_PERIOD} (max 10)"), false),
	);
}

fn uncertain() -> String {
	format!(
		"a statement-account claim for slot 0 of period {NOTIFICATION_PERIOD} may still land; not claiming another"
	)
}

/// The pool refused a copy and the target holds nothing: the claim in the
/// pool may still land, so no other slot is claimed.
#[test]
fn a_duplicate_statement_claim_with_nothing_registered_is_uncertain() {
	let mut answers = slots(["null"; 10]);
	answers.push("null");
	answers.extend(slots(["null"; 10]));
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 1);

	assert_eq!((outcome.unwrap_err().to_string(), submissions(&scripted)), (uncertain(), 1),);
}

/// A general slot can change hands, so another account in it does not
/// show this claim cannot land: still uncertain, and no next slot.
#[test]
fn a_duplicate_statement_claim_whose_slot_went_elsewhere_is_uncertain() {
	let other = slot_entry([0x44; 32]);
	let mut answers = slots(["null"; 10]);
	answers.push("null");
	answers.extend(slots([
		&other, "null", "null", "null", "null", "null", "null", "null", "null", "null",
	]));
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 1);

	assert_eq!((outcome.unwrap_err().to_string(), submissions(&scripted)), (uncertain(), 1),);
}

/// The pool's refusal is settled by the chain: a target registered
/// meanwhile is the claim's success.
#[test]
fn a_duplicate_statement_claim_whose_target_is_registered_reports_it() {
	let mine = slot_entry(NOTIFICATION_TARGET);
	let mut answers = slots(["null"; 10]);
	answers.push("null");
	answers.extend(slots([
		&mine, "null", "null", "null", "null", "null", "null", "null", "null", "null",
	]));
	let (outcome, scripted) = scripted_statement_claim(ClaimSubmission::Submit, &answers, 1);

	assert_eq!(
		(outcome.unwrap(), submissions(&scripted)),
		(
			StatementAccountOutcome::AlreadyAllocated {
				seq: 0,
				collection: PersonhoodCollection::LitePeople,
			},
			1,
		),
	);
}

/// Settling reads every collection: a claim in People that failed, with the
/// target meanwhile registered in LitePeople, is that registration.
#[test]
fn settling_a_failed_claim_finds_the_target_in_another_collection() {
	let mine = slot_entry(NOTIFICATION_TARGET);
	let mut answers = vec!["null".to_string(); 30];
	answers.push("null".to_string());
	answers.extend(vec!["null".to_string(); 20]);
	answers.extend(one_held(&mine, 3, 10));
	let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
	let (outcome, scripted) = scripted_pooled_statement_claim(&answers, &pooled_memberships(), 1);

	assert_eq!(
		(outcome.unwrap(), submissions(&scripted)),
		(
			StatementAccountOutcome::AlreadyAllocated {
				seq: 3,
				collection: PersonhoodCollection::LitePeople,
			},
			1,
		),
	);
}

/// The runtime's refusal of the claim is final: reported as it is, with
/// no other slot claimed.
#[test]
fn an_invalid_statement_claim_is_reported_and_not_retried() {
	let mut answers = slots(["null"; 10]);
	answers.push("null");
	answers.extend(slots(["null"; 10]));
	let (outcome, scripted) = scripted_statement_claim_failing(
		ClaimSubmission::Submit,
		&answers,
		"Invalid Transaction: Transaction has a bad signature",
		1,
	);

	let error = outcome.unwrap_err().to_string();
	assert!(error.contains("Invalid Transaction"), "unexpected error: {error}");
	assert_eq!(submissions(&scripted), 1);
}

/// The keys are the session's: a logout during the scan stops the claim
/// before its proof, so nothing is submitted.
#[test]
fn a_session_that_ends_during_a_statement_claim_submits_nothing() {
	let candidates = lite_people(lite_entropy());
	let keys = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ended = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let signer = SessionBoundSigner { keys: &keys, ended: ended.clone() };
	let mut answers = slots(["null"; 10]);
	answers.push("null");
	let scripted = statement_claim_chain(&answers, DUPLICATE, 0);
	let outcome = run_statement_claim(
		RpcClient::new(HostRpcClient::new(EndsSessionOnScan { inner: scripted.clone(), ended })),
		&signer,
		&[PersonhoodCollection::LitePeople],
		core::slice::from_ref(&lite_ring()),
		ClaimSubmission::Submit,
	);

	assert_eq!(
		(outcome.unwrap_err().to_string(), submissions(&scripted)),
		("Disconnected".to_string(), 0),
	);
}

#[test]
fn a_validate_only_statement_claim_submits_nothing() {
	let head = format!(r#""0x{}""#, hex::encode([0xf1; 32]));
	let mut answers = slots(["null"; 10]);
	answers.extend(["null", head.as_str(), r#""0x00""#]);
	let (valid, scripted) = scripted_statement_claim(ClaimSubmission::ValidateOnly, &answers, 0);
	let mut answers = slots(["null"; 10]);
	answers.extend(["null", head.as_str(), r#""0x010007e1""#]);
	let (refused, refused_chain) =
		scripted_statement_claim(ClaimSubmission::ValidateOnly, &answers, 0);

	let validation = scripted
		.calls()
		.into_iter()
		.find(|(method, _)| method == "state_call")
		.map(|(_, params)| serde_json::from_str::<Value>(&params).unwrap())
		.unwrap();
	assert_eq!(validation[0], json!("TaggedTransactionQueue_validate_transaction"));
	assert_eq!(
		(
			valid.unwrap(),
			submissions(&scripted) + submissions(&refused_chain),
			refused.unwrap_err().to_string()
		),
		(
			StatementAccountOutcome::Validated {
				seq: 0,
				collection: PersonhoodCollection::LitePeople,
			},
			0,
			format!(
				"statement-account claim (period {NOTIFICATION_PERIOD}, seq 0) does not validate: Invalid::Custom(225)"
			),
		),
	);
}

/// A transport that ends the signer's session at its `after`-th
/// `state_getStorage` read, as a logout or a wallet switch landing while a
/// claim reads the chain.
struct EndsSessionOnRead {
	inner: ScriptedRpc,
	ended: Arc<std::sync::atomic::AtomicBool>,
	reads: std::sync::atomic::AtomicUsize,
	after: usize,
}

impl subxt_rpcs::client::RpcClientT for EndsSessionOnRead {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<subxt_rpcs::client::RawValue>>,
	) -> subxt_rpcs::client::RawRpcFuture<'a, Box<subxt_rpcs::client::RawValue>> {
		if method == "state_getStorage" &&
			self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 == self.after
		{
			self.ended.store(true, std::sync::atomic::Ordering::SeqCst);
		}
		self.inner.request_raw(method, params)
	}

	fn subscribe_raw<'a>(
		&'a self,
		sub: &'a str,
		params: Option<Box<subxt_rpcs::client::RawValue>>,
		unsub: &'a str,
	) -> subxt_rpcs::client::RawRpcFuture<'a, subxt_rpcs::client::RawRpcSubscription> {
		self.inner.subscribe_raw(sub, params, unsub)
	}
}

/// A Bulletin long-term storage claim proves with the session's keys: once
/// the session ends mid-claim, no proof is made and nothing is submitted.
#[test]
fn a_session_that_ends_during_a_long_term_storage_claim_submits_nothing() {
	let candidates = lite_people(lite_entropy());
	let keys = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let ended = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let signer = SessionBoundSigner { keys: &keys, ended: ended.clone() };
	// The ring's revision, then counter 0 already spent; the session ends
	// with that second read, before counter 1's alias.
	let scripted = ScriptedRpc::new(["null", r#""0x01""#, "null"]);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(EndsSessionOnRead {
		inner: scripted.clone(),
		ended,
		reads: Default::default(),
		after: 2,
	}));
	let extrinsics = test_fixtures::people_extrinsics();
	let outcome = futures::executor::block_on(claim_long_term_storage(LongTermStorageClaim {
		rpc: &rpc,
		metadata: test_fixtures::people(),
		target_chain: test_fixtures::people_target(&extrinsics),
		signer: &signer,
		network_suffix: b"paseo",
		target: &NOTIFICATION_TARGET,
		period: 1,
		ring: &lite_ring(),
	}));

	let Err(error) = outcome else { panic!("a claim for a session that ended must fail") };
	assert_eq!((error.to_string(), submissions(&scripted)), ("Disconnected".to_string(), 0));
}
