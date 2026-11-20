//! PGAS claims through the durable engine: each claim mints one slot of PGAS
//! into an account, a destination has at most one active claim, and only the
//! identity that registered a claim builds it. [`with_pgas_claims`] serves
//! the domain, and [`PgasClaims`] registers claims, joins a destination's
//! active one, and waits for them.

mod builder;
mod delivery;
#[cfg(test)]
mod live;
mod oracle;
mod params;
mod policy;
#[cfg(test)]
mod testing;

use std::sync::Arc;

use subxt::utils::{AccountId32, H256};

pub use builder::{ClaimBuildError, ClaimIdentity, PgasClaimBuilder};
pub use delivery::UndeliveredClaim;
#[cfg(test)]
pub use testing::{ClaimsHarness, FakeClaimBuilder, TEST_IDENTITY, destination, transient};

use self::{oracle::PgasClaimOracle, params::ClaimParams, policy::PgasClaimPolicy};
use crate::{
	chain::WatchedStorage,
	durable::{
		self, DeferredSubmission, DomainId, DurableRegistry, DurableRequest, DurableTxEngine,
		DurableTxId, DurableTxState, DurableTxStatus, GroupId, RegistrationError,
	},
	store::DbError,
	time::Timer,
	unix_time::current_unix_secs,
};

/// How long a registered claim waits for a session of its identity before it
/// is given up, in seconds.
const SESSION_WAIT_LIMIT_SECS: u64 = 10 * 60;

/// The durable domain PGAS claims are registered under.
fn pgas_domain() -> DomainId {
	DomainId::new("pgas")
}

/// What PGAS claims need to be decided and built.
pub struct PgasClaimsDeps {
	/// Storage of the Asset Hub that runs the `Pgas` pallet.
	pub storage: Arc<dyn WatchedStorage>,
	/// Genesis of that Asset Hub.
	pub genesis: H256,
	/// Builds each claim as the identity that registered it.
	pub builder: Arc<dyn PgasClaimBuilder>,
}

/// `registry` with the PGAS domain on Asset Hub, whose claims the alias
/// oracle decides and the claim policy builds.
pub fn with_pgas_claims(registry: DurableRegistry, deps: PgasClaimsDeps) -> DurableRegistry {
	let PgasClaimsDeps { storage, genesis, builder } = deps;
	let oracle = Arc::new(PgasClaimOracle::new(storage, genesis));
	let policy = Arc::new(PgasClaimPolicy::new(builder));
	registry.with_domain(pgas_domain(), genesis, oracle, policy)
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
	/// Times out a caller's wait for its claim.
	timer: Arc<dyn Timer>,
}

impl PgasClaims {
	/// Claims registered with `engine`, which must serve [`with_pgas_claims`],
	/// whose callers stop waiting on `timer`.
	pub fn new(engine: Arc<DurableTxEngine>, timer: Arc<dyn Timer>) -> Self {
		Self { engine, timer }
	}

	/// Registers a claim of one slot into `dest`, built later as `identity`,
	/// and returns its ledger id. Refuses with
	/// [`PgasClaimError::AlreadyClaiming`] when `dest` already has an active
	/// claim, checked in the registration write.
	async fn claim(
		&self,
		dest: AccountId32,
		identity: ClaimIdentity,
	) -> Result<DurableTxId, RegistrationError<PgasClaimError>> {
		let group = destination_group(&dest);
		let give_up_after = current_unix_secs() + SESSION_WAIT_LIMIT_SECS;
		let params = ClaimParams { target: dest, identity, give_up_after, attempt: None };
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
