//! Proactive renewal of statement-store allowances across period boundaries.
//!
//! Allowances are claimed per UTC-day period and stop being renewed at the
//! boundary, so a long-lived host must re-register every account it promised to
//! keep allowed (RFC-0010 assigns renewal to the Account Holder). They are not
//! revoked the instant the period ends: `Resources.StmtStoreGraceWindow` keeps
//! an ended period's allowances active until cleanup catches up, 172800 seconds
//! on `paseo-next-v2` as of 2026-08-15. This module is the
//! chain-pure pass: given already-resolved targets, register each for the
//! requested period. Scheduling and target persistence live in
//! `signing_host::wallet_account_holder::allowance_renewal`.
//!
//! A host owns the schedule. The core answers when the next pass is due and what
//! a pass achieved; it never asks to be woken, because the period arithmetic a
//! host would need is derivable from the clock and the grace window leaves days
//! of slack. Three layers cover it, and only the first needs the operating
//! system: a scheduled wake for an app nobody opens, a pass on session
//! activation for an app somebody does, and on-demand allocation, which covers
//! the account of a product asking for an allowance but not the rest of the
//! ledger.
//!
//! A caller reading [`StatementRenewalReport`] to answer an OS scheduler should
//! treat every `Registered` or `AlreadyAllocated` as success, a `Failed` as worth
//! retrying on the next opportunistic wake, and exhaustion as success: retrying
//! cannot free a slot, only time can. Exhaustion wins over `Failed`, since the
//! target that found the period full is reported `Failed` too. A host driving
//! the loop instead reads the same report from `last_statement_renewal_report`,
//! since the loop has no caller to return one to. The host READMEs under
//! `ios/truapi-host` and `android/truapi-host` carry the platform-specific form
//! of this.

use std::time::Duration;

use futures::lock::Mutex;
use tracing::{debug, info, warn};

use super::{
	CollectionScanParams, PersonhoodSigner, PooledRegistrationParams, RegistrationOutcome,
	StatementAllowanceError, allocated_in,
	collection::PersonhoodCollection,
	extension::Metadata,
	register_statement_account_pooled,
	rpc::RpcClient,
	scan_collections,
	slot::{STATEMENT_STORE_PERIOD_SECONDS, SlotError},
};

/// Cap between renewal ticks for the in-process loop.
///
/// A retry rhythm, not a deadline. An allowance stays usable for
/// `Resources.StmtStoreGraceWindow` past its period boundary, which is 48 hours
/// on `paseo-next-v2`, so a pass has ample slack and this only decides how
/// promptly a transient failure is retried. A host scheduling its own wake-ups
/// does not need this cadence; one pass per period is enough.
const MAX_TICK_INTERVAL: Duration = Duration::from_secs(3_600);
/// Margin after a period boundary before the boundary tick fires, so the
/// chain has rotated to the new period by the time we scan slots.
const PERIOD_BOUNDARY_MARGIN: Duration = Duration::from_secs(120);

/// Why one target's renewal failed, and whether the host had no slot left.
///
/// The distinction drives the rest of the pass, so it is read off the typed
/// error rather than its rendered text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RenewalFailure {
	reason: String,
	slots_exhausted: bool,
}

impl From<StatementAllowanceError> for RenewalFailure {
	fn from(err: StatementAllowanceError) -> Self {
		Self {
			slots_exhausted: matches!(
				err,
				StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot { .. })
			),
			reason: err.to_string(),
		}
	}
}

/// One resolved renewal target: account id plus a label for reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRenewalTarget {
	/// Human-readable name used in logs and reports.
	pub label: String,
	/// Account to keep allowed.
	pub account_id: [u8; 32],
}

