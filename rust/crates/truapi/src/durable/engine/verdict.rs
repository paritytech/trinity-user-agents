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
/// retry is written as [`DurableTxStatus::PendingSubmission`] instead.
pub async fn write_verdict(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	verdict: Verdict,
) -> Result<bool, DbError> {
	let status = status_to_write(engine, observed, verdict.status);
	let observed = observed.clone();
	let scanned_to = verdict.scanned_to;
	engine
		.db
		.write(move |tx| dao::compare_and_set(tx, &observed, status, scanned_to))
		.await
}

/// [`DurableTxStatus::PendingSubmission`] when `status` is a failure the
/// row's policy retries, otherwise `status` itself.
fn status_to_write(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	status: VerdictStatus,
) -> DurableTxStatus {
	let VerdictStatus::Failure(failure) = status else {
		return status.into();
	};
	let Some(plan) = &observed.plan else {
		return status.into();
	};
	if policy_retries(engine, observed, plan, failure) {
		return DurableTxStatus::PendingSubmission;
	}
	status.into()
}

/// Asks the policy of `observed`'s domain whether it is built again. A
/// domain that is not registered retries nothing.
fn policy_retries(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	plan: &SubmissionPlan,
	failure: VerdictFailureKind,
) -> bool {
	let Some(policy) = engine.registry.submission_policy(&observed.domain) else {
		error!(
			id = observed.id.0,
			domain = observed.domain.as_str(),
			"durable failure belongs to a domain that is not registered"
		);
		return false;
	};
	policy.can_retry(observed, &plan.params, failure)
}

#[cfg(test)]
mod tests;
