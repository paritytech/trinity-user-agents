use subxt_rpcs::RpcClient as HostRpcClient;

use super::{
	super::{rpc::testing::ScriptedRpc, test_fixtures},
	testing::lite_people,
	*,
};
use crate::runtime::statement_allowance::FixedPersonhoodSigner;

/// Fixture metadata captured from paseo-next-v2; its
/// `LiteStmtStoreSlotsPerPeriod` is 10.
const SLOTS: usize = 10;
const NETWORK_SUFFIX: &[u8] = b"paseo";

/// `StmtStoreAllowanceEntry { account_id, seq: 0, since: 0 }` as a scripted
/// JSON storage result.
fn slot_entry(account: [u8; 32]) -> String {
	entry_with_since(account, 0)
}

/// A scripted slot entry that was set at `since`.
fn entry_with_since(account: [u8; 32], since: u64) -> String {
	let entry = StatementStoreAllowanceEntry { account_id: account, seq: 0, since }.encode();
	format!(r#""0x{}""#, hex::encode(entry))
}

/// An occupied-slot candidate for the replacement rule.
fn occupied(seq: u32, account_id: [u8; 32], since: u64) -> OccupiedSlot {
	OccupiedSlot { seq, account_id, since }
}

/// Run `scan_slot_excluding` for `[0x22; 32]` against a scripted period
/// whose slot occupancy is `slots`.
fn scripted_find(slots: &[Option<[u8; 32]>]) -> SlotSelection {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let metadata = test_fixtures::people();
	let entries: Vec<String> = slots
		.iter()
		.map(|slot| slot.map_or_else(|| "null".to_string(), slot_entry))
		.collect();
	let scripted = ScriptedRpc::new(entries.iter().map(String::as_str));
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	futures::executor::block_on(scan_slot_excluding(
		&rpc,
		metadata,
		SlotScan {
			collection: PersonhoodCollection::LitePeople,
			signer: &signer,
			network_suffix: NETWORK_SUFFIX,
			period: 7,
			target: &[0x22; 32],
			excluded: &[],
			reuse_existing: true,
		},
	))
	.unwrap()
}

/// The scan bound is whatever Asset Hub declares, not a compiled-in constant,
/// and it bounds how many keys a full scan hashes and reads.
///
/// Asset Hub budgets each collection separately, so both are pinned: one
/// number alone would still pass if the two constants were swapped.
#[test]
fn the_asset_hub_fixture_declares_a_daily_pgas_budget_per_collection() {
	let metadata = test_fixtures::asset_hub();

	assert_eq!(max_pgas_claims(metadata, PersonhoodCollection::People).unwrap(), 100,);
	assert_eq!(max_pgas_claims(metadata, PersonhoodCollection::LitePeople).unwrap(), 40,);
}

#[test]
fn an_empty_period_offers_the_first_slot() {
	assert_eq!(scripted_find(&[None; SLOTS]), SlotSelection::Free(0));
}

#[test]
fn the_slot_the_target_holds_is_found() {
	let mut slots = [None; SLOTS];
	slots[2] = Some([0x22; 32]);

	assert_eq!(scripted_find(&slots), SlotSelection::AlreadyAllocated(2));
}

/// The whole row is read in one round trip. One request per slot, one after
/// another, cost seconds per statement proof against a live chain.
#[test]
fn the_slot_scan_reads_its_row_in_one_round_trip() {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let mut entries = vec!["null".to_string(); SLOTS];
	entries[SLOTS - 1] = slot_entry([0x22; 32]);
	let scripted = ScriptedRpc::new(entries.iter().map(String::as_str));
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let selection = futures::executor::block_on(scan_slot_excluding(
		&rpc,
		test_fixtures::people(),
		SlotScan {
			collection: PersonhoodCollection::LitePeople,
			signer: &signer,
			network_suffix: NETWORK_SUFFIX,
			period: 7,
			target: &[0x22; 32],
			excluded: &[],
			reuse_existing: true,
		},
	))
	.unwrap();

	let methods: Vec<String> = scripted.calls().into_iter().map(|(method, _)| method).collect();
	assert_eq!(
		(selection, methods),
		(
			SlotSelection::AlreadyAllocated(
				u32::try_from(SLOTS).expect("slot count fits in u32") - 1
			),
			vec!["state_queryStorageAt".to_string()],
		),
	);
}

#[test]
fn a_table_filled_by_other_accounts_reports_full_rather_than_erroring() {
	let SlotSelection::Full { max, occupied } = scripted_find(&[Some([0x99; 32]); SLOTS]) else {
		panic!("a full table should report Full");
	};

	assert_eq!(max, u32::try_from(SLOTS).expect("slot count fits in u32"));
	// Every slot is reported, with the age the replacement rule needs.
	assert_eq!(occupied.len(), SLOTS);
	assert_eq!(occupied[0].seq, 0);
	assert_eq!(occupied[0].account_id, [0x99; 32]);
}

/// `since` values are what the replacement rule sorts on, so the scan has to
/// carry them through rather than discard them.
#[test]
fn the_scan_reports_each_occupied_slots_age() {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let metadata = test_fixtures::people();
	let entries: Vec<String> =
		(0..SLOTS).map(|seq| entry_with_since([0x99; 32], 1_000 + seq as u64)).collect();
	let scripted = ScriptedRpc::new(entries.iter().map(String::as_str).collect::<Vec<_>>());
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let SlotSelection::Full { occupied, .. } = futures::executor::block_on(scan_slot_excluding(
		&rpc,
		metadata,
		SlotScan {
			collection: PersonhoodCollection::LitePeople,
			signer: &signer,
			network_suffix: NETWORK_SUFFIX,
			period: 7,
			target: &[0x22; 32],
			excluded: &[],
			reuse_existing: true,
		},
	))
	.unwrap() else {
		panic!("a full table should report Full");
	};

	assert_eq!(
		occupied.iter().map(|slot| slot.since).collect::<Vec<_>>(),
		(0..SLOTS).map(|seq| 1_000 + seq as u64).collect::<Vec<_>>(),
	);
}

/// The oldest slot wins, and the target's own slot is never a candidate.
#[test]
fn the_oldest_replaceable_slot_is_chosen() {
	let candidates = [
		occupied(0, [0x99; 32], 5_000),
		occupied(1, [0x22; 32], 1_000),
		occupied(2, [0x98; 32], 2_000),
	];

	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]), Some(2),);
}

