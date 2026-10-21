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
mod tests {
	use futures::executor::block_on;

	use super::*;
	use crate::durable::{
		model::{
			DurableTxFailureKind, DurableTxStatus, PolicyParams, VerdictFailureKind, VerdictStatus,
		},
		testing::{RetryQuestion, ScheduledTxHarness, ScriptedPolicy, block, extrinsic, test_plan},
	};

	/// A submitted row the test policy rebuilds with params `[7]`.
	fn rebuildable(harness: &ScheduledTxHarness) -> DurableTxEntry {
		let id = harness.insert_unwatched(extrinsic(1, 100, 64), Some(test_plan(vec![7])));
		harness.entry(id)
	}

	fn failure(kind: VerdictFailureKind) -> Verdict {
		VerdictStatus::Failure(kind).into()
	}

	fn write(harness: &ScheduledTxHarness, observed: &DurableTxEntry, verdict: Verdict) -> bool {
		block_on(write_verdict(harness.engine(), observed, verdict)).unwrap()
	}

	/// The nonce went stale while the transaction waited; its policy can sign
	/// it again, so the failure goes back to be built instead of being final.
	#[test]
	fn a_failure_the_policy_retries_goes_back_to_be_built() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let observed = rebuildable(&harness);

		assert!(write(&harness, &observed, failure(VerdictFailureKind::DispatchFailed)));

		assert_eq!(harness.status(observed.id), DurableTxStatus::PendingSubmission);
		assert_eq!(
			harness.policy().retry_questions(),
			vec![RetryQuestion {
				entry: observed,
				params: PolicyParams(vec![7]),
				failure: VerdictFailureKind::DispatchFailed,
			}]
		);
	}

	#[test]
	fn a_failure_without_a_policy_stays_final() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let id = harness.insert_unwatched(extrinsic(1, 100, 64), None);

		write(&harness, &harness.entry(id), failure(VerdictFailureKind::Expired));

		assert_eq!(harness.status(id), DurableTxStatus::Failure(DurableTxFailureKind::Expired));
		assert_eq!(harness.policy().retry_questions(), vec![]);
	}

	#[test]
	fn a_failure_the_policy_declines_stays_final() {
		let declining = ScriptedPolicy::never_answering().declining_retries();
		let harness = ScheduledTxHarness::with_policy(declining);
		let observed = rebuildable(&harness);

		write(&harness, &observed, failure(VerdictFailureKind::Rejected));

		let rejected = DurableTxStatus::Failure(DurableTxFailureKind::Rejected);
		assert_eq!(harness.status(observed.id), rejected);
	}

	/// An app update dropped the policy a row from an earlier run names.
	/// Nothing can build it again, so the failure is final.
	#[test]
	fn a_failure_whose_policy_is_no_longer_registered_stays_final() {
		let harness = ScheduledTxHarness::with_policies(vec![]);
		let observed = rebuildable(&harness);

		write(&harness, &observed, failure(VerdictFailureKind::Expired));

		let expired = DurableTxStatus::Failure(DurableTxFailureKind::Expired);
		assert_eq!(harness.status(observed.id), expired);
	}

	#[test]
	fn a_success_is_written_without_asking_the_policy() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let observed = rebuildable(&harness);
		let finalized = VerdictStatus::FinalizedSuccess { at: Some(block(120)) };

		write(&harness, &observed, finalized.into());

		assert_eq!(harness.status(observed.id), finalized.into());
		assert_eq!(harness.policy().retry_questions(), vec![]);
	}

	/// Whether the transaction may be retried is unknown, so neither answer
	/// may be written: the write fails and the next pass asks again.
	#[test]
	fn a_policy_that_cannot_be_read_writes_nothing() {
		let harness = ScheduledTxHarness::with_policy(ScriptedPolicy::never_answering());
		let observed = rebuildable(&harness);
		let id = observed.id;
		block_on(harness.engine().db.write(move |tx| {
			tx.execute("UPDATE durable_tx SET policy_params = 5 WHERE id = ?1", [id.0])?;
			Ok(())
		}))
		.unwrap();

		let written = block_on(write_verdict(
			harness.engine(),
			&observed,
			failure(VerdictFailureKind::Expired),
		));

		assert!(written.is_err());
		assert_eq!(harness.status(id), DurableTxStatus::Pending);
	}
}
