//! The recovery pass: one evaluation of every live transaction no submission
//! watch owns.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use subxt::utils::H256;
use tracing::warn;

use super::DurableTxEngine;
use crate::chain_runtime::RuntimeFailure;
use crate::durable::dao;
use crate::durable::ladder::{PinnedView, RuleOutcome, evaluate_ladder};
use crate::durable::model::{DomainId, DurableTxEntry, DurableTxId, Verdict};
use crate::durable::oracle::{CompletionOracle, LedgerView};
use crate::durable::search::PinnedChain;
use crate::store::DbError;

/// Why one round of a domain decided nothing.
#[derive(Debug, thiserror::Error)]
enum RoundError {
    #[error(transparent)]
    Ledger(#[from] DbError),
    #[error(transparent)]
    Oracle(#[from] RuntimeFailure),
}

/// The live domains of one chain, each with its oracle.
type ChainDomains<'a> = Vec<(DomainId, &'a Arc<dyn CompletionOracle>)>;

impl DurableTxEngine {
    /// Decides what it can of every live transaction. At most one pass runs
    /// at a time; a call made while one runs returns at once. The ladder runs
    /// outside any write, and each verdict is a compare-and-set against the
    /// status it was derived from.
    pub async fn run_pass(&self) {
        let Some(_running) = self.pass_lock.try_lock() else {
            return;
        };
        let domains = match self.db.read(|conn| Ok(dao::live_domains(conn)?)).await {
            Ok(domains) => domains,
            Err(error) => return warn!(%error, "durable recovery pass could not read the ledger"),
        };
        for (genesis, domains) in self.by_chain(domains) {
            self.decide_chain(genesis, domains).await;
        }
    }

    /// Groups `domains` by the chain their oracle names, so each chain's
    /// heads are read once. A domain with no oracle has no chain and is
    /// skipped.
    fn by_chain(&self, domains: Vec<DomainId>) -> BTreeMap<H256, ChainDomains<'_>> {
        let mut by_chain: BTreeMap<H256, ChainDomains<'_>> = BTreeMap::new();
        for domain in domains {
            match self.registry.oracle(&domain) {
                Some(oracle) => by_chain
                    .entry(oracle.chain())
                    .or_default()
                    .push((domain, oracle)),
                None => warn!(
                    domain = domain.as_str(),
                    "durable recovery pass skips a domain with no oracle"
                ),
            }
        }
        by_chain
    }

    /// Pins the chain with `genesis` and decides each of its domains.
    async fn decide_chain(&self, genesis: H256, domains: ChainDomains<'_>) {
        let view = match PinnedChain::pin(&*self.heads, &*self.blocks, genesis).await {
            Ok(view) => view,
            Err(error) => {
                return warn!(?genesis, %error, "durable recovery pass could not read heads");
            }
        };
        for (domain, oracle) in domains {
            self.decide_domain(&domain, oracle.as_ref(), genesis, &view)
                .await;
        }
    }

    /// Two rounds: a verdict written in the first is exactly the evidence a
    /// predecessor's oracle may need in the second.
    async fn decide_domain(
        &self,
        domain: &DomainId,
        oracle: &dyn CompletionOracle,
        genesis: H256,
        view: &PinnedChain<'_>,
    ) {
        for _ in 0..2 {
            match self.decide_round(domain, oracle, genesis, view).await {
                Ok(0) => return,
                Ok(_) => continue,
                Err(error) => {
                    return warn!(domain = domain.as_str(), %error, "durable recovery round failed");
                }
            }
        }
    }

    /// Decides every decidable transaction of `domain` once. Returns how
    /// many verdicts it wrote.
    async fn decide_round(
        &self,
        domain: &DomainId,
        oracle: &dyn CompletionOracle,
        genesis: H256,
        view: &PinnedChain<'_>,
    ) -> Result<usize, RoundError> {
        let (ledger, decidable) = self.decidable(domain).await?;
        if decidable.is_empty() {
            return Ok(0);
        }
        let scope = oracle.open_pass(&decidable, &ledger, &view.heads()).await?;
        let canonical = self.recorded_canonicity(genesis, &decidable).await;

        let mut written = 0;
        for tx in &decidable {
            let outcome =
                evaluate_ladder(tx, scope.as_ref(), view, canonical.get(&tx.id).copied()).await;
            if let RuleOutcome::Decided(verdict) = outcome
                && self.write_if_changed(tx, verdict).await
            {
                written += 1;
            }
        }
        Ok(written)
    }

