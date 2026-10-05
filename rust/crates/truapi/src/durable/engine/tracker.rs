//! The submission watch: puts one attempt on the wire and follows it, for
//! latency. It proposes verdicts through the same compare-and-set a pass
//! uses, and hands the transaction back to recovery when it stops watching.

use core::time::Duration;
use std::sync::Arc;

use futures::StreamExt;
use futures::future::{Either, select};
use futures::stream::BoxStream;
use subxt::tx::ValidationResult;
use subxt::utils::H256;
use tracing::{debug, warn};

use super::DurableTxEngine;
use crate::chain::{DispatchOutcome, EncodedExtrinsic, HashAndNumber, WatchEvent};
use crate::durable::dao;
use crate::durable::model::{DurableTxId, DurableTxStatus, FailureKind, Verdict};

/// A watch that hears nothing for this long hands the transaction to
/// recovery, long before its era can end.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(30);

impl DurableTxEngine {
    /// Watches `extrinsic`, the attempt of `id` this engine already owns, on
    /// the chain with `genesis`.
    pub fn spawn_watch(
        self: &Arc<Self>,
        id: DurableTxId,
        genesis: H256,
        extrinsic: EncodedExtrinsic,
    ) {
        let engine = self.clone();
        (self.spawner)(Box::pin(async move {
            engine.watch(id, genesis, &extrinsic).await;
            engine.finish_watch(id, extrinsic.hash()).await;
        }));
    }

    async fn watch(&self, id: DurableTxId, genesis: H256, extrinsic: &EncodedExtrinsic) {
        if !self.admitted(id, genesis, extrinsic).await {
            return;
        }
        match self.submitter.submit_and_watch(genesis, extrinsic).await {
            Ok(events) => self.follow(id, genesis, extrinsic.hash(), events).await,
            Err(error) => warn!(id = id.0, %error, "durable transaction could not be submitted"),
        }
    }

    /// Releases the attempt and, unless the watch decided it, hands the
    /// transaction to recovery.
    async fn finish_watch(&self, id: DurableTxId, tx_hash: H256) {
        self.ownership.release(id, tx_hash);
        if self.needs_recovery(id).await {
            self.nudges.nudge();
        }
    }

    /// Whether the node may be sent `extrinsic`. A refusal fails the
    /// transaction at once: nothing can ever include these bytes.
    async fn admitted(&self, id: DurableTxId, genesis: H256, extrinsic: &EncodedExtrinsic) -> bool {
        match self.validator.validate(genesis, extrinsic).await {
            Ok(ValidationResult::Valid(_) | ValidationResult::Unknown(_)) => true,
            Ok(ValidationResult::Invalid(reason)) => {
                debug!(
                    id = id.0,
                    ?reason,
                    "durable transaction refused before submission"
                );
                let rejected = Verdict {
                    status: DurableTxStatus::Failure,
                    success_detected_at: None,
                    failure: Some(FailureKind::Rejected),
                };
                self.propose(id, extrinsic.hash(), rejected).await;
                false
            }
            Err(error) => {
                warn!(id = id.0, %error, "durable transaction could not be validated");
                false
            }
        }
    }

    /// Follows `events` until a terminal one, the end of the stream, or
    /// [`SILENCE_TIMEOUT`] without one.
    async fn follow(
        &self,
        id: DurableTxId,
        genesis: H256,
        tx_hash: H256,
        mut events: BoxStream<'static, WatchEvent>,
    ) {
        let mut best: Option<H256> = None;
        loop {
            let event = match select(events.next(), self.timer.sleep(SILENCE_TIMEOUT)).await {
                Either::Left((Some(event), _)) => event,
                Either::Left((None, _)) => return,
                Either::Right(_) => {
                    return warn!(id = id.0, "durable submission watch fell silent");
                }
            };
            debug!(id = id.0, ?event, "durable submission status");
            match event {
                WatchEvent::InBestBlock(hash) => {
                    best = Some(hash);
                    self.on_best_block(id, genesis, tx_hash, hash).await;
                }
                WatchEvent::NoLongerInBestBlock => {
                    if let Some(retracted) = best.take() {
                        self.clear_record_at(id, tx_hash, retracted).await;
                    }
                }
                WatchEvent::InFinalizedBlock(hash) => {
                    return self.on_finalized_block(id, genesis, tx_hash, hash).await;
                }
                WatchEvent::Invalid(_) | WatchEvent::Dropped(_) | WatchEvent::Error(_) => return,
            }
        }
    }

