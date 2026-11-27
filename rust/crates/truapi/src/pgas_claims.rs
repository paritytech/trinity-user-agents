//! PGAS claims through the durable engine: each claim mints one slot of PGAS
//! into an account, and a destination has at most one active claim.
//! [`with_pgas_claims`] serves the domain, and [`PgasClaims`] registers
//! claims.

mod builder;
mod claim_extrinsic;
mod params;
mod policy;

use std::sync::Arc;

use subxt::utils::{AccountId32, H256};

pub use builder::{ClaimBuildError, PgasClaimBuilder};
pub use claim_extrinsic::{BuiltClaim, ClaimSlot, build_claim_in_free_slot};

use self::{params::ClaimParams, policy::PgasClaimPolicy};
use crate::{
	durable::{
		self, DeferredSubmission, DomainId, DurableRegistry, DurableRequest, DurableTxEngine,
		DurableTxId, DurableTxState, DurableTxStatus, GroupId, RegistrationError, Unobservable,
	},
	store::DbError,
	unix_time::current_unix_secs,
};

/// How long after registration an attempt of a claim may still be built, in
/// seconds. No wait for a session, no transient failure and no rebuild
/// outlasts it.
const CLAIM_DEADLINE_SECS: u64 = 10 * 60;

/// The durable domain PGAS claims are registered under.
fn pgas_domain() -> DomainId {
	DomainId::new("pgas")
}

/// `registry` with the PGAS domain on the Asset Hub with `genesis`, whose
/// claims `builder` builds.
///
/// No storage proves a claim landed: `Pgas.ClaimedGasAliases` records only
/// the person's alias for a day and slot, which any claim of that slot
/// writes, whatever its target. The engine decides each claim by finding its
/// extrinsic in a block instead.
pub fn with_pgas_claims(
	registry: DurableRegistry,
	genesis: H256,
	builder: Arc<dyn PgasClaimBuilder>,
) -> DurableRegistry {
	let policy = Arc::new(PgasClaimPolicy::new(builder));
	registry.with_domain(pgas_domain(), genesis, Arc::new(Unobservable), policy)
}

/// Why a claim was not registered. Nothing was written.
#[derive(Debug, thiserror::Error)]
pub enum PgasClaimError {
	/// The destination already has an active claim: this one.
	#[error("the destination already has an active PGAS claim, {0:?}")]
	AlreadyClaiming(DurableTxId),
	/// The ledger could not be read.
	#[error("the PGAS claims of the destination could not be read: {0}")]
	Ledger(#[from] DbError),
}

/// Claims PGAS through the durable engine, one slot per claim, with at most
/// one active claim per destination. A claim is active while it waits to be
/// built or is submitted and not yet seen in a block.
#[derive(Clone)]
pub struct PgasClaims {
	engine: Arc<DurableTxEngine>,
}

impl PgasClaims {
	/// Claims registered with `engine`, which must serve [`with_pgas_claims`].
	pub fn new(engine: Arc<DurableTxEngine>) -> Self {
		Self { engine }
	}

	/// Registers a claim of one slot into `dest`, built later, and returns
	/// its ledger id. Refuses with [`PgasClaimError::AlreadyClaiming`] when
	/// `dest` already has an active claim, checked in the registration write.
	pub async fn claim(
		&self,
		dest: AccountId32,
	) -> Result<DurableTxId, RegistrationError<PgasClaimError>> {
		let group = destination_group(&dest);
		let give_up_after = current_unix_secs() + CLAIM_DEADLINE_SECS;
		let params = ClaimParams { target: dest, give_up_after };
		let request = DurableRequest::deferred_in(pgas_domain(), Some(group.clone()), move |tx| {
			refuse_active_claim(tx, &group)?;
			Ok(vec![DeferredSubmission::independent(params.into())])
		});
		let (ids, ()) = self.engine.execute_with(request, |_, _| Ok(())).await?;
		let [id] = ids[..] else {
			unreachable!("a request of one claim registers one row, got {ids:?}")
		};
		Ok(id)
	}
}

/// The group every claim into `dest` is registered under.
fn destination_group(dest: &AccountId32) -> GroupId {
	GroupId::new(format!("dest:{}", hex::encode(dest.0)))
}

/// Refuses with the active claim of `group` when it has one.
fn refuse_active_claim(
	tx: &rusqlite::Transaction<'_>,
	group: &GroupId,
) -> Result<(), PgasClaimError> {
	let claims = durable::group(tx, &pgas_domain(), group)?;
	match first_active_claim(claims) {
		Some(active) => Err(PgasClaimError::AlreadyClaiming(active)),
		None => Ok(()),
	}
}

/// The claim of `claims` that still waits to be built or to be seen in a
/// block. One seen in a best block counts as delivered.
fn first_active_claim(claims: Vec<DurableTxState>) -> Option<DurableTxId> {
	claims
		.into_iter()
		.find(|claim| {
			matches!(claim.status, DurableTxStatus::PendingSubmission | DurableTxStatus::Pending)
		})
		.map(|claim| claim.id)
}

#[cfg(test)]
mod tests;
