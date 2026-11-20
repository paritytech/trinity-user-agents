//! Waiting for a claim to be delivered: seen in a block, which is when an
//! allowance request answers.

use core::{future::Future, time::Duration};

use futures::{FutureExt, StreamExt, future::ready, stream::BoxStream};
use subxt::utils::AccountId32;

use super::{ClaimIdentity, PgasClaimError, PgasClaims};
use crate::{
	CancellationReason,
	durable::{DurableTxFailureKind, DurableTxId, DurableTxStatus, RegistrationError},
	store::DbError,
};

/// How long a caller waits for its claim to reach a block: the 120 seconds a
/// direct claim waits for inclusion plus the 60 a build may wait for Asset
/// Hub to import the ring revision.
const DELIVERY_WAIT: Duration = Duration::from_secs(180);

/// Why a caller stopped waiting before its claim reached a block. Unless the
/// claim failed, it carries on in the ledger.
#[derive(Debug, thiserror::Error)]
pub enum UndeliveredClaim {
	/// The claim failed for good.
	#[error("the PGAS claim failed: {0:?}")]
	Failed(DurableTxFailureKind),
	/// The claim was not seen in a block within [`DELIVERY_WAIT`].
	#[error("the PGAS claim was not seen in a block within {}s", DELIVERY_WAIT.as_secs())]
	TimedOut,
	/// The caller's request was cancelled.
	#[error("the request waiting for the PGAS claim was {0}")]
	Cancelled(CancellationReason),
	/// The ledger holds no such claim.
	#[error("the ledger holds no such PGAS claim")]
	Missing,
	/// The ledger could not be read.
	#[error("the PGAS claim could not be read: {0}")]
	Ledger(DbError),
}

impl PgasClaims {
	/// Registers a claim of one slot into `dest` as [`Self::claim`] does, or
	/// returns the destination's active claim when it has one.
	pub async fn claim_or_join(
		&self,
		dest: AccountId32,
		identity: ClaimIdentity,
	) -> Result<DurableTxId, RegistrationError<PgasClaimError>> {
		match self.claim(dest, identity).await {
			Err(RegistrationError::Hook(PgasClaimError::AlreadyClaiming(active))) => Ok(active),
			registered => registered,
		}
	}

	/// Waits until claim `id` is seen in a best or finalized block. Fails
	/// once it fails, after [`DELIVERY_WAIT`], or once `cancelled` resolves.
	pub async fn wait_until_delivered(
		&self,
		id: DurableTxId,
		cancelled: impl Future<Output = CancellationReason>,
	) -> Result<(), UndeliveredClaim> {
		let delivered = delivery_outcome(self.engine.observe_status(id)).fuse();
		let timed_out = self.timer.sleep(DELIVERY_WAIT).fuse();
		let cancelled = cancelled.fuse();
		futures::pin_mut!(delivered, timed_out, cancelled);
		futures::select! {
			outcome = delivered => outcome,
			() = timed_out => Err(UndeliveredClaim::TimedOut),
			reason = cancelled => Err(UndeliveredClaim::Cancelled(reason)),
		}
	}
}

/// The outcome of the first status of `statuses` that settles the wait.
async fn delivery_outcome(
	statuses: BoxStream<'static, Result<Option<DurableTxStatus>, DbError>>,
) -> Result<(), UndeliveredClaim> {
	let mut settled = statuses.filter_map(|read| ready(settled_outcome(read)));
	settled.next().await.unwrap_or(Err(UndeliveredClaim::Ledger(DbError::Closed)))
}

/// The outcome `read` settles the wait with, or `None` while the claim is
/// still under way.
fn settled_outcome(
	read: Result<Option<DurableTxStatus>, DbError>,
) -> Option<Result<(), UndeliveredClaim>> {
	match read {
		Err(error) => Some(Err(UndeliveredClaim::Ledger(error))),
		Ok(None) => Some(Err(UndeliveredClaim::Missing)),
		Ok(Some(DurableTxStatus::PendingSubmission | DurableTxStatus::Pending)) => None,
		Ok(Some(
			DurableTxStatus::PendingSuccess { .. } | DurableTxStatus::FinalizedSuccess { .. },
		)) => Some(Ok(())),
		Ok(Some(DurableTxStatus::Failure(kind))) => Some(Err(UndeliveredClaim::Failed(kind))),
	}
}

#[cfg(test)]
mod tests;
