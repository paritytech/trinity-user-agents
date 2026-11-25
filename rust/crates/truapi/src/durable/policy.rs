//! [`SubmissionPolicy`]: how a domain builds the transactions it deferred,
//! and whether a failed attempt is built again.

use std::collections::HashMap;

use super::model::{
	DeferredDurableTx, DurableTxEntry, DurableTxId, PolicyParams, VerdictFailureKind,
};
use crate::chain::MortalExtrinsic;

/// Builds transactions that were registered before they could be signed, and
/// decides which proven failures are worth another attempt.
#[async_trait::async_trait]
pub trait SubmissionPolicy: Send + Sync {
	/// Whether `entry`, which failed for `failure`, goes back to
	/// [`Self::prepare`] instead of failing for good. Asked inside the write
	/// of its verdict, so it decides from what it is given, never waits and
	/// never touches the core database.
	fn can_retry(
		&self,
		entry: &DurableTxEntry,
		params: &PolicyParams,
		failure: VerdictFailureKind,
	) -> bool;

	/// What to do with each transaction of `batch`: the domain's waiting
	/// transactions the engine wants built now, of any group. A batch is never
	/// all of them, as it leaves out those still followed, cooling down or
	/// blocked by a dependency. Each one lists its dependencies, which all
	/// succeeded or failed with
	/// [`OnDependencyFailure::Ignore`](super::model::OnDependencyFailure::Ignore), so
	/// decide each transaction from itself, the ledger and the chain, never
	/// from what the batch holds. A transaction missing from the answer keeps
	/// waiting. An error is never a verdict: the batch is asked again later.
	async fn prepare(
		&self,
		batch: &[DeferredDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError>;
}

/// A policy's answer for one deferred transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preparation {
	/// Signed and ready to submit.
	Ready {
		/// The attempt.
		extrinsic: MortalExtrinsic,
		/// Params that replace the row's from this attempt on, for what the
		/// policy learned while building it. `None` keeps the row's.
		params: Option<PolicyParams>,
	},
	/// It can never be built. The transaction fails.
	GiveUp,
}

/// Why a policy could not answer for a batch this time.
#[derive(Debug, Clone, thiserror::Error)]
#[error("submission policy could not prepare its batch: {reason}")]
pub struct PolicyError {
	/// What went wrong.
	pub reason: String,
}
