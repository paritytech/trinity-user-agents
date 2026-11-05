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
//! cannot free a slot, only time or a replacement can. A host driving the loop
//! instead reads the same report from `last_statement_renewal_report`, since the
//! loop has no caller to return one to. The host READMEs under `ios/truapi-host`
//! and `android/truapi-host` carry the platform-specific form of this.

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
	/// Not attempted: the host ran out of slots earlier in the pass.
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

/// Register every target for `period`, continuing past per-target failures
/// and stopping early once the host's slots for the period are exhausted
/// (remaining targets are reported as skipped).
///
/// Slots already held by any target, plus slots claimed earlier in the pass,
/// are protected from replacement. Without that, a new target early in the
/// ledger can evict a paired device that appears later, or targets can take
/// slots back from each other within one pass. Protecting them turns that into
/// an exhaustion report, which is the honest answer when the ledger wants more
/// slots than exist.
///
/// `registration_lock` is held for one target operation at a time, not for the
/// whole pass. A target without an existing slot is rescanned and registered
/// under one guard so no other allocation can claim its cached free slot.
pub async fn renew_targets(
	context: &RenewalChainContext<'_>,
	period: u32,
	targets: &[ResolvedRenewalTarget],
	registration_lock: &Mutex<()>,
) -> StatementRenewalReport {
	let mut target_scans = Vec::with_capacity(targets.len());
	for target in targets {
		let scans = {
			let _guard = registration_lock.lock().await;
			scan_collections(
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
			.await
			.map_err(RenewalFailure::from)
		};
		target_scans.push(scans);
	}

	let mut claimed: Vec<(PersonhoodCollection, u32)> = target_scans
		.iter()
		.filter_map(|scans| scans.as_ref().ok().and_then(|scans| allocated_in(scans)))
		.collect();
	let mut results = Vec::with_capacity(targets.len());
	for (target, scans) in targets.iter().zip(target_scans) {
		let scans = match scans {
			Ok(scans) => scans,
			Err(failure) => {
				results.push(Err(failure));
				continue;
			},
		};
		let result = {
			let _guard = registration_lock.lock().await;
			let scans = if allocated_in(&scans).is_some() {
				Ok(scans)
			} else {
				scan_collections(
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
				.await
				.map_err(RenewalFailure::from)
			};
			match scans {
				Ok(scans) => {
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
							// Renewal exists to keep the ledger's targets alive across a
							// period boundary, so it may reclaim space when full.
							allow_eviction: true,
							protected: &claimed,
						},
					)
					.await
					.map_err(RenewalFailure::from)
				},
				Err(failure) => Err(failure),
			}
		};
		log_target_result(period, &target.label, &result);
		if let Ok(outcome) = &result {
			match outcome {
				RegistrationOutcome::Registered { seq, collection, .. } |
				RegistrationOutcome::AlreadyAllocated { seq, collection } => {
					let claimed_slot = (*collection, *seq);
					if !claimed.contains(&claimed_slot) {
						claimed.push(claimed_slot);
					}
				},
			}
		}
		let exhausted = matches!(&result, Err(failure) if failure.slots_exhausted);
		results.push(result);
		if exhausted {
			break;
		}
	}
	fold_outcomes(period, targets, results)
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
	result: &Result<RegistrationOutcome, RenewalFailure>,
) {
	match result {
		Ok(RegistrationOutcome::Registered { block_hash, seq, .. }) => {
			info!(period, label, seq, %block_hash, "renewed statement-store allowance")
		},
		Ok(RegistrationOutcome::AlreadyAllocated { seq, .. }) => {
			debug!(period, label, seq, "statement-store allowance already fresh");
		},
		Err(failure) => {
			warn!(period, label, reason = %failure.reason, "statement-store renewal failed");
		},
	}
}

/// Pair each target with its registration result; targets past the end of
/// `results` were never attempted (the pass stopped on slot exhaustion).
fn fold_outcomes(
	period: u32,
	targets: &[ResolvedRenewalTarget],
	results: Vec<Result<RegistrationOutcome, RenewalFailure>>,
) -> StatementRenewalReport {
	let mut slots_exhausted = false;
	let mut results = results.into_iter();
	let outcomes = targets
		.iter()
		.map(|target| {
			let status = match results.next() {
				Some(Ok(RegistrationOutcome::Registered { block_hash, seq, .. })) => {
					TargetRenewalStatus::Registered { seq, block_hash }
				},
				Some(Ok(RegistrationOutcome::AlreadyAllocated { seq, .. })) => {
					TargetRenewalStatus::AlreadyAllocated { seq }
				},
				Some(Err(failure)) => {
					slots_exhausted |= failure.slots_exhausted;
					TargetRenewalStatus::Failed { reason: failure.reason }
				},
				None => TargetRenewalStatus::SkippedExhausted,
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
mod tests;