    /// Records a successful dispatch in the best block. A failure there is
    /// not final, so it proposes nothing.
    async fn on_best_block(&self, id: DurableTxId, genesis: H256, tx_hash: H256, hash: H256) {
        if let Some((block, DispatchOutcome::Succeeded)) =
            self.outcome_at(genesis, hash, tx_hash).await
        {
            let verdict = Verdict {
                status: DurableTxStatus::PendingSuccess,
                success_detected_at: Some(block),
                failure: None,
            };
            self.propose(id, tx_hash, verdict).await;
        }
    }

    /// Settles the transaction from its finalized block, when the outcome
    /// can be read there.
    async fn on_finalized_block(&self, id: DurableTxId, genesis: H256, tx_hash: H256, hash: H256) {
        let verdict = match self.outcome_at(genesis, hash, tx_hash).await {
            Some((block, DispatchOutcome::Succeeded)) => Verdict {
                status: DurableTxStatus::FinalizedSuccess,
                success_detected_at: Some(block),
                failure: None,
            },
            Some((_, DispatchOutcome::Failed)) => Verdict {
                status: DurableTxStatus::Failure,
                success_detected_at: None,
                failure: Some(FailureKind::DispatchFailed),
            },
            None => return,
        };
        self.propose(id, tx_hash, verdict).await;
    }

    /// The block with `hash` and how `tx_hash` dispatched in it, when both
    /// can be read.
    async fn outcome_at(
        &self,
        genesis: H256,
        hash: H256,
        tx_hash: H256,
    ) -> Option<(HashAndNumber, DispatchOutcome)> {
        let number = self.blocks.block_number(genesis, hash).await.ok()??;
        let block = HashAndNumber { hash, number };
        let outcome = self
            .blocks
            .dispatch_outcome(genesis, block, tx_hash)
            .await
            .ok()??;
        Some((block, outcome))
    }

    /// Demotes the transaction when its record names the retracted block:
    /// a success resting on a block that is gone is no evidence.
    async fn clear_record_at(&self, id: DurableTxId, tx_hash: H256, retracted: H256) {
        let Ok(Some(entry)) = self.db.read(move |conn| Ok(dao::entry(conn, id)?)).await else {
            return;
        };
        if entry.success_detected_at.map(|block| block.hash) == Some(retracted) {
            let demoted = Verdict {
                status: DurableTxStatus::Pending,
                success_detected_at: None,
                failure: None,
            };
            self.propose(id, tx_hash, demoted).await;
        }
    }

    /// Writes `verdict` while the row still awaits one for the watched
    /// attempt.
    async fn propose(&self, id: DurableTxId, tx_hash: H256, verdict: Verdict) {
        let observed = match self.db.read(move |conn| Ok(dao::entry(conn, id)?)).await {
            Ok(Some(observed)) => observed,
            Ok(None) => return,
            Err(error) => {
                return warn!(id = id.0, %error, "durable proposal could not read its row");
            }
        };
        if !observed.status.awaits_verdict() || observed.tx_hash != tx_hash {
            return;
        }
        if let Err(error) = self.write_verdict(&observed, verdict).await {
            warn!(id = id.0, %error, "durable proposal could not be written");
        }
    }

