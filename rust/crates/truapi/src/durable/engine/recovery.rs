//! The recovery loop: passes on every head and wake until nothing is live.

use std::collections::{BTreeMap, BTreeSet};

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
use crate::durable::model::DomainId;

/// What wakes the loop.
enum Trigger {
    /// Something may have changed: run another pass.
    Pass,
    /// The loop can no longer follow the new blocks of the chain with this
    /// genesis, for the given reason.
    ChainLost(H256, String),
}

type Triggers = BoxStream<'static, Trigger>;

/// Chains whose new blocks the loop can no longer follow, with the reason.
#[derive(Default)]
struct LostChains(BTreeMap<H256, String>);

impl LostChains {
    fn contains(&self, genesis: H256) -> bool {
        self.0.contains_key(&genesis)
    }

    /// Fails for the first lost chain that still has live transactions, so
    /// the host retries for it.
    fn result_for(&self, live_chains: &BTreeSet<H256>) -> Result<(), RecoveryError> {
        let lost_with_live = self
            .0
            .iter()
            .find(|(genesis, _)| live_chains.contains(genesis));
        match lost_with_live {
            Some((genesis, reason)) => Err(RecoveryError::HeadsLost {
                genesis: *genesis,
                reason: reason.clone(),
            }),
            None => Ok(()),
        }
    }
}

/// Why recovery stopped while transactions were still live. The host retries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecoveryError {
    /// The runtime has no core database, so there is nothing to recover.
    #[error("no core database configured")]
    NotConfigured,
    /// A chain's head subscription failed or ended while transactions on it
    /// were still pending, so nothing would drive their next pass.
    #[error("head events of chain {genesis:?} unavailable: {reason}")]
    HeadsLost {
        /// Genesis hash of the chain.
        genesis: H256,
        /// What happened.
        reason: String,
    },
}

impl DurableTxEngine {
    /// Runs recovery passes until no pending transaction is left whose outcome
    /// recovery can look up.
    ///
    /// Recovery looks up a transaction's outcome on chain only when no sending
    /// task is following it and its domain has a registered oracle.
    ///
    /// The first pass runs as soon as this is called. After that, a pass runs on
    /// each new block of a registered chain, and whenever a sending task stops
    /// following a transaction. Anything that arrives while a pass runs is
    /// handled by the next pass.
    ///
    /// If the ledger cannot be read, recovery keeps running. When it can no
    /// longer follow a chain's new blocks, it keeps serving the other chains,
    /// then fails with [`RecoveryError::HeadsLost`] if that chain still has
    /// pending transactions. Failing or being dropped leaves every pending
    /// transaction in the ledger for the next run.
    pub async fn run_until_settled(&self) -> Result<(), RecoveryError> {
        let wakes = self.recovery_wakes.subscribe();
        let mut lost = LostChains::default();
        self.run_pass().await;
        if let Some(result) = self.settled(&lost).await {
            return result;
        }
        let mut triggers = self.triggers(wakes).await;
        loop {
            next_trigger(&mut triggers, &mut lost).await;
            self.run_pass().await;
            if let Some(result) = self.settled(&lost).await {
                return result;
            }
        }
    }

    /// Every head of every registered chain, and every wake. Subscribed only
    /// after the first pass, so a ledger that is already settled needs no
    /// head at all.
    async fn triggers(&self, wakes: mpsc::Receiver<()>) -> Triggers {
        let mut sources = vec![wakes.map(|()| Trigger::Pass).boxed()];
        for genesis in self.registry.chains() {
            sources.push(self.head_triggers(genesis).await);
        }
        stream::select_all(sources).boxed()
    }

    /// One trigger per head of the chain with `genesis`, ending in
    /// [`Trigger::ChainLost`] when the node's head subscription cannot open
    /// or ends.
    async fn head_triggers(&self, genesis: H256) -> Triggers {
        let heads = match self.heads.head_events(genesis).await {
            Ok(heads) => heads,
            Err(error) => return chain_lost(genesis, error.to_string()),
        };
        heads
            .filter_map(move |head| ready(as_trigger(genesis, head)))
            .chain(chain_lost(genesis, "head events ended".into()))
            .boxed()
    }

    /// How recovery ends, or `None` while a transaction some registered
    /// domain can decide is live on a chain recovery still follows. Rows of a
    /// domain without an oracle stay as they are: no pass could decide them.
    /// An unreadable ledger counts as live: abandoning transactions is far
    /// worse than one more pass.
    async fn settled(&self, lost: &LostChains) -> Option<Result<(), RecoveryError>> {
        let live_chains = match self.db.read(dao::live_domains).await {
            Ok(domains) => self.chains_of(&domains),
            Err(error) => {
                warn!(%error, "durable recovery could not read the ledger");
                return None;
            }
        };
        let followed_live = live_chains.iter().any(|genesis| !lost.contains(*genesis));
        (!followed_live).then(|| lost.result_for(&live_chains))
    }

    /// The chains `domains` live on, skipping domains without an oracle.
    fn chains_of(&self, domains: &[DomainId]) -> BTreeSet<H256> {
        domains
            .iter()
            .filter_map(|domain| self.registry.oracle(domain))
            .map(|oracle| oracle.chain())
            .collect()
    }
}

/// A head read is one trigger. A failed read is one missed head, not a
/// reason to stop: later heads still follow.
fn as_trigger(genesis: H256, head: Result<HeadEvent, RuntimeFailure>) -> Option<Trigger> {
    match head {
        Ok(_) => Some(Trigger::Pass),
        Err(error) => {
            warn!(?genesis, %error, "durable recovery missed a head");
            None
        }
    }
}

