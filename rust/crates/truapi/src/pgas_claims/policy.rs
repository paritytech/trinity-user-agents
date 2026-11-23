//! [`PgasClaimPolicy`]: builds each claim when the engine submits it, in a
//! slot no other claim of this process holds, as the identity that
//! registered it.

use std::{
	collections::{BTreeSet, HashMap},
	sync::Arc,
};

use parking_lot::Mutex;

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

/// How long a slot this process built a claim in stays off limits to its
/// other claims, in seconds. Longer than the 64-block era an attempt is
/// signed with, so a slot is handed out again only once its attempt landed
/// or can no longer land.
const RECENT_SLOT_SECS: u64 = 15 * 60;

/// Builds PGAS claims: one fee-less `Pgas.claim_pgas` per claim, rebuilt with
/// a fresh slot after a refused or expired attempt.
pub struct PgasClaimPolicy {
	builder: Arc<dyn PgasClaimBuilder>,
	recent: Mutex<RecentSlots>,
}

/// The slots this process built claims in lately, with the unix time of
/// each build. Held in memory only: after a restart, a slot reused too soon
/// costs one refused attempt and a rebuild.
#[derive(Default)]
struct RecentSlots(HashMap<ClaimSlot, u64>);

/// One deferred claim with everything needed to build it.
struct PlannedClaim {
	id: DurableTxId,
	attempts: u32,
	params: ClaimParams,
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
			return PreparedClaim::answered(Some(Preparation::GiveUp));
		}
		let ClaimParams { target, identity, .. } = &claim.params;
		match self.builder.build_claim(*target, *identity, excluded).await {
			Ok(built) => PreparedClaim::built(built),
			Err(error) => PreparedClaim::answered(answer_unbuilt_claim(claim, &error)),
		}
	}

	/// Holds `slot`, built at unix time `now`, back from every later claim of
	/// this batch and of the batches that follow.
	fn hold_slot(&self, excluded: &mut BTreeSet<ClaimSlot>, slot: ClaimSlot, now: u64) {
		excluded.insert(slot);
		self.recent.lock().remember(slot, now);
	}
}

/// What one claim of a batch came to: the engine's answer, if any, and the
/// slot it was built in, if it was.
struct PreparedClaim {
	answer: Option<Preparation>,
	built_slot: Option<ClaimSlot>,
}

impl PreparedClaim {
	/// A claim answered with `answer` and built in no slot.
	fn answered(answer: Option<Preparation>) -> Self {
		Self { answer, built_slot: None }
	}

	/// A claim ready to submit as `built`.
	fn built(built: BuiltClaim) -> Self {
		let ready = Preparation::Ready { extrinsic: built.extrinsic, params: None };
		Self { answer: Some(ready), built_slot: Some(built.slot) }
	}
}

#[async_trait::async_trait]
impl SubmissionPolicy for PgasClaimPolicy {
	/// A refused or expired attempt is built again in a fresh slot. A failed
	/// dispatch is final: the pallet refused the claim itself.
	fn can_retry(&self, _: &DurableTxEntry, _: &PolicyParams, failure: VerdictFailureKind) -> bool {
		matches!(failure, VerdictFailureKind::Rejected | VerdictFailureKind::Expired)
	}

	/// Builds the claims one after another, so each one's slot is excluded
	/// from the next.
	async fn prepare(
		&self,
		batch: &[DeferredDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError> {
		let now = current_unix_secs();
		let mut excluded = self.recent.lock().held_slots(now.saturating_sub(RECENT_SLOT_SECS));
		let mut prepared = HashMap::new();
		for claim in planned_claims(batch) {
			let PreparedClaim { answer, built_slot } =
				self.prepare_claim(&claim, &excluded, now).await;
			if let Some(slot) = built_slot {
				self.hold_slot(&mut excluded, slot, now);
			}
			if let Some(answer) = answer {
				prepared.insert(claim.id, answer);
			}
		}
		Ok(prepared)
	}
}

impl RecentSlots {
	/// Records that a claim was built in `slot` at unix time `now`, and
	/// forgets every slot built more than [`RECENT_SLOT_SECS`] before it.
	fn remember(&mut self, slot: ClaimSlot, now: u64) {
		let cutoff = now.saturating_sub(RECENT_SLOT_SECS);
		self.0.retain(|_, built_at| *built_at >= cutoff);
		self.0.insert(slot, now);
	}

	/// The slots built at unix time `cutoff` or later.
	fn held_slots(&self, cutoff: u64) -> BTreeSet<ClaimSlot> {
		self.0
			.iter()
			.filter(|(_, built_at)| **built_at >= cutoff)
			.map(|(slot, _)| *slot)
			.collect()
	}
}

/// What `claim` answers when it could not be built for `error`: a lost
/// session or a transient failure waits, which ends at the claim's
/// deadline, and anything else gives up at once.
fn answer_unbuilt_claim(claim: &PlannedClaim, error: &ClaimBuildError) -> Option<Preparation> {
	tracing::warn!(id = claim.id.0, %error, "PGAS claim not built");
	match error {
		ClaimBuildError::NoSession | ClaimBuildError::Transient(_) => None,
		ClaimBuildError::OtherIdentity |
		ClaimBuildError::NoPersonhood |
		ClaimBuildError::NoFreeSlot { .. } => Some(Preparation::GiveUp),
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
