//! The verdict writer: the one path every verdict takes into the ledger,
//! whether a recovery pass or a submission watch reached it.

use rusqlite::Connection;
use tracing::error;

use super::DurableTxEngine;
use crate::{
	durable::{
		dao,
		model::{
			DurableTxEntry, DurableTxStatus, SubmissionPlan, Verdict, VerdictFailureKind,
			VerdictStatus,
		},
		oracle::DurableRegistry,
	},
	store::DbError,
};

/// Writes `verdict` only while `observed` is still the row's status and
/// attempt. Returns whether it wrote. A failure is written as
/// [`DurableTxStatus::PendingSubmission`] instead when a dependency of the
/// row has not taken effect, or else when the row's policy accepts to
/// retry it.
pub async fn write_verdict(
	engine: &DurableTxEngine,
	observed: &DurableTxEntry,
	verdict: Verdict,
) -> Result<bool, DbError> {
	let registry = engine.registry.clone();
	let observed = observed.clone();
	let scanned_to = verdict.scanned_to;
	engine
		.db
		.write(move |tx| {
			let status = status_to_write(tx, &registry, &observed, verdict.status)?;
			dao::compare_and_set(tx, &observed, status, scanned_to)
		})
		.await
}

/// [`DurableTxStatus::PendingSubmission`] when `status` is a failure of a
/// row with a plan whose dependency has not taken effect, or that its
/// policy retries. Otherwise `status` itself.
fn status_to_write(
	conn: &Connection,
	registry: &DurableRegistry,
	observed: &DurableTxEntry,
	status: VerdictStatus,
) -> Result<DurableTxStatus, DbError> {
	let VerdictStatus::Failure(failure) = status else {
		return Ok(status.into());
	};
	let Some(plan) = &observed.plan else {
		return Ok(status.into());
	};
	let waits_again = dao::has_unsettled_dependency(conn, observed.id)? ||
		policy_retries(registry, observed, plan, failure);
	Ok(if waits_again { DurableTxStatus::PendingSubmission } else { status.into() })
}

/// Asks the policy of `observed`'s domain whether it is built again. A
/// domain that is not registered retries nothing.
fn policy_retries(
	registry: &DurableRegistry,
	observed: &DurableTxEntry,
	plan: &SubmissionPlan,
	failure: VerdictFailureKind,
) -> bool {
	let Some(policy) = registry.submission_policy(&observed.domain) else {
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
