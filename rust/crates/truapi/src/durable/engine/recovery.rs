//! The recovery loop: passes on every head and nudge until nothing is live.

use futures::FutureExt;
use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use parking_lot::Mutex;
use subxt::utils::H256;
use tracing::warn;

use super::DurableTxEngine;
use crate::durable::dao;

/// Why recovery stopped while transactions were still live. The host retries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    /// The runtime has no core database, so there is nothing to recover.
    #[error("no core database configured")]
    NotConfigured,
    /// A chain's head subscription failed or ended, so nothing would drive
    /// the next pass.
    #[error("head events of chain {genesis:?} unavailable: {reason}")]
    HeadsLost {
        /// Genesis hash of the chain.
        genesis: H256,
        /// What happened.
        reason: String,
    },
}

/// Asks a running recovery loop for a pass without waiting for a head.
#[derive(Default)]
pub struct Nudges {
    loops: Mutex<Vec<mpsc::Sender<()>>>,
}

impl Nudges {
    /// A stream of nudges for one loop. Nudges sent while one is pending
    /// collapse into it.
    pub fn subscribe(&self) -> mpsc::Receiver<()> {
        let (sender, receiver) = mpsc::channel(0);
        self.loops.lock().push(sender);
        receiver
    }

    /// Wakes every running loop.
    pub fn nudge(&self) {
        let mut loops = self.loops.lock();
        loops.retain(|sender| !sender.is_closed());
        for sender in loops.iter_mut() {
            // A full channel already holds a nudge, which says the same.
            let _ = sender.try_send(());
        }
    }
}

impl DurableTxEngine {
    /// Runs recovery passes until no transaction is live: one at once, then
    /// one per new head of any registered chain and per nudge, collapsing
    /// whatever arrives while a pass runs. An unreadable ledger counts as
    /// live. Fails when a head subscription does; dropping the future stops
    /// it.
    pub async fn run_until_settled(&self) -> Result<(), RecoveryError> {
        let mut triggers = self.triggers().await?;
        loop {
            self.run_pass().await;
            if !self.has_live().await {
                return Ok(());
            }
            next_trigger(&mut triggers).await?;
        }
    }

    /// Every head of every registered chain, and every nudge. A head
    /// subscription that ends yields an error, so the loop stops.
    async fn triggers(
        &self,
    ) -> Result<BoxStream<'static, Result<(), RecoveryError>>, RecoveryError> {
        let mut sources = vec![self.nudges.subscribe().map(Ok).boxed()];
        for genesis in self.registry.chains() {
            sources.push(self.head_triggers(genesis).await?);
        }
        Ok(stream::select_all(sources).boxed())
    }

    /// One trigger per head of the chain with `genesis`. A failed head read
    /// is one missed head; the subscription ending is an error.
    async fn head_triggers(
        &self,
        genesis: H256,
    ) -> Result<BoxStream<'static, Result<(), RecoveryError>>, RecoveryError> {
        let heads =
            self.heads
                .head_events(genesis)
                .await
                .map_err(|error| RecoveryError::HeadsLost {
                    genesis,
                    reason: error.to_string(),
                })?;
        let ended = stream::once(async move {
            Err(RecoveryError::HeadsLost {
                genesis,
                reason: "head events ended".into(),
            })
        });
        Ok(heads
            .filter_map(move |head| async move {
                head.map_err(|error| warn!(?genesis, %error, "durable recovery missed a head"))
                    .ok()
                    .map(|_| Ok(()))
            })
            .chain(ended)
            .boxed())
    }

    /// Whether any transaction is live. An unreadable ledger counts as live:
    /// abandoning transactions is far worse than one more pass.
    async fn has_live(&self) -> bool {
        self.db
            .read(|conn| Ok(dao::has_live(conn)?))
            .await
            .unwrap_or_else(|error| {
                warn!(%error, "durable recovery could not read the ledger");
                true
            })
    }
}