/// Outcome of renewing one target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum TargetRenewalStatus {
	/// The extrinsic reached a block; the target holds `seq` this period.
	Registered {
		/// Claimed slot sequence.
		seq: u32,
		/// Block hash the extrinsic landed in.
		block_hash: String,
	},
	/// The target already held a slot this period; nothing submitted.
	AlreadyAllocated {
		/// Existing slot sequence.
		seq: u32,
	},
	/// Registration failed; the target is retried on the next tick.
	Failed {
		/// Failure detail.
		reason: String,
	},
	/// Not registered: the period filled earlier in the pass and the target
	/// holds no slot in it.
	SkippedExhausted,
}

/// What one target's renewal produced, paired with the label that identifies it
/// in the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct StatementRenewalOutcome {
	/// Ledger label for the renewed target.
	pub label: String,
	/// What the pass did for this target.
	pub status: TargetRenewalStatus,
}

/// Summary of one renewal pass.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct StatementRenewalReport {
	/// Period the pass registered for.
	pub period: u32,
	/// Per-target outcomes in ledger order.
	pub outcomes: Vec<StatementRenewalOutcome>,
	/// Labels of targets this pass dropped because a different identity
	/// promised them.
	///
	/// Dropping is silent otherwise: a pruned target simply stops appearing in
	/// `outcomes`, and the surface has no way to list the ledger, so a host
	/// could only infer it from an absence. A raw account target does not
	/// survive a change of root entropy, so this is how a host learns to
	/// re-track one.
	pub pruned: Vec<String>,
	/// Whether the pass hit slot exhaustion for this period.
	pub slots_exhausted: bool,
}

/// Chain context shared by every registration in one renewal pass.
pub struct RenewalChainContext<'a> {
	/// People-chain RPC connection.
	pub rpc: &'a RpcClient,
	/// Decoded runtime metadata.
	pub metadata: &'a Metadata,
	/// Where each registration extrinsic is built.
	pub target_chain: super::ExtrinsicTarget<'a>,
	/// Runtime-wide suffix used for product-scoped aliases and proofs.
	pub network_suffix: &'a [u8],
	/// Every collection the host can derive aliases for, so an allowance already
	/// held in a collection whose ring cannot currently be proved is still seen.
	pub collections: &'a [PersonhoodCollection],
	/// Authorizes each synchronous alias and proof in this pass.
	pub signer: &'a dyn PersonhoodSigner,
	/// Every collection the host can prove membership in, widest budget first.
	/// Renewal pools slots across all of them, so a device with full personhood
	/// renews against the combined budget rather than one collection's share.
	pub memberships: &'a [super::ring::RingParams],
}

/// Register every target for `period`, continuing past per-target failures.
///
/// A target is only ever registered into a free slot. A slot held by any other
/// account is never replaced, so a full period is reported as exhaustion for
/// the target that found it full. Slot tables are per period, so a target never
/// needs to displace its own older registration either.
///
/// Once the period is full, later targets are only scanned: one that already
/// holds a slot reports `AlreadyAllocated`, the rest `SkippedExhausted`. A
/// caller deciding whether a target is still allowed must not read exhaustion
/// earlier in the ledger as that target losing its slot.
///
/// `registration_lock` is held for one target operation at a time, not for the
/// whole pass. Each target is scanned and registered under one guard so no
/// other allocation can claim the free slot it found.
pub async fn renew_targets(
	context: &RenewalChainContext<'_>,
	period: u32,
	targets: &[ResolvedRenewalTarget],
	registration_lock: &Mutex<()>,
) -> StatementRenewalReport {
	let mut results = Vec::with_capacity(targets.len());
	let mut exhausted = false;
	for target in targets {
		let result = if exhausted {
			held_slot(context, period, target).await
		} else {
			let _guard = registration_lock.lock().await;
			renew_target(context, period, target).await.map(Some)
		}
		.map_err(RenewalFailure::from);
		log_target_result(period, &target.label, &result);
		exhausted |= matches!(&result, Err(failure) if failure.slots_exhausted);
		results.push(result);
	}
	fold_outcomes(period, targets, results)
}