#[test]
fn a_slot_inside_its_cooldown_is_not_replaceable() {
	let candidates = [occupied(0, [0x99; 32], 9_990)];

	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]), None,);
}

/// The runtime requires `now > since + cooldown`, so a slot at exactly the
/// cooldown is still refused. Offering it means an extrinsic the chain
/// rejects as invalid, which is not retried.
#[test]
fn the_cooldown_boundary_is_strict() {
	let candidates = [occupied(0, [0x99; 32], 1_000)];

	assert_eq!(
		replaceable_slot(&candidates, &[0x22; 32], 1_060, 60, &[]),
		None,
		"age exactly at the cooldown must not be offered",
	);
	assert_eq!(
		replaceable_slot(&candidates, &[0x22; 32], 1_061, 60, &[]),
		Some(0),
		"one second past the cooldown is replaceable",
	);
}

/// `Timestamp.Now` is milliseconds; ages are judged in seconds.
#[test]
fn the_chain_clock_is_read_in_seconds() {
	let scripted = ScriptedRpc::new(vec![r#""0x60ea000000000000""#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	assert_eq!(futures::executor::block_on(read_chain_now_seconds(&rpc)).unwrap(), 60,);
}

#[test]
fn a_table_of_only_the_targets_own_and_cooling_slots_yields_nothing() {
	let candidates = [occupied(0, [0x22; 32], 1_000), occupied(1, [0x99; 32], 9_999)];

	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]), None,);
}

#[test]
fn an_excluded_slot_is_skipped_so_a_failed_replacement_moves_on() {
	let candidates = [occupied(0, [0x99; 32], 1_000), occupied(1, [0x98; 32], 2_000)];

	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[0]), Some(1),);
}

/// Equal ages resolve to the lowest seq, so retries do not oscillate.
#[test]
fn equal_ages_break_to_the_lowest_seq() {
	let candidates = [occupied(5, [0x99; 32], 1_000), occupied(2, [0x98; 32], 1_000)];

	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]), Some(2),);
}

