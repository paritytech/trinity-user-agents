//! The verdict writer: the one path every verdict takes into the ledger,
//! whether a recovery pass or a submission watch reached it.

use tracing::error;

use super::DurableTxEngine;
use crate::durable::dao;
use crate::durable::model::{DurableTxEntry, DurableTxStatus, FailureKind, PolicyRef, Verdict};
use crate::store::DbError;

/// The verdict that hands a transaction back to its policy to be built again.
const BACK_TO_POLICY: Verdict = Verdict {
    status: DurableTxStatus::PendingSubmission,
    success_detected_at: None,
    failure: None,
};

impl DurableTxEngine {
    /// Writes `verdict` only while `observed` is still the row's status and
    /// attempt. Returns whether it wrote. A failure the row's policy accepts
    /// to retry is written as [`DurableTxStatus::PendingSubmission`] instead.
    /// When the policy cannot be read the write fails, because neither answer
    /// is known to be right.
    pub async fn write_verdict(
        &self,
        observed: &DurableTxEntry,
        verdict: Verdict,
    ) -> Result<bool, DbError> {
        let verdict = self.retry_or_keep(observed, verdict).await?;
        let observed = observed.clone();
        self.db
            .write(move |tx| dao::compare_and_set(tx, &observed, &verdict))
            .await
    }

    /// [`BACK_TO_POLICY`] when `verdict` is a failure the row's policy
    /// retries, otherwise `verdict` itself.
    async fn retry_or_keep(
        &self,
        observed: &DurableTxEntry,
        verdict: Verdict,
    ) -> Result<Verdict, DbError> {
        let Some(failure) = failure_kind(&verdict) else {
            return Ok(verdict);
        };
        let id = observed.id;
        let policy = self
            .db
            .read(move |conn| dao::submission_policy(conn, id))
            .await?;
        match policy {
            Some(policy) if self.policy_retries(observed, &policy, failure).await => {
                Ok(BACK_TO_POLICY)
            }
            _ => Ok(verdict),
        }
    }

    /// Asks the policy `policy` names whether `observed` is built again. A
    /// policy that is not registered retries nothing.
    async fn policy_retries(
        &self,
        observed: &DurableTxEntry,
        policy: &PolicyRef,
        failure: FailureKind,
    ) -> bool {
        let Some(registered) = self.registry.policy(&policy.id) else {
            error!(
                id = observed.id.0,
                policy = policy.id.as_str(),
                "durable failure names a submission policy that is not registered"
            );
            return false;
        };
        registered
            .can_retry(observed, &policy.params, failure)
            .await
    }
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
    use crate::durable::model::{
        DomainId, DurableTxId, DurableTxStatus, FailureKind, PolicyId, PolicyRef,
    };
    use crate::durable::oracle::{DurableRegistry, Unobservable};
    use crate::durable::testing::{
        FakeChain, GENESIS, RetryQuestion, ScriptedPolicy, block, engine_with, extrinsic, insert,
        insert_retryable,
    };

    const TEST: DomainId = DomainId::from_static("test");
    const POLICY: PolicyId = PolicyId::from_static("rebuild");

    struct Fixture {
        _dir: tempfile::TempDir,
        engine: Arc<DurableTxEngine>,
        policy: Arc<ScriptedPolicy>,
    }

    /// An engine whose `rebuild` policy answers `retries`, or that has no
    /// policy at all when `registered` is false.
    fn fixture(retries: bool, registered: bool) -> Fixture {
        let chain = FakeChain::new(130, 140);
        let policy = ScriptedPolicy::retrying(POLICY, retries);
        let mut registry =
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS)));
        if registered {
            registry = registry.with_policy(policy.clone());
        }
        let (dir, engine, _timer) = engine_with(&chain, registry);
        Fixture {
            _dir: dir,
            engine,
            policy,
        }
    }

    fn retryable(fixture: &Fixture) -> DurableTxEntry {
        let policy = PolicyRef {
            id: POLICY,
            params: vec![7],
        };
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
        block_on(fixture.engine.write_verdict(observed, verdict)).unwrap()
    }

    /// The nonce went stale while the transaction waited; its policy can sign
    /// it again, so the failure goes back to be built instead of being final.
    #[test]
    fn a_failure_the_policy_retries_goes_back_to_be_built() {
        let fixture = fixture(true, true);
        let observed = retryable(&fixture);

        assert!(write(
            &fixture,
            &observed,
            failure(Some(FailureKind::DispatchFailed))
        ));

        assert_eq!(
            status(&fixture, observed.id),
            Some(DurableTxStatus::PendingSubmission)
        );
        assert_eq!(
            fixture.policy.retry_questions(),
            vec![RetryQuestion {
                entry: observed,
                params: vec![7],
                failure: FailureKind::DispatchFailed,
            }]
        );
    }

    #[test]
    fn a_failure_without_a_policy_stays_final() {
        let fixture = fixture(true, true);
        let id = insert(&fixture.engine.db, &TEST, extrinsic(1, 100, 64));
        let observed = entry(&fixture, id);

        write(&fixture, &observed, failure(Some(FailureKind::Expired)));

        assert_eq!(status(&fixture, id), Some(DurableTxStatus::Failure));
        assert_eq!(fixture.policy.retry_questions(), vec![]);
    }

    #[test]
    fn a_failure_the_policy_declines_stays_final() {
        let fixture = fixture(false, true);
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
        let fixture = fixture(true, false);
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
        let fixture = fixture(true, true);
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
        let fixture = fixture(true, true);
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
        let fixture = fixture(true, true);
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

        let written = block_on(
            fixture
                .engine
                .write_verdict(&observed, failure(Some(FailureKind::Expired))),
        );

        assert!(written.is_err());
        assert_eq!(status(&fixture, id), Some(DurableTxStatus::Pending));
    }
}
