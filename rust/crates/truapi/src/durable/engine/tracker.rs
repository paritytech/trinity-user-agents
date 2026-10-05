//! The submission watch: submits one attempt to the node and follows it,
//! for latency. It proposes verdicts through the same compare-and-set a pass
//! uses, and hands the transaction back to recovery when it stops watching.

use core::ops::ControlFlow;
use core::time::Duration;
use std::sync::Arc;

use futures::StreamExt;
use futures::future::{Either, select};
use futures::stream::BoxStream;
use subxt::tx::ValidationResult;
use subxt::utils::H256;
use tracing::{debug, warn};

use super::DurableTxEngine;
use crate::chain::{DispatchOutcome, EncodedExtrinsic, HashAndNumber, MortalExtrinsic, WatchEvent};
use crate::durable::dao;
use crate::durable::model::{DurableTxId, DurableTxStatus, FailureKind, Verdict};

/// A watch that hears nothing for this long hands the transaction to
/// recovery, long before its era can end.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(30);

/// One attempt of a transaction, as a watch follows it.
#[derive(Clone, Copy, Debug)]
pub struct Attempt {
    id: DurableTxId,
    genesis: H256,
    tx_hash: H256,
}

impl Attempt {
    /// The attempt `extrinsic` makes at transaction `id` on the chain with
    /// `genesis`.
    pub fn new(id: DurableTxId, genesis: H256, extrinsic: &MortalExtrinsic) -> Self {
        Self {
            id,
            genesis,
            tx_hash: extrinsic.extrinsic.hash(),
        }
    }
}

/// Watches `extrinsic`, an attempt `engine` already owns, until it settles or
/// the watch ends, then hands it back.
pub fn spawn_watch(engine: &Arc<DurableTxEngine>, attempt: Attempt, extrinsic: EncodedExtrinsic) {
    let engine = engine.clone();
    (engine.spawner.clone())(Box::pin(async move {
        engine.watch(attempt, &extrinsic).await;
        engine.finish_watch(attempt).await;
    }));
}

impl DurableTxEngine {
    async fn watch(&self, attempt: Attempt, extrinsic: &EncodedExtrinsic) {
        if !self.admitted(attempt, extrinsic).await {
            return;
        }
        match self
            .submitter
            .submit_and_watch(attempt.genesis, extrinsic)
            .await
        {
            Ok(events) => self.follow(attempt, events).await,
            Err(error) => {
                warn!(id = attempt.id.0, %error, "durable transaction could not be submitted")
            }
        }
    }

    /// Releases the attempt and, unless the watch decided it, hands the
    /// transaction to recovery: a running loop, or else the host.
    async fn finish_watch(&self, attempt: Attempt) {
        self.ownership.release(attempt.id, attempt.tx_hash);
        if self.needs_recovery(attempt.id).await && !self.recovery_wakes.wake() {
            self.host_wakes.wake();
        }
    }

    /// Whether the node may be sent `extrinsic`. A refusal fails the
    /// transaction at once: nothing can ever include these bytes. A
    /// validation that could not run lets it through, as Android does,
    /// because the bytes are held nowhere else.
    async fn admitted(&self, attempt: Attempt, extrinsic: &EncodedExtrinsic) -> bool {
        let validation = self.validator.validate(attempt.genesis, extrinsic).await;
        let Ok(ValidationResult::Invalid(reason)) = validation else {
            return true;
        };
        debug!(
            id = attempt.id.0,
            ?reason,
            "durable transaction refused before submission"
        );
        self.propose(attempt, failure(FailureKind::Rejected)).await;
        false
    }

    /// Follows `events` until a terminal one, the end of the stream, or
    /// [`SILENCE_TIMEOUT`] without one.
    async fn follow(&self, attempt: Attempt, mut events: BoxStream<'static, WatchEvent>) {
        let mut best = None;
        while let Some(event) = self.next_event(attempt, &mut events).await {
            if self.on_event(attempt, &mut best, event).await.is_break() {
                return;
            }
        }
    }

    /// The next event, or `None` when the stream ended or fell silent.
    async fn next_event(
        &self,
        attempt: Attempt,
        events: &mut BoxStream<'static, WatchEvent>,
    ) -> Option<WatchEvent> {
        match select(events.next(), self.timer.sleep(SILENCE_TIMEOUT)).await {
            Either::Left((event, _)) => event,
            Either::Right(_) => {
                warn!(id = attempt.id.0, "durable submission watch fell silent");
                None
            }
        }
    }