fn chain_lost(genesis: H256, reason: String) -> Triggers {
    stream::once(ready(Trigger::ChainLost(genesis, reason))).boxed()
}

/// Waits for the next trigger, then takes every one already queued behind
/// it, so heads that arrived during a pass cost one more pass, not one each.
async fn next_trigger(triggers: &mut Triggers, lost: &mut LostChains) {
    let first = triggers.next().await;
    first.into_iter().for_each(|trigger| record(trigger, lost));
    while let Some(Some(trigger)) = triggers.next().now_or_never() {
        record(trigger, lost);
    }
}

fn record(trigger: Trigger, lost: &mut LostChains) {
    if let Trigger::ChainLost(genesis, reason) = trigger {
        warn!(?genesis, %reason, "durable recovery lost a chain's heads");
        lost.0.insert(genesis, reason);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::executor::block_on;

    use super::*;
    use crate::chain::{HeadEvent, Heads};
    use crate::chain_runtime::RuntimeFailure;
    use crate::durable::model::{DomainId, DurableTxEntry, DurableTxStatus, HeadKind};
    use crate::durable::oracle::{CompletionOracle, DurableRegistry, LedgerView, PassScope};
    use crate::durable::testing::{FakeChain, GENESIS, block, engine_with, extrinsic, insert};
    use crate::test_support::wait_until;

    fn test_domain() -> DomainId {
        DomainId::new("test")
    }

    /// Counts the passes that reached the domain, and proves its
    /// transactions completed from the `complete_from`th one on.
    struct CountingOracle {
        chain: H256,
        opened: AtomicUsize,
        complete_from: usize,
    }

    impl CountingOracle {
        fn new(chain: H256, complete_from: usize) -> Arc<Self> {
            Arc::new(Self {
                chain,
                opened: AtomicUsize::new(0),
                complete_from,
            })
        }
    }

    #[async_trait::async_trait]
    impl CompletionOracle for CountingOracle {
        fn chain(&self) -> H256 {
            self.chain
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
        let oracle = CountingOracle::new(GENESIS, complete_from);
        let (dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(test_domain(), oracle.clone()),
        );
        insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));
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

    /// Sorts before [`GENESIS`], so the fake chain hands it the first
    /// scripted head stream.
    const OTHER_GENESIS: H256 = H256([0x01; 32]);

    /// One chain's node connection closed while a transaction on another
    /// chain was still waiting for its verdict. The healthy chain keeps
    /// recovery running until it settles; the host then retries for the
    /// lost one.
    #[test]
    fn a_lost_chain_does_not_stop_recovery_on_another_chain() {
        let chain = FakeChain::new(130, 140);
        let _other_heads = chain.script_heads();
        let lost_heads = chain.script_heads();
        let other_oracle = CountingOracle::new(OTHER_GENESIS, 2);
        let registry = DurableRegistry::new()
            .with_domain(
                DomainId::new("lost"),
                CountingOracle::new(GENESIS, usize::MAX),
            )
            .with_domain(DomainId::new("other"), other_oracle.clone());
        let (_dir, engine, _timer) = engine_with(&chain, registry);
        insert(&engine.db, &DomainId::new("lost"), extrinsic(1, 100, 64));
        let other = insert(&engine.db, &DomainId::new("other"), extrinsic(2, 100, 64));
        let running = run_in_background(&engine);
        wait_until(
            || other_oracle.opened.load(Ordering::SeqCst) == 1,
            "the first pass runs at once",
        );

        drop(lost_heads);

        assert!(matches!(
            running.join().unwrap(),
            Err(RecoveryError::HeadsLost { genesis, .. }) if genesis == GENESIS
        ));
        assert_eq!(
            block_on(engine.status(other)).unwrap(),
            Some(DurableTxStatus::FinalizedSuccess)
        );
    }

    /// A chain whose node connection closed has nothing waiting on it, so
    /// recovery has nothing to retry there.
    #[test]
    fn a_lost_chain_with_nothing_live_does_not_fail_recovery() {
        let chain = FakeChain::new(130, 140);
        let _other_heads = chain.script_heads();
        let lost_heads = chain.script_heads();
        let other_oracle = CountingOracle::new(OTHER_GENESIS, 2);
        let registry = DurableRegistry::new()
            .with_domain(
                DomainId::new("lost"),
                CountingOracle::new(GENESIS, usize::MAX),
            )
            .with_domain(DomainId::new("other"), other_oracle.clone());
        let (_dir, engine, _timer) = engine_with(&chain, registry);
        insert(&engine.db, &DomainId::new("other"), extrinsic(2, 100, 64));
        let running = run_in_background(&engine);
        wait_until(
            || other_oracle.opened.load(Ordering::SeqCst) == 1,
            "the first pass runs at once",
        );

        drop(lost_heads);

        assert_eq!(running.join().unwrap(), Ok(()));
    }

    #[test]
    fn a_head_subscription_that_cannot_open_fails_the_loop() {
        let chain = FakeChain::new(130, 140);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(
                test_domain(),
                Arc::new(crate::durable::oracle::Unobservable(GENESIS)),
            ),
        );
        insert(&engine.db, &test_domain(), extrinsic(1, 100, 64));

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

        assert!(block_on(fixture.engine.settled(&LostChains::default())).is_none());
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
                test_domain(),
                Arc::new(crate::durable::oracle::Unobservable(GENESIS)),
            ),
        );
        insert(&engine.db, &DomainId::new("orphan"), extrinsic(1, 100, 64));

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
                test_domain(),
                Arc::new(crate::durable::oracle::Unobservable(GENESIS)),
            ),
        );

        assert_eq!(block_on(engine.run_until_settled()), Ok(()));
    }
}