    /// The domain's ledger, and the transactions in it that await a verdict
    /// and no watch owns.
    async fn decidable(
        &self,
        domain: &DomainId,
    ) -> Result<(LedgerView, Vec<DurableTxEntry>), DbError> {
        let domain = domain.clone();
        let entries = self
            .db
            .read(move |conn| Ok(dao::domain_entries(conn, &domain)?))
            .await?;
        let decidable = entries
            .iter()
            .filter(|entry| entry.status.awaits_verdict() && !self.ownership.is_owned(entry.id))
            .cloned()
            .collect();
        Ok((LedgerView::new(entries), decidable))
    }

    /// Writes `verdict` unless it restates `tx`. Returns whether it wrote.
    async fn write_if_changed(&self, tx: &DurableTxEntry, verdict: Verdict) -> bool {
        if verdict.status == tx.status && verdict.success_detected_at == tx.success_detected_at {
            return false;
        }
        self.write_verdict(tx, verdict)
            .await
            .unwrap_or_else(|error| {
                warn!(id = tx.id.0, %error, "durable verdict write failed");
                false
            })
    }

    /// Whether each recorded success block is still canonical. A failed read
    /// leaves its transactions out, so Rule 0 stays undecided instead of
    /// discarding a record on a transport error.
    async fn recorded_canonicity(
        &self,
        genesis: H256,
        transactions: &[DurableTxEntry],
    ) -> HashMap<DurableTxId, bool> {
        let heights = transactions
            .iter()
            .filter_map(|tx| tx.success_detected_at.map(|block| block.number));
        let hashes = self.canonical_hashes(genesis, heights).await;
        transactions
            .iter()
            .filter_map(|tx| {
                let recorded = tx.success_detected_at?;
                // A chain shorter than the record no longer has its block.
                let hash = hashes.get(&recorded.number)?;
                Some((tx.id, *hash == Some(recorded.hash)))
            })
            .collect()
    }

    /// The canonical hash at each height, read once per distinct height.
    /// Heights whose read failed are missing.
    async fn canonical_hashes(
        &self,
        genesis: H256,
        heights: impl Iterator<Item = u64>,
    ) -> HashMap<u64, Option<H256>> {
        let mut heights: Vec<u64> = heights.collect();
        heights.sort_unstable();
        heights.dedup();
        futures::future::join_all(
            heights.into_iter().map(|number| async move {
                (number, self.blocks.block_hash(genesis, number).await)
            }),
        )
        .await
        .into_iter()
        .filter_map(|(number, read)| read.ok().map(|hash| (number, hash)))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::chain::{DispatchOutcome, HashAndNumber, Heads};
    use crate::chain_runtime::RuntimeFailure;
    use crate::durable::model::{DurableTxStatus, HeadKind};
    use crate::durable::oracle::{DurableRegistry, PassScope, Unobservable};
    use crate::durable::testing::{
        FakeChain, GENESIS, ScriptedScope, block, engine_with, extrinsic, insert,
    };
    use crate::store::Db;

    const TEST: DomainId = DomainId::from_static("test");

    fn status(db: &Db, id: DurableTxId) -> DurableTxStatus {
        block_on(db.read(move |conn| Ok(dao::status(conn, id)?)))
            .unwrap()
            .unwrap()
    }

    /// An oracle whose scope is computed from the ledger it is shown.
    struct ScriptedOracle<F> {
        scope: F,
        fails: bool,
    }

    impl<F> ScriptedOracle<F>
    where
        F: Fn(&LedgerView) -> Box<dyn PassScope> + Send + Sync,
    {
        fn new(scope: F) -> Arc<Self> {
            Arc::new(Self {
                scope,
                fails: false,
            })
        }
    }

    fn says(
        scope: ScriptedScope,
    ) -> Arc<ScriptedOracle<impl Fn(&LedgerView) -> Box<dyn PassScope> + Send + Sync>> {
        ScriptedOracle::new(move |_: &LedgerView| Box::new(scope) as Box<dyn PassScope>)
    }

    #[async_trait::async_trait]
    impl<F> CompletionOracle for ScriptedOracle<F>
    where
        F: Fn(&LedgerView) -> Box<dyn PassScope> + Send + Sync,
    {
        fn chain(&self) -> H256 {
            GENESIS
        }

        async fn open_pass(
            &self,
            _transactions: &[DurableTxEntry],
            ledger: &LedgerView,
            _heads: &Heads,
        ) -> Result<Box<dyn PassScope>, RuntimeFailure> {
            if self.fails {
                return Err(RuntimeFailure::host_failure("open_pass", "down"));
            }
            Ok((self.scope)(ledger))
        }
    }

    fn completed_at_finalized() -> ScriptedScope {
        ScriptedScope {
            completed_at_finalized: true,
            ..ScriptedScope::default()
        }
    }

    /// iOS: `A settled ledger does not pin a chain view`.
    #[test]
    fn a_settled_ledger_reads_no_chain() {
        let chain = FakeChain::new(150, 200);
        let registry = DurableRegistry::new().with_domain(TEST, says(completed_at_finalized()));
        let (_dir, engine, _timer) = engine_with(&chain, registry);

        block_on(engine.run_pass());

        assert_eq!(chain.state().heads_reads, 0);
    }

    /// iOS: `A decided transaction is written through the oracle's answer`.
    #[test]
    fn the_oracles_answer_is_written() {
        let chain = FakeChain::new(150, 200);
        let registry = DurableRegistry::new().with_domain(TEST, says(completed_at_finalized()));
        let (_dir, engine, _timer) = engine_with(&chain, registry);
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::FinalizedSuccess);
    }