    /// Acts on one event. `best` is the best block the extrinsic was last
    /// seen in, since a retraction does not name it.
    async fn on_event(
        &self,
        attempt: Attempt,
        best: &mut Option<H256>,
        event: WatchEvent,
    ) -> ControlFlow<()> {
        debug!(id = attempt.id.0, ?event, "durable submission status");
        match event {
            WatchEvent::InBestBlock(hash) => self.on_best_block(attempt, best.insert(hash)).await,
            WatchEvent::NoLongerInBestBlock => self.on_retraction(attempt, best.take()).await,
            WatchEvent::InFinalizedBlock(hash) => {
                return self.on_finalized_block(attempt, hash).await;
            }
            WatchEvent::Invalid(_) | WatchEvent::Dropped(_) | WatchEvent::Error(_) => {
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }

    /// Records a successful dispatch in the best block. A failure there is
    /// not final, so it proposes nothing.
    async fn on_best_block(&self, attempt: Attempt, hash: &H256) {
        if let Some((block, DispatchOutcome::Succeeded)) = self.outcome_at(attempt, *hash).await {
            self.propose(attempt, success(DurableTxStatus::PendingSuccess, block))
                .await;
        }
    }

    /// Settles the transaction from its finalized block, when the outcome
    /// can be read there. The watch ends either way.
    async fn on_finalized_block(&self, attempt: Attempt, hash: H256) -> ControlFlow<()> {
        let verdict = match self.outcome_at(attempt, hash).await {
            Some((block, DispatchOutcome::Succeeded)) => {
                success(DurableTxStatus::FinalizedSuccess, block)
            }
            Some((_, DispatchOutcome::Failed)) => failure(FailureKind::DispatchFailed),
            None => return ControlFlow::Break(()),
        };
        self.propose(attempt, verdict).await;
        ControlFlow::Break(())
    }

    /// The block with `hash` and how the attempt dispatched in it, when both
    /// can be read.
    async fn outcome_at(
        &self,
        attempt: Attempt,
        hash: H256,
    ) -> Option<(HashAndNumber, DispatchOutcome)> {
        let number = self
            .blocks
            .block_number(attempt.genesis, hash)
            .await
            .ok()??;
        let block = HashAndNumber { hash, number };
        let outcome = self
            .blocks
            .dispatch_outcome(attempt.genesis, block, attempt.tx_hash)
            .await
            .ok()??;
        Some((block, outcome))
    }

    /// Demotes the transaction when its record names the retracted block:
    /// a success resting on a block that is gone is no evidence.
    async fn on_retraction(&self, attempt: Attempt, retracted: Option<H256>) {
        let Some(retracted) = retracted else {
            return;
        };
        let id = attempt.id;
        let Ok(Some(entry)) = self.db.read(move |conn| Ok(dao::entry(conn, id)?)).await else {
            return;
        };
        if entry.success_detected_at.map(|block| block.hash) == Some(retracted) {
            let demoted = Verdict {
                status: DurableTxStatus::Pending,
                success_detected_at: None,
                failure: None,
            };
            self.propose(attempt, demoted).await;
        }
    }

    /// Writes `verdict` while the row still awaits one for the watched
    /// attempt.
    async fn propose(&self, attempt: Attempt, verdict: Verdict) {
        let id = attempt.id;
        let observed = match self.db.read(move |conn| Ok(dao::entry(conn, id)?)).await {
            Ok(Some(observed)) => observed,
            Ok(None) => return,
            Err(error) => {
                return warn!(id = id.0, %error, "durable proposal could not read its row");
            }
        };
        if !observed.status.awaits_verdict() || observed.tx_hash != attempt.tx_hash {
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

fn success(status: DurableTxStatus, block: HashAndNumber) -> Verdict {
    Verdict {
        status,
        success_detected_at: Some(block),
        failure: None,
    }
}

fn failure(kind: FailureKind) -> Verdict {
    Verdict {
        status: DurableTxStatus::Failure,
        success_detected_at: None,
        failure: Some(kind),
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

        /// Waits for the registration task and the watch it started to
        /// finish, the watch's release included.
        fn wait_released(&self) {
            wait_until(
                || self.finished.load(Ordering::SeqCst) == 2,
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
        let nudges = engine.recovery_wakes.subscribe();
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

    /// The bytes exist only in memory, so a validation call that fails must
    /// not stop them being submitted: the node decides when it receives them.
    #[test]
    fn a_validation_the_node_cannot_answer_still_submits() {
        let (fixture, _) = register(|chain| {
            chain.state().validation = Err(RuntimeFailure::host_failure("validate", "down"));
            None
        });

        wait_until(
            || fixture.chain.state().submitted.len() == 1,
            "the extrinsic is submitted",
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
}