/// A pass that registers several targets protects the slots it has already
/// claimed, so target N cannot take the slot target N-1 just got. Without
/// this a pass with more targets than slots undoes its own work forever.
#[test]
fn slots_already_claimed_in_this_pass_are_protected() {
	let candidates = [
		occupied(0, [0x01; 32], 1_000),
		occupied(1, [0x02; 32], 2_000),
		occupied(2, [0x03; 32], 3_000),
	];

	// Nothing claimed yet: the oldest goes.
	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[]), Some(0),);
	// Having claimed 0 and 1 earlier in the pass, only 2 is available.
	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[0, 1]), Some(2),);
	// Once every slot is one this pass claimed, the honest answer is none.
	assert_eq!(replaceable_slot(&candidates, &[0x22; 32], 10_000, 60, &[0, 1, 2]), None,);
}

/// Excluding a slot because a submission for it is in flight must not read as
/// "the period is full": the free slot is coming back, and a caller that
/// treats this as full would replace a live slot for nothing.
#[test]
fn an_excluded_free_slot_is_not_reported_as_a_full_period() {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let metadata = test_fixtures::people();
	// Only seq 9 is free, and the caller already excluded it.
	let mut entries: Vec<String> = (0..SLOTS - 1).map(|_| slot_entry([0x99; 32])).collect();
	entries.push("null".to_string());
	let scripted = ScriptedRpc::new(entries.iter().map(String::as_str).collect::<Vec<_>>());
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let selection = futures::executor::block_on(scan_slot_excluding(
		&rpc,
		metadata,
		SlotScan {
			collection: PersonhoodCollection::LitePeople,
			signer: &signer,
			network_suffix: NETWORK_SUFFIX,
			period: 7,
			target: &[0x22; 32],
			excluded: &[u32::try_from(SLOTS - 1).expect("slot index fits in u32")],
			reuse_existing: true,
		},
	))
	.unwrap();

	assert_eq!(selection, SlotSelection::FreeSlotsExcluded);
}