/// The slot `target` already holds this period, if any, without registering.
async fn held_slot(
	context: &RenewalChainContext<'_>,
	period: u32,
	target: &ResolvedRenewalTarget,
) -> Result<Option<RegistrationOutcome>, StatementAllowanceError> {
	let scans = scan_collections(
		context.rpc,
		context.metadata,
		context.signer,
		context.collections,
		CollectionScanParams {
			network_suffix: context.network_suffix,
			period,
			target: &target.account_id,
			reuse_existing: true,
		},
	)
	.await?;
	Ok(allocated_in(&scans)
		.map(|(collection, seq)| RegistrationOutcome::AlreadyAllocated { seq, collection }))
}

async fn renew_target(
	context: &RenewalChainContext<'_>,
	period: u32,
	target: &ResolvedRenewalTarget,
) -> Result<RegistrationOutcome, StatementAllowanceError> {
	let scans = scan_collections(
		context.rpc,
		context.metadata,
		context.signer,
		context.collections,
		CollectionScanParams {
			network_suffix: context.network_suffix,
			period,
			target: &target.account_id,
			reuse_existing: true,
		},
	)
	.await?;
	register_statement_account_pooled(
		context.rpc,
		context.metadata,
		context.target_chain,
		context.signer,
		&scans,
		context.memberships,
		PooledRegistrationParams {
			target: &target.account_id,
			period,
			network_suffix: context.network_suffix,
			reuse_existing: true,
			allow_eviction: false,
		},
	)
	.await
}

/// Delay until the next renewal tick: hourly, but always shortly after each
/// period boundary rather than before it. The margin is about the chain's clock,
/// not urgency; see the inline note below.
pub fn next_tick_delay(now_seconds: u64) -> Duration {
	let next_boundary =
		(now_seconds / STATEMENT_STORE_PERIOD_SECONDS + 1) * STATEMENT_STORE_PERIOD_SECONDS;
	let until_boundary = next_boundary - now_seconds;
	let until_after_boundary = Duration::from_secs(until_boundary) + PERIOD_BOUNDARY_MARGIN;
	// Once the boundary is within an hour, wait for it plus the margin rather
	// than capping: a capped tick can land inside the margin, where the local
	// clock reports the new period but the chain has not rotated into it, and
	// the pass would scan slots for a period the chain does not agree on.
	if MAX_TICK_INTERVAL.as_secs() >= until_boundary {
		until_after_boundary
	} else {
		MAX_TICK_INTERVAL
	}
}

fn log_target_result(
	period: u32,
	label: &str,
	result: &Result<Option<RegistrationOutcome>, RenewalFailure>,
) {
	match result {
		Ok(Some(RegistrationOutcome::Registered { block_hash, seq, .. })) => {
			info!(period, label, seq, %block_hash, "renewed statement-store allowance")
		},
		Ok(Some(RegistrationOutcome::AlreadyAllocated { seq, .. })) => {
			debug!(period, label, seq, "statement-store allowance already fresh");
		},
		Ok(None) => {
			warn!(period, label, "statement-store renewal skipped: no free slot this period");
		},
		Err(failure) => {
			warn!(period, label, reason = %failure.reason, "statement-store renewal failed");
		},
	}
}

