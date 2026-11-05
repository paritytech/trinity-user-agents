//! The verdict writer: the one path every verdict takes into the ledger,
//! whether a recovery pass or a submission watch reached it.

use tracing::error;

use super::DurableTxEngine;
use crate::{
	durable::{
		dao,
		model::{
			DurableTxEntry, DurableTxStatus, SubmissionPlan, Verdict, VerdictFailureKind,
			VerdictStatus,
		},
	},
	store::DbError,
};

/// Writes `verdict` only while `observed` is still the row's status and
/// attempt. Returns whether it wrote. A failure the row's policy accepts to
/// retry is written as [`DurableTxStatus::PendingSubmission`] instead. When
/// the policy cannot be read the write fails, because neither answer is
/// known to be right.
pub async fn write_verdict(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	verdict: Verdict,
) -> Result<bool, DbError> {
	let status = status_to_write(engine, observed, verdict.status).await?;
	let observed = observed.clone();
	let scanned_to = verdict.scanned_to;
	engine
		.db
		.write(move |tx| dao::compare_and_set(tx, &observed, status, scanned_to))
		.await
}

/// [`DurableTxStatus::PendingSubmission`] when `status` is a failure the
/// row's policy retries, otherwise `status` itself.
async fn status_to_write(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	status: VerdictStatus,
) -> Result<DurableTxStatus, DbError> {
	let VerdictStatus::Failure(failure) = status else {
		return Ok(status.into());
	};
	let id = observed.id;
	let plan = engine.db.read(move |conn| dao::submission_plan(conn, id)).await?;
	let Some(plan) = plan else {
		return Ok(status.into());
	};
	if policy_retries(engine, observed, &plan, failure) {
		return Ok(DurableTxStatus::PendingSubmission);
	}
	Ok(status.into())
}

/// Asks the policy `plan` names whether `observed` is built again. A policy
/// that is not registered retries nothing.
fn policy_retries(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	plan: &SubmissionPlan,
	failure: VerdictFailureKind,
) -> bool {
	let Some(registered) = engine.registry.submission_policy(&plan.policy) else {
		error!(
			id = observed.id.0,
			policy = plan.policy.as_str(),
			"durable failure names a submission policy that is not registered"
		);
		return false;
	};
	registered.can_retry(observed, &plan.params, failure)
}

#[cfg(test)]
mod tests;