    /// iOS: `A submission-owned transaction gets no verdict`.
    #[test]
    fn a_transaction_a_watch_owns_gets_no_verdict() {
        let chain = FakeChain::new(150, 200);
        let registry = DurableRegistry::new().with_domain(TEST, says(completed_at_finalized()));
        let (_dir, engine, _timer) = engine_with(&chain, registry);
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));
        engine
            .ownership
            .acquire(id, extrinsic(1, 100, 64).extrinsic.hash());

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
    }

    /// iOS: `A domain with no registered oracle is left alone`.
    #[test]
    fn a_domain_without_an_oracle_is_left_alone() {
        let chain = FakeChain::new(150, 200);
        let (_dir, engine, _timer) = engine_with(&chain, DurableRegistry::new());
        let tx = extrinsic(1, 100, 64);
        chain.include(120, tx.extrinsic.hash(), DispatchOutcome::Succeeded);
        let id = insert(&engine.db, &DomainId::from_static("orphan"), tx);

        block_on(engine.run_pass());

        assert_eq!(
            (status(&engine.db, id), chain.state().heads_reads),
            (DurableTxStatus::Pending, 0)
        );
    }

    /// Proves completion at the finalized head for the listed transactions.
    struct CompletedAtFinalized(Vec<DurableTxId>);

    impl PassScope for CompletedAtFinalized {
        fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
            head == HeadKind::Finalized && self.0.contains(&tx.id)
        }
    }

    /// iOS: `A second round lets a domain see what the first round wrote`.
    #[test]
    fn a_second_round_sees_what_the_first_wrote() {
        const PREDECESSOR: DurableTxId = DurableTxId(1);
        const SUCCESSOR: DurableTxId = DurableTxId(2);
        let chain = FakeChain::new(150, 200);
        // The predecessor's completion is only inferred from its successor's
        // finalized status, as coinage infers a minter from its consumer.
        let oracle = ScriptedOracle::new(|ledger: &LedgerView| {
            let mut completed = vec![SUCCESSOR];
            if ledger.status_of(SUCCESSOR) == Some(DurableTxStatus::FinalizedSuccess) {
                completed.push(PREDECESSOR);
            }
            Box::new(CompletedAtFinalized(completed)) as Box<dyn PassScope>
        });
        let (_dir, engine, _timer) =
            engine_with(&chain, DurableRegistry::new().with_domain(TEST, oracle));
        assert_eq!(
            [
                insert(&engine.db, &TEST, extrinsic(1, 100, 64)),
                insert(&engine.db, &TEST, extrinsic(2, 100, 64))
            ],
            [PREDECESSOR, SUCCESSOR]
        );

        block_on(engine.run_pass());

        assert_eq!(
            (
                status(&engine.db, PREDECESSOR),
                status(&engine.db, SUCCESSOR)
            ),
            (
                DurableTxStatus::FinalizedSuccess,
                DurableTxStatus::FinalizedSuccess
            )
        );
    }

    /// iOS: `Two domains on one chain share a single pinned view`.
    #[test]
    fn two_domains_on_one_chain_read_its_heads_once() {
        let chain = FakeChain::new(150, 200);
        let registry = DurableRegistry::new()
            .with_domain(DomainId::from_static("a"), Arc::new(Unobservable(GENESIS)))
            .with_domain(DomainId::from_static("b"), Arc::new(Unobservable(GENESIS)));
        let (_dir, engine, _timer) = engine_with(&chain, registry);
        insert(
            &engine.db,
            &DomainId::from_static("a"),
            extrinsic(1, 100, 64),
        );
        insert(
            &engine.db,
            &DomainId::from_static("b"),
            extrinsic(2, 100, 64),
        );

        block_on(engine.run_pass());

        assert_eq!(chain.state().heads_reads, 1);
    }

    /// iOS: `A pass that cannot pin writes nothing`.
    #[test]
    fn a_chain_whose_heads_cannot_be_read_gets_no_verdict() {
        let chain = FakeChain::new(150, 200);
        chain.state().heads_unavailable = true;
        let registry = DurableRegistry::new().with_domain(TEST, says(completed_at_finalized()));
        let (_dir, engine, _timer) = engine_with(&chain, registry);
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
    }

    /// iOS: `An oracle that fails to open leaves its domain untouched this pass`.
    #[test]
    fn an_oracle_that_fails_to_open_leaves_its_domain_untouched() {
        let chain = FakeChain::new(150, 200);
        let tx = extrinsic(1, 100, 64);
        chain.include(120, tx.extrinsic.hash(), DispatchOutcome::Succeeded);
        let oracle = Arc::new(ScriptedOracle {
            scope: |_: &LedgerView| Box::new(ScriptedScope::default()) as Box<dyn PassScope>,
            fails: true,
        });
        let (_dir, engine, _timer) =
            engine_with(&chain, DurableRegistry::new().with_domain(TEST, oracle));
        let id = insert(&engine.db, &TEST, tx);

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
    }

    /// iOS: `Recorded canonicality is read from the view, once per recorded height`.
    /// Android: ReorgScenariosTest `Rule 0 clears and demotes when the recorded block is gone`.
    #[test]
    fn a_record_whose_block_was_reorged_out_is_demoted_and_cleared() {
        let chain = FakeChain::new(150, 200);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
        );
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));
        record_success(&engine.db, id, block(160));
        chain.state().reorged.insert(160, H256::repeat_byte(0x99));

        block_on(engine.run_pass());

        let entry = block_on(engine.db.read(move |conn| Ok(dao::entry(conn, id)?)))
            .unwrap()
            .unwrap();
        assert_eq!(
            (entry.status, entry.success_detected_at),
            (DurableTxStatus::Pending, None)
        );
    }

    /// Android: ReorgScenariosTest `an unreachable node does not withdraw a detected success`.
    #[test]
    fn an_unreadable_record_height_keeps_the_success() {
        let chain = FakeChain::new(150, 200);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
        );
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));
        record_success(&engine.db, id, block(160));
        chain.state().unreadable_heights.insert(160);

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::PendingSuccess);
    }

    /// Android: ReorgScenariosTest `a reorg shortening the chain past the record clears it`.
    #[test]
    fn a_chain_shorter_than_the_record_clears_it() {
        let chain = FakeChain::new(150, 155);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
        );
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));
        record_success(&engine.db, id, block(160));

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
    }

    /// The search decides a transaction no oracle can see, end to end over
    /// the chain's own blocks.
    #[test]
    fn a_transaction_found_in_a_finalized_block_finalizes() {
        let chain = FakeChain::new(150, 200);
        let tx = extrinsic(1, 100, 64);
        chain.include(120, tx.extrinsic.hash(), DispatchOutcome::Succeeded);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
        );
        let id = insert(&engine.db, &TEST, tx);

        block_on(engine.run_pass());

        let entry = block_on(engine.db.read(move |conn| Ok(dao::entry(conn, id)?)))
            .unwrap()
            .unwrap();
        assert_eq!(
            (entry.status, entry.success_detected_at),
            (DurableTxStatus::FinalizedSuccess, Some(block(120)))
        );
    }

    /// Android: SystemScenariosTest `best-chain height alone yields no terminal verdict while finality stalls`.
    #[test]
    fn best_height_alone_never_expires_a_transaction() {
        let chain = FakeChain::new(150, 400);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
        );
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::Pending);
    }

    #[test]
    fn absence_over_a_closed_window_fails_the_transaction() {
        let chain = FakeChain::new(200, 210);
        let (_dir, engine, _timer) = engine_with(
            &chain,
            DurableRegistry::new().with_domain(TEST, Arc::new(Unobservable(GENESIS))),
        );
        let id = insert(&engine.db, &TEST, extrinsic(1, 100, 64));

        block_on(engine.run_pass());

        assert_eq!(status(&engine.db, id), DurableTxStatus::Failure);
    }

    fn record_success(db: &Db, id: DurableTxId, at: HashAndNumber) {
        block_on(db.write(move |tx| {
            let observed = dao::entry(tx, id)?.unwrap();
            dao::compare_and_set(
                tx,
                &observed,
                &crate::durable::model::Verdict {
                    status: DurableTxStatus::PendingSuccess,
                    success_detected_at: Some(at),
                    failure: None,
                },
            )?;
            Ok(())
        }))
        .unwrap();
    }
}
