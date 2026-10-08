//! The verdict writer: the one path every verdict takes into the ledger,
//! whether a recovery pass or a submission watch reached it.

use tracing::error;

use super::DurableTxEngine;
use crate::durable::dao;
use crate::durable::model::{DurableTxEntry, DurableTxStatus, FailureKind, SubmissionPlan, Verdict};
use crate::store::DbError;

/// The verdict that hands a transaction back to its policy to be built again.
const BACK_TO_POLICY: Verdict = Verdict {
    status: DurableTxStatus::PendingSubmission,
    success_detected_at: None,
    failure: None,
};

/// Writes `verdict` only while `observed` is still the row's status and
/// attempt. Returns whether it wrote. A failure the row's policy accepts to
/// retry is written as [`DurableTxStatus::PendingSubmission`] instead. When
/// the policy cannot be read the write fails, because neither answer is
/// known to be right. A final verdict ends the row's rebuild cooldown.
pub async fn write_verdict(
    engine: &DurableTxEngine,
    observed: &DurableTxEntry,
    verdict: Verdict,
) -> Result<bool, DbError> {
    let verdict = retry_or_keep(engine, observed, verdict).await?;
    let (id, decided) = (observed.id, verdict.status.is_final());
    let observed = observed.clone();
    let written = engine
        .db
        .write(move |tx| dao::compare_and_set(tx, &observed, &verdict))
        .await?;
    if written && decided {
        engine.executor.forget_attempts(id);
    }
    Ok(written)
}

/// [`BACK_TO_POLICY`] when `verdict` is a failure the row's policy retries,
/// otherwise `verdict` itself.
async fn retry_or_keep(
    engine: &DurableTxEngine,
    observed: &DurableTxEntry,
    verdict: Verdict,
) -> Result<Verdict, DbError> {
    let Some(failure) = failure_kind(&verdict) else {
        return Ok(verdict);
    };
    let id = observed.id;
    let plan = engine
        .db
        .read(move |conn| dao::submission_plan(conn, id))
        .await?;
    let Some(plan) = plan else {
        return Ok(verdict);
    };
    if policy_retries(engine, observed, &plan, failure).await {
        return Ok(BACK_TO_POLICY);
    }
    Ok(verdict)
}

/// Asks the policy `plan` names whether `observed` is built again. A policy
/// that is not registered retries nothing.
async fn policy_retries(
    engine: &DurableTxEngine,
    observed: &DurableTxEntry,
    plan: &SubmissionPlan,
    failure: FailureKind,
) -> bool {
    let Some(registered) = engine.registry.submission_policy(&plan.policy) else {
        error!(
            id = observed.id.0,
            policy = plan.policy.as_str(),
            "durable failure names a submission policy that is not registered"
        );
        return false;
    };
    registered
        .can_retry(observed, &plan.params, failure)
        .await
}