/// The scan probes a batch per round trip, not a key per round trip, and still
/// returns the first free slot in order. One round trip per slot cost seconds
/// against a live chain when the early slots were taken.
#[test]
fn the_pgas_scan_reads_a_batch_per_round_trip() {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	const DAY: u32 = 20678;

	// Slots 0-2 are claimed; 3 is free. The batch of ten is answered key by key.
	let answers: Vec<&str> =
		(0..10).map(|slot| if slot < 3 { r#""0x""# } else { "null" }).collect();
	let scripted = ScriptedRpc::new(answers);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let chosen = futures::executor::block_on(scan_pgas_slot_in(
		&rpc,
		&signer,
		PersonhoodCollection::LitePeople,
		NETWORK_SUFFIX,
		DAY,
		40,
		&[],
	))
	.unwrap();

	assert_eq!(chosen, 3, "the first free slot, in order");
	let calls = scripted.calls();
	assert_eq!(calls.len(), 1, "one round trip covered the whole batch");
	assert_eq!(calls[0].0, "state_queryStorageAt");
}

/// Inclusion is not success: the pallet marks the alias spent only when the
/// claim dispatches cleanly, so its absence at the included block is how a
/// dispatch error is caught.
#[test]
fn a_claim_is_only_recorded_when_the_alias_is_spent() {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let alias =
		pgas_alias(&signer, PersonhoodCollection::LitePeople, NETWORK_SUFFIX, DAY, 0).unwrap();
	const DAY: u32 = 20678;

	let spent = ScriptedRpc::new(vec![r#""0x""#]);
	let absent = ScriptedRpc::new(vec!["null"]);

	assert!(
		futures::executor::block_on(pgas_slot_is_claimed_at(
			&RpcClient::new(HostRpcClient::new(spent)),
			&alias,
			DAY,
			"0xb10c"
		))
		.unwrap()
	);
	assert!(
		!futures::executor::block_on(pgas_slot_is_claimed_at(
			&RpcClient::new(HostRpcClient::new(absent)),
			&alias,
			DAY,
			"0xb10c"
		))
		.unwrap()
	);
}

#[test]
fn pgas_context_matches_mobile_clients_and_runtime() {
	let expected: [u8; 32] =
		hex::decode("e47ba2c7eae3b97beabaeef8df599afd53e44ba9c2b851cd80850d3ed95a685b")
			.unwrap()
			.try_into()
			.unwrap();

	assert_eq!(derive_pgas_context(NETWORK_SUFFIX, 100, 3), expected);
}

/// `ClaimedGasAliases` is `Identity(u32be day) ‖ Blake2_128Concat(alias)`.
#[test]
fn claimed_gas_alias_key_layout() {
	let alias = [0x42; 32];
	let key = claimed_gas_alias_key(0x0102_0304, &alias);

	assert_eq!(key.len(), 16 + 16 + 4 + 16 + 32);
	assert_eq!(&key[32..36], &[0x01, 0x02, 0x03, 0x04], "day is big-endian");
	assert_eq!(&key[52..], &alias, "alias follows its blake2_128 prefix");
}

/// The nonzero vector pins field order and endianness. The second separately
/// pins the period-zero, sequence-zero reference vector.
#[test]
fn statement_slot_context_matches_mobile_clients_and_runtime() {
	let vector = |hex: &str| -> [u8; 32] { hex::decode(hex).unwrap().try_into().unwrap() };

	assert_eq!(
		(derive_slot_context(NETWORK_SUFFIX, 100, 3), derive_slot_context(NETWORK_SUFFIX, 0, 0),),
		(
			vector("b6c21225dcf4c2aeeca32b6db1fc93b6942ca0e8ff5c3cb1b2c5d8f0b4647ee3"),
			vector("deee1c90cf0d31093d318ac6629b4c4ab08650d4a4164511cc2496205f20f067"),
		),
	);
}

#[test]
fn product_contexts_are_scoped_to_the_network() {
	assert_eq!(
		[
			derive_slot_context(b"paseo", 100, 3) != derive_slot_context(b"polkadot", 100, 3),
			derive_long_term_storage_context(b"paseo", 100, 3) !=
				derive_long_term_storage_context(b"polkadot", 100, 3),
			derive_pgas_context(b"paseo", 100, 3) != derive_pgas_context(b"polkadot", 100, 3),
		],
		[true; 3],
	);
}

#[test]
fn network_suffix_is_read_from_chain_storage() {
	let scripted = ScriptedRpc::new(vec![r#""0x14706173656f""#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));
	let suffix = futures::executor::block_on(read_network_suffix(&rpc)).unwrap();

	assert_eq!(
		(suffix, scripted.calls()),
		(
			b"paseo".to_vec(),
			vec![(
				"state_getStorage".to_string(),
				r#"["0x63c2d4c355d2d188c234d0a0669dc90863c2d4c355d2d188c234d0a0669dc908"]"#
					.to_string(),
			)],
		),
	);
}

#[test]
fn missing_network_suffix_is_rejected() {
	let scripted = ScriptedRpc::new(vec!["null"]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	assert!(matches!(
		futures::executor::block_on(read_network_suffix(&rpc)),
		Err(StatementAllowanceError::Slot(SlotError::MissingNetworkSuffix)),
	));
}

#[test]
fn malformed_network_suffix_is_rejected() {
	let malformed = ScriptedRpc::new(vec![r#""0x14""#]);
	let malformed_rpc = RpcClient::new(HostRpcClient::new(malformed));

	assert!(matches!(
		futures::executor::block_on(read_network_suffix(&malformed_rpc)),
		Err(StatementAllowanceError::Slot(SlotError::NetworkSuffixDecode(_))),
	));
}

#[test]
fn empty_network_suffix_is_rejected() {
	let empty = ScriptedRpc::new(vec![r#""0x00""#]);
	let empty_rpc = RpcClient::new(HostRpcClient::new(empty));

	assert!(matches!(
		futures::executor::block_on(read_network_suffix(&empty_rpc)),
		Err(StatementAllowanceError::Slot(SlotError::InvalidNetworkSuffixLength { len: 0 })),
	));
}

#[test]
fn oversized_network_suffix_is_rejected() {
	let oversized_response = format!(r#""0x{}""#, hex::encode(vec![0x44; 18]));
	let oversized = ScriptedRpc::new([oversized_response.as_str()]);
	let oversized_rpc = RpcClient::new(HostRpcClient::new(oversized));

	assert!(matches!(
		futures::executor::block_on(read_network_suffix(&oversized_rpc)),
		Err(StatementAllowanceError::Slot(SlotError::InvalidNetworkSuffixLength { len: 17 })),
	));
}

#[test]
fn long_term_storage_context_matches_mobile_clients_and_runtime() {
	let expected: [u8; 32] =
		hex::decode("1b3fbe4dd813ea1e349878c9228c6823db8345207690ca4df656acb7fee81bd1")
			.unwrap()
			.try_into()
			.unwrap();

	assert_eq!(derive_long_term_storage_context(NETWORK_SUFFIX, 100, 3), expected,);
}

#[test]
fn long_term_storage_scan_uses_the_requested_network_suffix() {
	let candidates = lite_people([0x11; 32]);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	const PERIOD: u32 = 7;
	const SUFFIX: &[u8] = b"previewnet";

	let metadata = test_fixtures::people();
	let scripted = ScriptedRpc::new([r#""0x""#, "null"]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let counter = futures::executor::block_on(scan_long_term_storage_counter_excluding(
		&rpc,
		metadata,
		&signer,
		PersonhoodCollection::LitePeople,
		SUFFIX,
		PERIOD,
		&[],
	))
	.unwrap();
	let calls = (0..=1)
		.map(|counter| {
			let alias = long_term_storage_alias(
				&signer,
				PersonhoodCollection::LitePeople,
				SUFFIX,
				PERIOD,
				counter,
			)
			.unwrap();
			(
				"state_getStorage".to_string(),
				format!(
					r#"["0x{}"]"#,
					hex::encode(spent_long_term_storage_alias_key(PERIOD, &alias))
				),
			)
		})
		.collect::<Vec<_>>();

	assert_eq!((counter, scripted.calls()), (1, calls));
}

#[test]
fn period_is_utc_day_index() {
	assert_eq!(current_period(86_400 * 20_000 + 5), 20_000);
}

#[test]
fn long_term_storage_period_uses_chain_duration() {
	assert_eq!(current_long_term_storage_period(1_209_600 * 20 + 5, 1_209_600).unwrap(), 20,);
}

/// Pins `reports_exhausted_period` against the renderings above: rewording
/// one of these variants fails here, in the file it was reworded in, rather
/// than silently turning the signing host's account rotation into a retry
/// loop. The reason it reads is the registration error wrapped in context,
/// so the match has to survive both the wrapping and the casing.
#[test]
fn an_exhausted_period_is_reported_whatever_wraps_it() {
	use crate::runtime::login_failure::reports_exhausted_period;

	let error = SlotError::NoFreeStatementStoreSlot { period: 7, max: 10 };

	assert!(reports_exhausted_period(&error.to_string()));
	assert!(reports_exhausted_period(&format!(
		"allowance registration for device failed: {error}"
	)));
	assert!(reports_exhausted_period(
		&SlotError::NoFreeLongTermStorageSlot { period: 7, max: 4 }.to_string()
	));
	assert!(!reports_exhausted_period(
		&SlotError::FreeSlotsAwaitingSubmission { period: 7 }.to_string()
	));
}
/// The runtime's `notification_context(reference)` hashes
/// `"sys/" ‖ u32_le(1) ‖ u32_le(period) ‖ seq ‖ zero padding` under the
/// network's product prefix. A wrong family or width would prove a slot the
/// runtime never checks.
#[test]
fn notification_context_follows_the_runtime_layout() {
	let mut suffix = [0u8; 32];
	suffix[..4].copy_from_slice(b"sys/");
	suffix[4..8].copy_from_slice(&1u32.to_le_bytes());
	suffix[8..12].copy_from_slice(&20_756u32.to_le_bytes());
	suffix[12] = 7;
	let expected = blake2_256(&[b"product/peopl.paseo/".as_slice(), &suffix].concat());

	assert_eq!(derive_notification_context(b"paseo", 20_756, 7), expected);
}