/// Waits for the next trigger, then takes every one already queued behind
/// it, so heads that arrived during a pass cost one more pass, not one each.
async fn next_trigger(
    triggers: &mut BoxStream<'static, Result<(), RecoveryError>>,
) -> Result<(), RecoveryError> {
    triggers.next().await.unwrap_or(Ok(()))?;
    while let Some(Some(trigger)) = triggers.next().now_or_never() {
        trigger?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use futures::executor::block_on;

    use super::*;
    use crate::chain::{HeadEvent, Heads};
    use crate::chain_runtime::RuntimeFailure;
    use crate::durable::model::{DomainId, DurableTxEntry, HeadKind};
    use crate::durable::oracle::{CompletionOracle, DurableRegistry, LedgerView, PassScope};
    use crate::durable::testing::{FakeChain, GENESIS, block, engine_with, extrinsic, insert};
    use crate::test_support::wait_until;

    const TEST: DomainId = DomainId::from_static("test");

    /// Counts the passes that reached the domain, and proves its
    /// transactions completed from the `complete_from`th one on.
    struct CountingOracle {
        opened: AtomicUsize,
        complete_from: usize,
    }

    #[async_trait::async_trait]
    impl CompletionOracle for CountingOracle {
        fn chain(&self) -> H256 {
            GENESIS
        }

        async fn open_pass(
            &self,
            _transactions: &[DurableTxEntry],
            _ledger: &LedgerView,
            _heads: &Heads,
        ) -> Result<Box<dyn PassScope>, RuntimeFailure> {
            let opened = self.opened.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Box::new(Completes(opened >= self.complete_from)))
        }
    }

    struct Completes(bool);

    impl PassScope for Completes {
        fn proven_completed(&self, _tx: &DurableTxEntry, head: HeadKind) -> bool {
            self.0 && head == HeadKind::Finalized
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        engine: Arc<DurableTxEngine>,
        oracle: Arc<CountingOracle>,
        heads: mpsc::UnboundedSender<Result<HeadEvent, RuntimeFailure>>,
    }

    fn fixture(complete_from: usize) -> Fixture {
        let chain = FakeChain::new(130, 140);
        let heads = chain.script_heads();
        let oracle = Arc::new(CountingOracle {
            opened: AtomicUsize::new(0),
            complete_from,
        });
        let (dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, oracle.clone()),
        );
        insert(&engine.db, &TEST, extrinsic(1, 100, 64));
        Fixture {
            _dir: dir,
            engine,
            oracle,
            heads,
        }
    }

    fn run_in_background(
        engine: &Arc<DurableTxEngine>,
    ) -> std::thread::JoinHandle<Result<(), RecoveryError>> {
        let engine = engine.clone();
        std::thread::spawn(move || block_on(engine.run_until_settled()))
    }

    fn passes(fixture: &Fixture) -> usize {
        fixture.oracle.opened.load(Ordering::SeqCst)
    }

    /// Android: DurableRecoveryLoopTest `a settled ledger costs one pass`.
    #[test]
    fn a_ledger_settled_by_the_first_pass_costs_one_pass() {
        let fixture = fixture(1);
        fixture
            .heads
            .unbounded_send(Ok(HeadEvent::Finalized(block(131))))
            .unwrap();

        block_on(fixture.engine.run_until_settled()).unwrap();

        assert_eq!(passes(&fixture), 1);
    }

    /// Android: DurableRecoveryLoopTest `one pass per head until nothing is live`.
    #[test]
    fn one_pass_per_head_until_nothing_is_live() {
        let fixture = fixture(3);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        fixture
            .heads
            .unbounded_send(Ok(HeadEvent::Finalized(block(131))))
            .unwrap();
        wait_until(|| passes(&fixture) == 2, "a finalized head runs a pass");
        fixture
            .heads
            .unbounded_send(Ok(HeadEvent::Finalized(block(132))))
            .unwrap();

        assert_eq!((running.join().unwrap(), passes(&fixture)), (Ok(()), 3));
    }

    /// Android: DurableRecoveryLoopTest `a best head drives a pass just as a finalized one does`.
    #[test]
    fn a_best_head_drives_a_pass() {
        let fixture = fixture(2);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        fixture
            .heads
            .unbounded_send(Ok(HeadEvent::Best(block(141))))
            .unwrap();

        assert_eq!((running.join().unwrap(), passes(&fixture)), (Ok(()), 2));
    }

    /// A released watch hands its transaction back without waiting a block.
    #[test]
    fn a_nudge_drives_a_pass() {
        let fixture = fixture(2);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        fixture.engine.nudges.nudge();

        assert_eq!((running.join().unwrap(), passes(&fixture)), (Ok(()), 2));
    }

    /// An error item is one missed head; the subscription goes on.
    #[test]
    fn a_failed_head_read_is_skipped() {
        let fixture = fixture(2);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        fixture
            .heads
            .unbounded_send(Err(RuntimeFailure::host_failure("head", "glitch")))
            .unwrap();
        fixture
            .heads
            .unbounded_send(Ok(HeadEvent::Finalized(block(131))))
            .unwrap();

        assert_eq!((running.join().unwrap(), passes(&fixture)), (Ok(()), 2));
    }

    /// Android: DurableRecoveryLoopTest `a lost head subscription fails the loop so its host can retry`.
    #[test]
    fn a_head_subscription_that_ends_fails_the_loop() {
        let fixture = fixture(usize::MAX);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        drop(fixture.heads);

        assert!(matches!(
            running.join().unwrap(),
            Err(RecoveryError::HeadsLost { genesis, .. }) if genesis == GENESIS
        ));
    }

    #[test]
    fn a_head_subscription_that_cannot_open_fails_the_loop() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(
                TEST,
                Arc::new(crate::durable::oracle::Unobservable(GENESIS)),
            ),
        );
        insert(&engine.db, &TEST, extrinsic(1, 100, 64));

        assert!(matches!(
            block_on(engine.run_until_settled()),
            Err(RecoveryError::HeadsLost { .. })
        ));
    }

    /// Android: DurableRecoveryLoopTest `an unreadable ledger keeps the loop running rather than abandoning entries`.
    #[test]
    fn an_unreadable_ledger_keeps_the_loop_running() {
        let fixture = fixture(1);
        block_on(fixture.engine.db.close()).unwrap();

        let running = run_in_background(&fixture.engine);
        std::thread::sleep(Duration::from_millis(200));

        assert!(!running.is_finished());
    }
}
