//! The recovery loop: passes on every head and wake until nothing is live.

use futures::FutureExt;
use futures::channel::mpsc;
use futures::future::ready;
use futures::stream::{self, BoxStream, StreamExt};
use subxt::utils::H256;
use tracing::warn;

use super::DurableTxEngine;
use crate::chain::HeadEvent;
use crate::chain_runtime::RuntimeFailure;
use crate::durable::dao;

/// What wakes the loop for another pass. An `Err` stops it.
type Triggers = BoxStream<'static, Result<(), RecoveryError>>;

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

impl DurableTxEngine {
    /// Runs recovery passes until no transaction a registered domain can
    /// decide is live: one at once, then one per new head of any registered
    /// chain and per wake, collapsing whatever arrives while a pass runs. An
    /// unreadable ledger counts as live. Fails when a head subscription does;
    /// dropping the future stops it.
    pub async fn run_until_settled(&self) -> Result<(), RecoveryError> {
        let wakes = self.recovery_wakes.subscribe();
        self.run_pass().await;
        if !self.has_live().await {
            return Ok(());
        }
        let mut triggers = self.triggers(wakes).await?;
        loop {
            next_trigger(&mut triggers).await?;
            self.run_pass().await;
            if !self.has_live().await {
                return Ok(());
            }
        }
    }

    /// Every head of every registered chain, and every wake. Subscribed only
    /// after the first pass, so a ledger that is already settled needs no
    /// head at all.
    async fn triggers(&self, wakes: mpsc::Receiver<()>) -> Result<Triggers, RecoveryError> {
        let mut sources = vec![wakes.map(Ok).boxed()];
        for genesis in self.registry.chains() {
            sources.push(self.head_triggers(genesis).await?);
        }
        Ok(stream::select_all(sources).boxed())
    }

    /// One trigger per head of the chain with `genesis`, ending in an error
    /// when the node's head subscription ends.
    async fn head_triggers(&self, genesis: H256) -> Result<Triggers, RecoveryError> {
        let heads = self
            .heads
            .head_events(genesis)
            .await
            .map_err(|error| heads_lost(genesis, error.to_string()))?;
        let ended =
            stream::once(async move { Err(heads_lost(genesis, "head events ended".into())) });
        Ok(heads
            .filter_map(move |head| ready(as_trigger(genesis, head)))
            .chain(ended)
            .boxed())
    }

    /// Whether a transaction some registered domain can decide is live. Rows
    /// of a domain without an oracle stay as they are: no pass could decide
    /// them. An unreadable ledger counts as live: abandoning transactions is
    /// far worse than one more pass.
    async fn has_live(&self) -> bool {
        match self.db.read(dao::live_domains).await {
            Ok(domains) => domains
                .iter()
                .any(|domain| self.registry.oracle(domain).is_some()),
            Err(error) => {
                warn!(%error, "durable recovery could not read the ledger");
                true
            }
        }
    }
}

/// A head read is one trigger. A failed read is one missed head, not a
/// reason to stop: later heads still follow.
fn as_trigger(
    genesis: H256,
    head: Result<HeadEvent, RuntimeFailure>,
) -> Option<Result<(), RecoveryError>> {
    match head {
        Ok(_) => Some(Ok(())),
        Err(error) => {
            warn!(?genesis, %error, "durable recovery missed a head");
            None
        }
    }
}

fn heads_lost(genesis: H256, reason: String) -> RecoveryError {
    RecoveryError::HeadsLost { genesis, reason }
}

/// Waits for the next trigger, then takes every one already queued behind
/// it, so heads that arrived during a pass cost one more pass, not one each.
async fn next_trigger(triggers: &mut Triggers) -> Result<(), RecoveryError> {
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

    /// The app was killed after its last transaction was finalized but before
    /// its row was updated; the launch pass settles it and recovery stops
    /// without waiting for a head.
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

        assert_eq!(running.join().unwrap(), Ok(()));
        assert_eq!(passes(&fixture), 3);
    }

    /// Pre-finality success is read at the best head, so a new best block can
    /// decide a transaction several blocks before finality does.
    #[test]
    fn a_best_head_drives_a_pass() {
        let fixture = fixture(2);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        fixture
            .heads
            .unbounded_send(Ok(HeadEvent::Best(block(141))))
            .unwrap();

        assert_eq!(running.join().unwrap(), Ok(()));
        assert_eq!(passes(&fixture), 2);
    }

    /// A released watch hands its transaction back without waiting a block.
    #[test]
    fn a_nudge_drives_a_pass() {
        let fixture = fixture(2);
        let running = run_in_background(&fixture.engine);
        wait_until(|| passes(&fixture) == 1, "the first pass runs at once");

        fixture.engine.recovery_wakes.wake();

        assert_eq!(running.join().unwrap(), Ok(()));
        assert_eq!(passes(&fixture), 2);
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

        assert_eq!(running.join().unwrap(), Ok(()));
        assert_eq!(passes(&fixture), 2);
    }

    /// The app went to background and the node connection closed. Without heads
    /// nothing would drive the next pass, so the loop fails and the host
    /// retries it.
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

    /// Abandoning live transactions because the ledger could not be read once
    /// is far worse than one more pass.
    #[test]
    fn an_unreadable_ledger_counts_as_live() {
        let fixture = fixture(1);
        block_on(fixture.engine.db.close()).unwrap();

        assert!(block_on(fixture.engine.has_live()));
    }

    /// Nothing could decide a domain without an oracle, so its rows must not
    /// keep recovery running.
    #[test]
    fn live_rows_no_oracle_can_decide_do_not_keep_recovery_running() {
        let chain = FakeChain::new(130, 140);
        chain.script_heads();
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(
                TEST,
                Arc::new(crate::durable::oracle::Unobservable(GENESIS)),
            ),
        );
        insert(
            &engine.db,
            &DomainId::from_static("orphan"),
            extrinsic(1, 100, 64),
        );

        let running = run_in_background(&engine);

        wait_until(|| running.is_finished(), "recovery returns");
        assert_eq!(running.join().unwrap(), Ok(()));
    }

    /// The device is offline at launch with nothing live. Recovery must finish
    /// rather than fail on a head subscription it never needed, or the host
    /// would retry it forever.
    #[test]
    fn a_settled_ledger_needs_no_head_subscription() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(
                TEST,
                Arc::new(crate::durable::oracle::Unobservable(GENESIS)),
            ),
        );

        assert_eq!(block_on(engine.run_until_settled()), Ok(()));
    }
}