/// Pair each target with its result, in ledger order. `Ok(None)` is a target
/// that holds no slot and was not registered because the period was full.
fn fold_outcomes(
	period: u32,
	targets: &[ResolvedRenewalTarget],
	results: Vec<Result<Option<RegistrationOutcome>, RenewalFailure>>,
) -> StatementRenewalReport {
	let mut slots_exhausted = false;
	let outcomes = targets
		.iter()
		.zip(results)
		.map(|(target, result)| {
			let status = match result {
				Ok(Some(RegistrationOutcome::Registered { block_hash, seq, .. })) => {
					TargetRenewalStatus::Registered { seq, block_hash }
				},
				Ok(Some(RegistrationOutcome::AlreadyAllocated { seq, .. })) => {
					TargetRenewalStatus::AlreadyAllocated { seq }
				},
				Ok(None) => TargetRenewalStatus::SkippedExhausted,
				Err(failure) => {
					slots_exhausted |= failure.slots_exhausted;
					TargetRenewalStatus::Failed { reason: failure.reason }
				},
			};
			StatementRenewalOutcome { label: target.label.clone(), status }
		})
		.collect();
	StatementRenewalReport {
		period,
		outcomes,
		// fold_outcomes only sees targets that survived to be renewed; the
		// caller that read the ledger attaches what it dropped.
		pruned: Vec::new(),
		slots_exhausted,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The failure a slot-exhausted registration produces.
	fn exhausted_failure() -> RenewalFailure {
		StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot { period: 7, max: 8 })
			.into()
	}

	/// A full period holds allowances of accounts outside this host's ledger.
	/// Renewal must leave them in place and report the period as exhausted,
	/// even when one of them is past its replacement cooldown.
	#[test]
	fn renewal_never_replaces_another_accounts_slot() {
		use parity_scale_codec::Encode;
		use subxt_rpcs::RpcClient as HostRpcClient;

		use crate::runtime::statement_allowance::{
			FixedPersonhoodSigner, proof,
			ring::RingParams,
			rpc::{RpcClient, testing::ScriptedRpc},
			slot::testing::lite_people,
			test_fixtures,
		};

		const NOW: u64 = 10_000_000;

		fn entry(account: [u8; 32], since: u64) -> String {
			format!(r#""0x{}""#, hex::encode((account, 0u32, since).encode()))
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
		let targets = [target_with("device", [0xa1; 32])];

		// Every slot is taken by an account outside the ledger. Seq 0 is past its
		// replacement cooldown, so it is the slot an eviction would pick.
		let mut owned = vec![entry([0x99; 32], 1_000)];
		owned.extend((1..10u64).map(|seq| entry([0x90 + seq as u8; 32], NOW - 10)));
		// What a replacement of seq 0 would read next: the chain clock, the ring
		// revision, then the verification of the new holder. None of it is read
		// unless renewal evicts.
		owned.push(format!(r#""0x{}""#, hex::encode((NOW * 1_000).encode())));
		owned.push("null".to_string());
		owned.push(entry(targets[0].account_id, NOW));
		let scripted = ScriptedRpc::new(owned.iter().map(String::as_str).collect::<Vec<_>>());
		scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
		let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

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
		let report =
			futures::executor::block_on(renew_targets(&context, 7, &targets, &Mutex::new(())));

		let exhausted: RenewalFailure =
			StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
				period: 7,
				max: 10,
			})
			.into();
		assert_eq!(
			report,
			StatementRenewalReport {
				period: 7,
				outcomes: vec![outcome(
					"device",
					TargetRenewalStatus::Failed { reason: exhausted.reason }
				)],
				pruned: Vec::new(),
				slots_exhausted: true,
			}
		);
		assert!(
			scripted
				.calls()
				.iter()
				.all(|(method, _)| method != "author_submitAndWatchExtrinsic"),
			"nothing should have been submitted"
		);
	}

	/// Exhaustion earlier in the ledger says nothing about a later target that
	/// already holds its slot. A host that reads `SkippedExhausted` for it would
	/// fail a pairing or warn the person about an allowance that is fine.
	#[test]
	fn a_full_period_still_reports_slots_later_targets_hold() {
		use parity_scale_codec::Encode;
		use subxt_rpcs::RpcClient as HostRpcClient;

		use crate::runtime::statement_allowance::{
			FixedPersonhoodSigner, proof,
			ring::RingParams,
			rpc::{RpcClient, testing::ScriptedRpc},
			slot::testing::lite_people,
			test_fixtures,
		};

		const NOW: u64 = 10_000_000;

		fn entry(account: [u8; 32]) -> String {
			format!(r#""0x{}""#, hex::encode((account, 0u32, NOW - 10).encode()))
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
		let targets = [
			target_with("product", [0xa1; 32]),
			target_with("device", [0xa2; 32]),
			target_with("later", [0xa3; 32]),
		];

		// The device holds seq 3; every other slot belongs to an account outside
		// the ledger. Each target scans the same full table.
		let table: Vec<String> = (0..10u8)
			.map(
				|seq| {
					if seq == 3 { entry(targets[1].account_id) } else { entry([0x90 + seq; 32]) }
				},
			)
			.collect();
		let responses: Vec<&str> =
			(0..targets.len()).flat_map(|_| table.iter().map(String::as_str)).collect();
		let scripted = ScriptedRpc::new(responses);
		let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

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
		let report =
			futures::executor::block_on(renew_targets(&context, 7, &targets, &Mutex::new(())));

		let exhausted: RenewalFailure =
			StatementAllowanceError::Slot(SlotError::NoFreeStatementStoreSlot {
				period: 7,
				max: 10,
			})
			.into();
		assert_eq!(
			report,
			StatementRenewalReport {
				period: 7,
				outcomes: vec![
					outcome("product", TargetRenewalStatus::Failed { reason: exhausted.reason }),
					outcome("device", TargetRenewalStatus::AlreadyAllocated { seq: 3 }),
					outcome("later", TargetRenewalStatus::SkippedExhausted),
				],
				pruned: Vec::new(),
				slots_exhausted: true,
			}
		);
		assert!(
			scripted
				.calls()
				.iter()
				.all(|(method, _)| method != "author_submitAndWatchExtrinsic"),
			"nothing should have been submitted"
		);
	}

	/// Each target scans after the previous one's registration landed, so the
	/// second sees the first's slot taken and claims the next free one instead
	/// of submitting for a slot that is no longer free.
	#[test]
	fn consecutive_targets_claim_consecutive_free_slots() {
		use parity_scale_codec::Encode;
		use subxt_rpcs::RpcClient as HostRpcClient;

		use crate::runtime::statement_allowance::{
			FixedPersonhoodSigner, proof,
			ring::RingParams,
			rpc::{RpcClient, testing::ScriptedRpc},
			slot::testing::lite_people,
			test_fixtures,
		};

		const NOW: u64 = 10_000_000;

		fn entry(account: [u8; 32]) -> String {
			format!(r#""0x{}""#, hex::encode((account, 0u32, NOW).encode()))
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

		let mut responses = vec!["null".to_string(); 10];
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
		let report =
			futures::executor::block_on(renew_targets(&context, 7, &targets, &Mutex::new(())));

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
				Ok(Some(RegistrationOutcome::AlreadyAllocated {
					seq: 1,
					collection: PersonhoodCollection::LitePeople,
				})),
				Err(RenewalFailure { reason: "rpc timeout".to_string(), slots_exhausted: false }),
				Ok(Some(RegistrationOutcome::Registered {
					block_hash: "0xabc".to_string(),
					seq: 2,
					ring_index: 0,
					collection: PersonhoodCollection::LitePeople,
				})),
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
				Ok(Some(RegistrationOutcome::AlreadyAllocated {
					seq: 0,
					collection: PersonhoodCollection::LitePeople,
				})),
				Err(exhausted_failure()),
				Ok(None),
			],
		);
		assert_eq!(
			report,
			StatementRenewalReport {
				period: 7,
				outcomes: vec![
					outcome("a", TargetRenewalStatus::AlreadyAllocated { seq: 0 }),
					outcome(
						"b",
						TargetRenewalStatus::Failed { reason: exhausted_failure().reason }
					),
					outcome("c", TargetRenewalStatus::SkippedExhausted),
				],
				pruned: Vec::new(),
				slots_exhausted: true,
			}
		);
	}
}