/// Why `verdict` fails its transaction, when it does and the reason is known.
fn failure_kind(verdict: &Verdict) -> Option<FailureKind> {
    match verdict.status {
        DurableTxStatus::Failure => verdict.failure,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::executor::block_on;

    use super::*;
    use crate::durable::model::{DomainId, DurableTxId, DurableTxStatus, FailureKind, PolicyParams};
    use crate::durable::testing::{
        FakeChain, REBUILD_POLICY, RetryAnswer, RetryQuestion, ScriptedPolicy, block,
        engine_with_policies, extrinsic, insert, insert_retryable, rebuild_plan,
    };

    const TEST: DomainId = DomainId::from_static("test");

    struct Fixture {
        _dir: tempfile::TempDir,
        engine: Arc<DurableTxEngine>,
        policy: Arc<ScriptedPolicy>,
    }

    /// An engine whose `rebuild` policy answers `retry_answer`.
    fn fixture(retry_answer: RetryAnswer) -> Fixture {
        let policy = ScriptedPolicy::never_building(REBUILD_POLICY, retry_answer);
        fixture_registering(policy.clone(), vec![policy])
    }

    /// An engine whose rows name a `rebuild` policy it does not have.
    fn fixture_without_policy() -> Fixture {
        let policy = ScriptedPolicy::never_building(REBUILD_POLICY, RetryAnswer::Retry);
        fixture_registering(policy, vec![])
    }

    /// An engine registering `registered`, watched through `policy`.
    fn fixture_registering(
        policy: Arc<ScriptedPolicy>,
        registered: Vec<Arc<ScriptedPolicy>>,
    ) -> Fixture {
        let chain = FakeChain::new(130, 140);
        let (dir, engine, _timer) = engine_with_policies(&chain, &[TEST], registered);
        Fixture {
            _dir: dir,
            engine,
            policy,
        }
    }

    fn retryable(fixture: &Fixture) -> DurableTxEntry {
        let policy = rebuild_plan(vec![7]);
        let id = insert_retryable(&fixture.engine.db, &TEST, extrinsic(1, 100, 64), policy);
        entry(fixture, id)
    }

    fn entry(fixture: &Fixture, id: DurableTxId) -> DurableTxEntry {
        block_on(fixture.engine.db.read(move |conn| dao::entry(conn, id)))
            .unwrap()
            .unwrap()
    }

    fn status(fixture: &Fixture, id: DurableTxId) -> Option<DurableTxStatus> {
        block_on(fixture.engine.status(id)).unwrap()
    }

    fn failure(kind: Option<FailureKind>) -> Verdict {
        Verdict {
            status: DurableTxStatus::Failure,
            success_detected_at: None,
            failure: kind,
        }
    }

    fn write(fixture: &Fixture, observed: &DurableTxEntry, verdict: Verdict) -> bool {
        block_on(write_verdict(&fixture.engine, observed, verdict)).unwrap()
    }

    /// The nonce went stale while the transaction waited; its policy can sign
    /// it again, so the failure goes back to be built instead of being final.
    #[test]
    fn a_failure_the_policy_retries_goes_back_to_be_built() {
        let fixture = fixture(RetryAnswer::Retry);
        let observed = retryable(&fixture);
        let dispatch_failed = failure(Some(FailureKind::DispatchFailed));

        assert!(write(&fixture, &observed, dispatch_failed));

        assert_eq!(
            status(&fixture, observed.id),
            Some(DurableTxStatus::PendingSubmission)
        );
        assert_eq!(
            fixture.policy.retry_questions(),
            vec![RetryQuestion {
                entry: observed,
                params: PolicyParams(vec![7]),
                failure: FailureKind::DispatchFailed,
            }]
        );
    }

    #[test]
    fn a_failure_without_a_policy_stays_final() {
        let fixture = fixture(RetryAnswer::Retry);
        let id = insert(&fixture.engine.db, &TEST, extrinsic(1, 100, 64));
        let observed = entry(&fixture, id);

        write(&fixture, &observed, failure(Some(FailureKind::Expired)));

        assert_eq!(status(&fixture, id), Some(DurableTxStatus::Failure));
        assert_eq!(fixture.policy.retry_questions(), vec![]);
    }

    #[test]
    fn a_failure_the_policy_declines_stays_final() {
        let fixture = fixture(RetryAnswer::Final);
        let observed = retryable(&fixture);

        write(&fixture, &observed, failure(Some(FailureKind::Rejected)));

        assert_eq!(
            status(&fixture, observed.id),
            Some(DurableTxStatus::Failure)
        );
    }

    /// An app update dropped the policy a row from an earlier run names.
    /// Nothing can build it again, so the failure is final.
    #[test]
    fn a_failure_whose_policy_is_no_longer_registered_stays_final() {
        let fixture = fixture_without_policy();
        let observed = retryable(&fixture);

        write(&fixture, &observed, failure(Some(FailureKind::Expired)));

        assert_eq!(
            status(&fixture, observed.id),
            Some(DurableTxStatus::Failure)
        );
    }

    /// A failure whose reason is unknown gives the policy nothing to judge.
    #[test]
    fn a_failure_without_a_kind_stays_final_without_asking_the_policy() {
        let fixture = fixture(RetryAnswer::Retry);
        let observed = retryable(&fixture);

        write(&fixture, &observed, failure(None));

        assert_eq!(
            status(&fixture, observed.id),
            Some(DurableTxStatus::Failure)
        );
        assert_eq!(fixture.policy.retry_questions(), vec![]);
    }

    #[test]
    fn a_success_is_written_without_asking_the_policy() {
        let fixture = fixture(RetryAnswer::Retry);
        let observed = retryable(&fixture);
        let success = Verdict {
            status: DurableTxStatus::FinalizedSuccess,
            success_detected_at: Some(block(120)),
            failure: None,
        };

        write(&fixture, &observed, success);

        assert_eq!(
            status(&fixture, observed.id),
            Some(DurableTxStatus::FinalizedSuccess)
        );
        assert_eq!(fixture.policy.retry_questions(), vec![]);
    }

    /// Whether the transaction may be retried is unknown, so neither answer
    /// may be written: the write fails and the next pass asks again.
    #[test]
    fn a_policy_that_cannot_be_read_writes_nothing() {
        let fixture = fixture(RetryAnswer::Retry);
        let observed = retryable(&fixture);
        let id = observed.id;
        block_on(fixture.engine.db.write(move |tx| {
            tx.execute(
                "UPDATE durable_tx SET policy_params = 5 WHERE id = ?1",
                [id.0],
            )?;
            Ok(())
        }))
        .unwrap();

        let expired = failure(Some(FailureKind::Expired));
        let written = block_on(write_verdict(&fixture.engine, &observed, expired));

        assert!(written.is_err());
        assert_eq!(status(&fixture, id), Some(DurableTxStatus::Pending));
    }
}
