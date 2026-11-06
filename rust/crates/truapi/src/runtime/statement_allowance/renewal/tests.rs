use super::*;

/// The failure a slot-exhausted registration produces.
fn exhausted_failure() -> RenewalFailure {
	StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot { period: 7, max: 8 }).into()
}

/// A two-target pass against a full period must place the second target in a
/// different slot from the first. If claimed slots were not protected, the
/// second target would take the slot the first just got, and a pass with more
/// targets than slots would never settle.
#[test]
fn a_pass_does_not_take_back_the_slot_it_just_claimed() {
	use parity_scale_codec::Encode;
	use subxt_rpcs::RpcClient as HostRpcClient;

	use crate::runtime::statement_allowance::{
		FixedPersonhoodSigner, proof,
		ring::RingParams,
		rpc::{RpcClient, testing::ScriptedRpc},
		slot::{StatementStoreAllowanceEntry, testing::lite_people},
		test_fixtures,
	};

	const NOW: u64 = 10_000_000;

	/// One occupied slot entry, oldest first by `seq`.
	fn entry(account: [u8; 32], since: u64) -> String {
		let entry = StatementStoreAllowanceEntry { account_id: account, seq: 0, since }.encode();
		format!(r#""0x{}""#, hex::encode(entry))
	}
	fn clock() -> String {
		format!(r#""0x{}""#, hex::encode((NOW * 1_000).encode()))
	}

	let metadata = test_fixtures::people();
	let extrinsics = super::super::test_fixtures::people_extrinsics();
	let target_chain = super::super::test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	// One collection, so this stays a test of cross-target protection rather
	// than of pooling; pooling has its own tests.
	let memberships = [RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	}];
	let targets = [target_with("first", [0xa1; 32]), target_with("second", [0xa2; 32])];

	// Every target's slot table is scanned before registration begins. Each
	// missing target is then rescanned under the registration lock.
	let mut owned = Vec::new();
	for _ in &targets {
		owned.extend((0..10u64).map(|seq| entry([0x99; 32], 1_000 + seq)));
	}
	owned.extend((0..10u64).map(|seq| entry([0x99; 32], 1_000 + seq)));
	owned.push(clock());
	owned.push("null".to_string());
	owned.push(entry(targets[0].account_id, NOW));
	owned.push(entry(targets[0].account_id, NOW));
	owned.extend((1..10u64).map(|seq| entry([0x99; 32], 1_000 + seq)));
	owned.push(clock());
	owned.push("null".to_string());
	owned.push(entry(targets[1].account_id, NOW));
	let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	scripted.script_subscription([r#"{"inBlock":"0xb10d"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let context = RenewalChainContext {
		rpc: &rpc,
		metadata,
		target_chain,
		network_suffix: b"paseo",
		signer: &signer,
		collections: &[PersonhoodCollection::LitePeople],
		memberships: &memberships,
	};
	let lock = Mutex::new(());
	let report = futures::executor::block_on(renew_targets(&context, 7, &targets, &lock));

	let seqs: Vec<u32> = report
		.outcomes
		.iter()
		.filter_map(|outcome| match outcome.status {
			TargetRenewalStatus::Registered { seq, .. } |
			TargetRenewalStatus::AlreadyAllocated { seq } => Some(seq),
			_ => None,
		})
		.collect();
	assert_eq!(
		seqs,
		vec![0, 1],
		"the second target must not reclaim the first target's slot: {:?}",
		report.outcomes
	);
}

#[test]
fn a_pass_rescans_before_claiming_fresh_slots() {
	use parity_scale_codec::Encode;
	use subxt_rpcs::RpcClient as HostRpcClient;

	use crate::runtime::statement_allowance::{
		FixedPersonhoodSigner, proof,
		ring::RingParams,
		rpc::{RpcClient, testing::ScriptedRpc},
		slot::{StatementStoreAllowanceEntry, testing::lite_people},
		test_fixtures,
	};

	const NOW: u64 = 10_000_000;

	fn entry(account: [u8; 32]) -> String {
		let entry =
			StatementStoreAllowanceEntry { account_id: account, seq: 0, since: NOW }.encode();
		format!(r#""0x{}""#, hex::encode(entry))
	}

	let metadata = test_fixtures::people();
	let extrinsics = super::super::test_fixtures::people_extrinsics();
	let target_chain = super::super::test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let memberships = [RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	}];
	let targets = [target_with("first", [0xa1; 32]), target_with("second", [0xa2; 32])];

	let mut responses = vec!["null".to_string(); 20];
	responses.extend(vec!["null".to_string(); 10]);
	responses.push("null".to_string());
	responses.push(entry(targets[0].account_id));
	responses.push(entry(targets[0].account_id));
	responses.extend(vec!["null".to_string(); 9]);
	responses.push("null".to_string());
	responses.push(entry(targets[1].account_id));
	let scripted = ScriptedRpc::new(responses.iter().map(String::as_str).collect::<Vec<_>>());
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	scripted.script_subscription([r#"{"inBlock":"0xb10d"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let context = RenewalChainContext {
		rpc: &rpc,
		metadata,
		target_chain,
		network_suffix: b"paseo",
		signer: &signer,
		collections: &[PersonhoodCollection::LitePeople],
		memberships: &memberships,
	};
	let report = futures::executor::block_on(renew_targets(&context, 7, &targets, &Mutex::new(())));

	assert_eq!(
		report,
		StatementRenewalReport {
			period: 7,
			outcomes: vec![
				StatementRenewalOutcome {
					label: "first".to_string(),
					status: TargetRenewalStatus::Registered {
						seq: 0,
						block_hash: "0xb10c".to_string(),
					},
				},
				StatementRenewalOutcome {
					label: "second".to_string(),
					status: TargetRenewalStatus::Registered {
						seq: 1,
						block_hash: "0xb10d".to_string(),
					},
				},
			],
			pruned: Vec::new(),
			slots_exhausted: false,
		}
	);
}

#[test]
fn a_new_target_cannot_replace_an_existing_later_target() {
	use parity_scale_codec::Encode;
	use subxt_rpcs::RpcClient as HostRpcClient;

	use crate::runtime::statement_allowance::{
		FixedPersonhoodSigner, proof,
		ring::RingParams,
		rpc::{RpcClient, testing::ScriptedRpc},
		slot::{StatementStoreAllowanceEntry, testing::lite_people},
		test_fixtures,
	};

	const NOW: u64 = 10_000_000;

	fn entry(account: [u8; 32], since: u64) -> String {
		let entry = StatementStoreAllowanceEntry { account_id: account, seq: 0, since }.encode();
		format!(r#""0x{}""#, hex::encode(entry))
	}
	fn clock() -> String {
		format!(r#""0x{}""#, hex::encode((NOW * 1_000).encode()))
	}

	let metadata = test_fixtures::people();
	let extrinsics = super::super::test_fixtures::people_extrinsics();
	let target_chain = super::super::test_fixtures::people_target(&extrinsics);
	let entropy = [0x11; 32];
	let memberships = [RingParams {
		collection: PersonhoodCollection::LitePeople,
		members: vec![proof::member_key_now(entropy)],
		exponent: 9,
		ring_index: 0,
		block_hash: "0xfinal".to_string(),
	}];
	let new_target = target_with("new", [0xa1; 32]);
	let existing_target = target_with("existing", [0xa2; 32]);
	let targets = [new_target.clone(), existing_target.clone()];

	// The full table, read by each target's pre-scan and by the rescan.
	let mut table = vec![entry(existing_target.account_id, 1_000)];
	table.extend((1..10u64).map(|seq| entry([0x99; 32], 1_000 + seq)));
	let mut owned = [table.clone(), table.clone(), table].concat();
	owned.push(clock());
	owned.push("null".to_string());
	owned.push(entry(new_target.account_id, NOW));
	let responses: Vec<&str> = owned.iter().map(String::as_str).collect();
	let scripted = ScriptedRpc::new(responses);
	scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let context = RenewalChainContext {
		rpc: &rpc,
		metadata,
		target_chain,
		network_suffix: b"paseo",
		signer: &signer,
		collections: &[PersonhoodCollection::LitePeople],
		memberships: &memberships,
	};
	let lock = Mutex::new(());
	let report = futures::executor::block_on(renew_targets(&context, 7, &targets, &lock));

	let seqs: Vec<u32> = report
		.outcomes
		.iter()
		.filter_map(|outcome| match outcome.status {
			TargetRenewalStatus::Registered { seq, .. } |
			TargetRenewalStatus::AlreadyAllocated { seq } => Some(seq),
			_ => None,
		})
		.collect();
	assert_eq!(
		seqs,
		vec![1, 0],
		"the new target must not replace the later target's existing slot: {:?}",
		report.outcomes
	);
}

fn target_with(label: &str, account_id: [u8; 32]) -> ResolvedRenewalTarget {
	ResolvedRenewalTarget { label: label.to_string(), account_id }
}

#[test]
fn only_a_missing_slot_counts_as_exhaustion() {
	assert!(exhausted_failure().slots_exhausted);

	let other: RenewalFailure = StatementAllowanceError::BulletinAuthorizationTimeout.into();
	assert!(!other.slots_exhausted);
}

fn target(label: &str) -> ResolvedRenewalTarget {
	ResolvedRenewalTarget { label: label.to_string(), account_id: [0u8; 32] }
}

fn outcome(label: &str, status: TargetRenewalStatus) -> StatementRenewalOutcome {
	StatementRenewalOutcome { label: label.to_string(), status }
}

#[test]
fn tick_delay_caps_at_one_hour_mid_day() {
	let mid_day = 86_400 * 20_000 + 43_200;
	assert_eq!(next_tick_delay(mid_day), Duration::from_secs(3_600));
}

#[test]
fn tick_delay_lands_after_the_period_boundary() {
	let just_before_boundary = 86_400 * 20_001 - 10;
	assert_eq!(next_tick_delay(just_before_boundary), Duration::from_secs(10 + 120));
}

#[test]
fn tick_delay_never_lands_inside_the_post_boundary_margin() {
	let boundary = 86_400 * 20_001;
	// Every start in the last two hours before a boundary.
	for offset in 1..=7_200 {
		let now = boundary - offset;
		let landing = now + next_tick_delay(now).as_secs();
		assert!(
			landing < boundary || landing >= boundary + PERIOD_BOUNDARY_MARGIN.as_secs(),
			"tick from {now} lands at {landing}, inside the margin after {boundary}"
		);
	}
}

#[test]
fn tick_delay_waits_for_the_boundary_once_it_is_within_an_hour() {
	let boundary = 86_400 * 20_001;
	// Exactly one hour out, the cap used to land the tick on the boundary.
	assert_eq!(
		next_tick_delay(boundary - 3_600),
		Duration::from_secs(3_600) + PERIOD_BOUNDARY_MARGIN
	);
}

#[test]
fn tick_delay_at_boundary_reverts_to_hourly() {
	assert_eq!(next_tick_delay(86_400 * 20_001), Duration::from_secs(3_600));
}

#[test]
fn mid_list_failure_does_not_stop_the_pass() {
	let targets = [target("a"), target("b"), target("c")];
	let report = fold_outcomes(
		7,
		&targets,
		vec![
			Ok(RegistrationOutcome::AlreadyAllocated {
				seq: 1,
				collection: PersonhoodCollection::LitePeople,
			}),
			Err(RenewalFailure { reason: "rpc timeout".to_string(), slots_exhausted: false }),
			Ok(RegistrationOutcome::Registered {
				block_hash: "0xabc".to_string(),
				seq: 2,
				ring_index: 0,
				collection: PersonhoodCollection::LitePeople,
			}),
		],
	);
	assert_eq!(
		report,
		StatementRenewalReport {
			period: 7,
			outcomes: vec![
				outcome("a", TargetRenewalStatus::AlreadyAllocated { seq: 1 }),
				outcome("b", TargetRenewalStatus::Failed { reason: "rpc timeout".to_string() }),
				outcome(
					"c",
					TargetRenewalStatus::Registered { seq: 2, block_hash: "0xabc".to_string() }
				),
			],
			pruned: Vec::new(),
			slots_exhausted: false,
		}
	);
}

#[test]
fn exhaustion_skips_remaining_targets() {
	let targets = [target("a"), target("b"), target("c")];
	let report = fold_outcomes(
		7,
		&targets,
		vec![
			Ok(RegistrationOutcome::AlreadyAllocated {
				seq: 0,
				collection: PersonhoodCollection::LitePeople,
			}),
			Err(exhausted_failure()),
		],
	);
	assert_eq!(
		report,
		StatementRenewalReport {
			period: 7,
			outcomes: vec![
				outcome("a", TargetRenewalStatus::AlreadyAllocated { seq: 0 }),
				outcome("b", TargetRenewalStatus::Failed { reason: exhausted_failure().reason }),
				outcome("c", TargetRenewalStatus::SkippedExhausted),
			],
			pruned: Vec::new(),
			slots_exhausted: true,
		}
	);
}
