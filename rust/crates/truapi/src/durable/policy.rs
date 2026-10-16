//! [`SubmissionPolicy`]: how a domain builds the transactions it scheduled,
//! and whether a failed attempt is built again.

use std::collections::HashMap;

use super::model::{
	DurableTxEntry, DurableTxId, FailureKind, PolicyId, PolicyParams, ScheduledDurableTx,
};
use crate::chain::MortalExtrinsic;

/// Builds transactions that were registered before they could be signed, and
/// decides which proven failures are worth another attempt.
#[async_trait::async_trait]
pub trait SubmissionPolicy: Send + Sync {
	/// The name rows store to find this policy again after a restart.
	fn id(&self) -> PolicyId;

	/// Whether `entry`, which failed for `failure`, goes back to
	/// [`Self::prepare`] instead of failing for good. Asked while its verdict
	/// is written, so it must not read the chain.
	async fn can_retry(
		&self,
		entry: &DurableTxEntry,
		params: &PolicyParams,
		failure: FailureKind,
	) -> bool;

	/// What to do with each transaction of `batch`, which share a domain, a
	/// policy and a group. A transaction missing from the answer keeps
	/// waiting. An error is never a verdict: the batch is asked again later.
	async fn prepare(
		&self,
		batch: &[ScheduledDurableTx],
	) -> Result<HashMap<DurableTxId, Preparation>, PolicyError>;
}

/// A policy's answer for one scheduled transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preparation {
	/// Signed and ready to submit. Bytes an earlier attempt of the same
	/// transaction already used are not submitted again in this process: the
	/// transaction keeps waiting for new ones.
	Ready(MortalExtrinsic),
	/// It can never be built. The transaction fails.
	GiveUp,
}

/// Why a policy could not answer for a batch this time.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("submission policy could not prepare its batch: {reason}")]
pub struct PolicyError {
	/// What went wrong.
	pub reason: String,
}