    /// Whether recovery still has to decide `id`. Unreadable counts as yes.
    async fn needs_recovery(&self, id: DurableTxId) -> bool {
        match self.db.read(move |conn| Ok(dao::status(conn, id)?)).await {
            Ok(Some(status)) => status.awaits_verdict(),
            Ok(None) => false,
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::channel::mpsc;
    use futures::executor::block_on;

    use super::*;
    use crate::chain::{DispatchOutcome, MortalExtrinsic};
    use crate::chain_runtime::RuntimeFailure;
    use crate::durable::engine::DurableRequest;
    use crate::durable::model::{DomainId, DurableTxEntry};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::durable::oracle::{DurableRegistry, Unobservable};
    use crate::durable::testing::{
        FakeChain, GENESIS, ManualTimer, block, block_hash, counting_spawner, engine_with_spawner,
        extrinsic,
    };
    use crate::test_support::wait_until;
    use subxt::tx::TransactionInvalid;

    const TEST: DomainId = DomainId::from_static("test");

    struct Fixture {
        _dir: tempfile::TempDir,
        chain: Arc<FakeChain>,
        engine: Arc<DurableTxEngine>,
        timer: Arc<ManualTimer>,
        nudges: mpsc::Receiver<()>,
        finished: Arc<AtomicUsize>,
        id: DurableTxId,
        tx: MortalExtrinsic,
    }

    impl Fixture {
        fn entry(&self) -> DurableTxEntry {
            let id = self.id;
            block_on(self.engine.db.read(move |conn| Ok(dao::entry(conn, id)?)))
                .unwrap()
                .unwrap()
        }

        fn wait_for(&self, status: DurableTxStatus) {
            wait_until(
                || self.entry().status == status,
                "the watch writes its verdict",
            );
        }

        /// Waits for the watch task to finish, release included.
        fn wait_released(&self) {
            wait_until(
                || self.finished.load(Ordering::SeqCst) == 1,
                "the watch ends",
            );
            assert!(!self.engine.ownership.is_owned(self.id));
        }

        fn nudged(&mut self) -> bool {
            self.nudges.try_recv().is_ok()
        }
    }

    /// Registers one transaction whose watch reports what `prepare` scripts.
    fn register(
        prepare: impl FnOnce(&FakeChain) -> Option<mpsc::UnboundedSender<WatchEvent>>,
    ) -> (Fixture, Option<mpsc::UnboundedSender<WatchEvent>>) {
        let chain = FakeChain::new(130, 140);
        let events = prepare(&chain);
        let (spawner, finished) = counting_spawner();
        let (dir, engine, timer) = engine_with_spawner(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
            spawner,
        );
        let nudges = engine.nudges.subscribe();
        let tx = extrinsic(1, 100, 64);
        let id = block_on(
            engine.execute(DurableRequest::presigned(TEST, None, vec![tx.clone()]).unwrap()),
        )
        .unwrap()[0];
        (
            Fixture {
                _dir: dir,
                chain,
                engine,
                timer,
                nudges,
                finished,
                id,
                tx,
            },
            events,
        )
    }

    fn watched() -> (Fixture, mpsc::UnboundedSender<WatchEvent>) {
        let (fixture, events) = register(|chain| Some(chain.script_watch()));
        (fixture, events.unwrap())
    }

    /// Android: WatcherScenariosTest `InBlock with a successful dispatch records the block`.
    #[test]
    fn inclusion_in_a_best_block_with_success_records_that_block() {
        let (fixture, events) = watched();
        fixture
            .chain
            .include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);

        events
            .unbounded_send(WatchEvent::InBestBlock(block_hash(135)))
            .unwrap();

        fixture.wait_for(DurableTxStatus::PendingSuccess);
        assert_eq!(fixture.entry().success_detected_at, Some(block(135)));
    }

    /// Not finalized, so a failed dispatch there proposes nothing.
    #[test]
    fn a_failed_dispatch_in_a_best_block_proposes_nothing() {
        let (mut fixture, events) = watched();
        fixture
            .chain
            .include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Failed);

        events
            .unbounded_send(WatchEvent::InBestBlock(block_hash(135)))
            .unwrap();
        drop(events);

        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.entry().status;
                (observed, fixture.nudged())
            },
            (DurableTxStatus::Pending, true)
        );
    }

    /// Android: WatcherScenariosTest `Finalized with success finalizes the entry`,
    /// `a finalized transaction asks for no recovery`.
    #[test]
    fn finality_with_success_finalizes_and_asks_for_no_recovery() {
        let (mut fixture, events) = watched();
        fixture
            .chain
            .include(125, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);

        events
            .unbounded_send(WatchEvent::InFinalizedBlock(block_hash(125)))
            .unwrap();

        fixture.wait_for(DurableTxStatus::FinalizedSuccess);
        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.entry().success_detected_at;
                (observed, fixture.nudged())
            },
            (Some(block(125)), false)
        );
    }

    #[test]
    fn finality_with_a_failed_dispatch_fails() {
        let (fixture, events) = watched();
        fixture
            .chain
            .include(125, fixture.tx.extrinsic.hash(), DispatchOutcome::Failed);

        events
            .unbounded_send(WatchEvent::InFinalizedBlock(block_hash(125)))
            .unwrap();

        fixture.wait_for(DurableTxStatus::Failure);
    }

    /// Android: WatcherScenariosTest `a node refusal before submission fails without waiting for the window`,
    /// `a pre-submission refusal asks for no recovery`.
    #[test]
    fn a_refusal_before_submission_fails_at_once_and_sends_nothing() {
        let (mut fixture, _) = register(|chain| {
            chain.state().validation = Ok(ValidationResult::Invalid(TransactionInvalid::Stale));
            None
        });

        fixture.wait_for(DurableTxStatus::Failure);
        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.chain.state().submitted.len();
                (observed, fixture.nudged())
            },
            (0, false)
        );
    }

    /// Android: WatcherScenariosTest `a failure after reaching the node is left to the pass`,
    /// `a transaction left undecided when its watch ends is handed to recovery`.
    #[test]
    fn a_dropped_transaction_is_handed_to_recovery() {
        let (mut fixture, events) = watched();

        events
            .unbounded_send(WatchEvent::Dropped("pool full".into()))
            .unwrap();

        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.entry().status;
                (observed, fixture.nudged())
            },
            (DurableTxStatus::Pending, true)
        );
    }

    /// Android: WatcherScenariosTest `inclusion in an unreadable block records nothing`.
    #[test]
    fn inclusion_whose_outcome_cannot_be_read_records_nothing() {
        let (fixture, events) = watched();
        fixture
            .chain
            .include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);
        fixture
            .chain
            .state()
            .unreadable_outcomes
            .insert(block_hash(135));

        events
            .unbounded_send(WatchEvent::InBestBlock(block_hash(135)))
            .unwrap();
        drop(events);

        fixture.wait_released();
        assert_eq!(fixture.entry().status, DurableTxStatus::Pending);
    }

    /// Android: WatcherScenariosTest `a retraction of the recorded block clears the record`.
    #[test]
    fn a_retraction_of_the_recorded_block_clears_the_record() {
        let (fixture, events) = watched();
        fixture
            .chain
            .include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);
        events
            .unbounded_send(WatchEvent::InBestBlock(block_hash(135)))
            .unwrap();
        fixture.wait_for(DurableTxStatus::PendingSuccess);

        events
            .unbounded_send(WatchEvent::NoLongerInBestBlock)
            .unwrap();

        fixture.wait_for(DurableTxStatus::Pending);
        assert_eq!(fixture.entry().success_detected_at, None);
    }

    /// Android: WatcherScenariosTest `a retraction naming another block leaves the record alone`.
    #[test]
    fn a_retraction_of_a_block_that_was_not_recorded_leaves_the_record() {
        let (fixture, events) = watched();
        fixture
            .chain
            .include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);
        events
            .unbounded_send(WatchEvent::InBestBlock(block_hash(135)))
            .unwrap();
        fixture.wait_for(DurableTxStatus::PendingSuccess);
        fixture
            .chain
            .state()
            .unreadable_outcomes
            .insert(block_hash(136));

        events
            .unbounded_send(WatchEvent::InBestBlock(block_hash(136)))
            .unwrap();
        events
            .unbounded_send(WatchEvent::NoLongerInBestBlock)
            .unwrap();
        drop(events);

        fixture.wait_released();
        assert_eq!(
            (fixture.entry().status, fixture.entry().success_detected_at),
            (DurableTxStatus::PendingSuccess, Some(block(135)))
        );
    }

    /// Android: WatcherScenariosTest `a failed subscription hands the entry to recovery with its lock intact`.
    #[test]
    fn a_watch_that_ends_without_a_verdict_is_handed_to_recovery() {
        let (mut fixture, events) = watched();

        drop(events);

        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.entry().status;
                (observed, fixture.nudged())
            },
            (DurableTxStatus::Pending, true)
        );
    }

    /// Android: WatcherScenariosTest `a subscription that cannot open is handled like a dead one`.
    #[test]
    fn a_submission_that_cannot_open_is_handed_to_recovery() {
        let (mut fixture, _) = register(|chain| {
            chain.state().submit_fails = true;
            None
        });

        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.entry().status;
                (observed, fixture.nudged())
            },
            (DurableTxStatus::Pending, true)
        );
    }

    #[test]
    fn a_validation_the_node_cannot_answer_is_handed_to_recovery() {
        let (mut fixture, _) = register(|chain| {
            chain.state().validation = Err(RuntimeFailure::host_failure("validate", "down"));
            None
        });

        fixture.wait_released();
        assert_eq!(
            {
                let observed = fixture.chain.state().submitted.len();
                (observed, fixture.nudged())
            },
            (0, true)
        );
    }

    /// Android: WatcherScenariosTest `the silence timeout`; a watch that hears
    /// nothing releases the transaction.
    #[test]
    fn a_silent_watch_releases_after_the_timeout() {
        let (mut fixture, _events) = watched();
        wait_until(
            || fixture.timer.sleeps() > 0,
            "the watch waits for an event",
        );
        assert!(fixture.engine.ownership.is_owned(fixture.id));

        fixture.timer.advance(SILENCE_TIMEOUT);

        fixture.wait_released();
        assert!(fixture.nudged());
    }

    /// Android: RegistrationScenariosTest `a late event after release changes nothing`.
    #[test]
    fn a_late_event_after_release_changes_nothing() {
        let (fixture, events) = watched();
        events
            .unbounded_send(WatchEvent::Dropped("gone".into()))
            .unwrap();
        fixture.wait_released();
        fixture
            .chain
            .include(135, fixture.tx.extrinsic.hash(), DispatchOutcome::Succeeded);

        let _ = events.unbounded_send(WatchEvent::InBestBlock(block_hash(135)));
        std::thread::sleep(Duration::from_millis(50));

        assert_eq!(
            (
                fixture.entry().status,
                fixture.chain.state().submitted.len()
            ),
            (DurableTxStatus::Pending, 1)
        );
    }
}
