//! [`PgasClaimPolicy`]: builds each claim when the engine submits it, in a
//! slot no other claim of this process has consumed.

mod recent_slots;

use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use parking_lot::Mutex;

use self::recent_slots::RecentSlots;
use super::{
	builder::{ClaimBuildError, PgasClaimBuilder},
	claim_extrinsic::{BuiltClaim, ClaimSlot},
	params::ClaimParams,
};
use crate::{
	durable::{
		DeferredDurableTx, DurableTxEntry, DurableTxId, PolicyError, PolicyParams, Preparation,
		SubmissionPolicy, VerdictFailureKind,
	},
	unix_time::current_unix_secs,
};

/// Attempts after which a claim is given up, so a refusal that repeats
/// cannot loop forever.
const MAX_ATTEMPTS: u32 = 5;

/// Builds PGAS claims: one fee-less `Pgas.claim_pgas` per claim, rebuilt with
/// a fresh slot after a refused or expired attempt.
pub struct PgasClaimPolicy {
	builder: Arc<dyn PgasClaimBuilder>,
	recent: Mutex<RecentSlots>,
}

/// One deferred claim with everything needed to build it.
struct PlannedClaim {
	id: DurableTxId,
	attempts: u32,
	params: ClaimParams,
}

/// What one claim of a batch came to.
enum PreparedClaim {
	/// Built and ready to submit. Its slot is consumed.
	Success(BuiltClaim),
	/// Never to be built.
	GiveUp,
	/// Not built for now. The engine asks about it again, until its deadline.
	Retry,
}

impl PreparedClaim {
	/// The engine's answer for the claim, none when it is retried later.
	fn into_answer(self) -> Option<Preparation> {
		match self {
			Self::Success(built) => Some(Preparation::Ready { extrinsic: built.extrinsic, params: None }),
			Self::GiveUp => Some(Preparation::GiveUp),
			Self::Retry => None,
		}
	}
}

impl PgasClaimPolicy {
	/// A policy building claims through `builder`.
	pub fn new(builder: Arc<dyn PgasClaimBuilder>) -> Self {
		Self { builder, recent: Mutex::default() }
	}

	/// What `claim` comes to at unix time `now`, built in a slot `excluded`
	/// does not list.
	async fn prepare_claim(
		&self,
		claim: &PlannedClaim,
		excluded: &BTreeSet<ClaimSlot>,
		now: u64,
	) -> PreparedClaim {
		if claim.attempts >= MAX_ATTEMPTS || now >= claim.params.give_up_after {
			return PreparedClaim::GiveUp;
		}
		match self.builder.build_claim(claim.params.target, excluded).await {
			Ok(built) => PreparedClaim::Success(built),
			Err(error) => unbuilt_claim(claim, &error),
		}
	}

	/// Consumes `slot` at unix time `now`, so no later claim of this batch or
	/// of the batches that follow is built in it.
	fn consume_slot(&self, excluded: &mut BTreeSet<ClaimSlot>, slot: ClaimSlot, now: u64) {
		excluded.insert(slot);
		self.recent.lock().consume(slot, now);
	}
}

#[async_trait::async_trait]
impl SubmissionPolicy for PgasClaimPolicy {
	/// A refused or expired attempt is built again in a fresh slot. A failed
	/// dispatch is final: the pallet refused the claim itself.
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, failure: VerdictFailureKind) -> bool {
		matches!(failure, VerdictFailureKind::Rejected | VerdictFailureKind::Expired)
	}

	/// Builds the claims one after another, so each one's slot is consumed
	/// before the next is built.
	async fn prepare(
		&self,
		batch: &[DeferredDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let now = current_unix_secs();
		let mut excluded = self.recent.lock().consumed_slots(now);
		let mut answers = HashMap::new();
		for claim in planned_claims(batch) {
			let prepared = self.prepare_claim(&claim, &excluded, now).await;
			if let PreparedClaim::Success(built) = &prepared {
				self.consume_slot(&mut excluded, built.slot, now);
			}
			if let Some(answer) = prepared.into_answer() {
				answers.insert(claim.id, answer);
			}
		}
		Ok(answers)
	}
}

/// What `claim` comes to when it could not be built for `error`: a lost
/// session or a transient failure is retried, which ends at the claim's
/// deadline, and anything else gives up at once.
fn unbuilt_claim(claim: &PlannedClaim, error: &ClaimBuildError) -> PreparedClaim {
	tracing::warn!(id = claim.id.0, %error, "PGAS claim not built");
	match error {
		ClaimBuildError::NoSession | ClaimBuildError::Transient(_) => PreparedClaim::Retry,
		ClaimBuildError::NoPersonhood | ClaimBuildError::NoFreeSlot { .. } => PreparedClaim::GiveUp,
	}
}

/// The claims of `batch` whose params decode. Only registration writes
/// these params, so a row that does not decode is left out rather than
/// answered.
fn planned_claims(batch: &[DeferredDurableTx]) -> Vec<PlannedClaim> {
	batch.iter().filter_map(planned_claim).collect()
}

/// `deferred` as a claim, or `None` when its params do not decode.
fn planned_claim(deferred: &DeferredDurableTx) -> Option<PlannedClaim> {
	ClaimParams::try_from(&deferred.plan.params).ok().map(|params| PlannedClaim {
		id: deferred.id,
		attempts: deferred.attempts,
		params,
	})
}

#[cfg(test)]
mod tests;
