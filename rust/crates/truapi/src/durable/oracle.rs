//! How a domain tells the engine what it can see on chain.

use std::collections::HashMap;
use std::sync::Arc;

use subxt::utils::H256;

use super::model::{DomainId, DurableTxEntry, DurableTxId, DurableTxStatus, HeadKind};
use crate::chain::{HashAndNumber, Heads};
use crate::chain_runtime::RuntimeFailure;

/// A domain's answer to the two questions the completion ladder asks of it.
///
/// Neither answer ever becomes a verdict by being absent: a transaction
/// neither proven completed nor proven not completed falls through to the
/// block-body search.
#[async_trait::async_trait]
pub trait CompletionOracle: Send + Sync {
    /// Genesis hash of the chain this domain's transactions live on.
    fn chain(&self) -> H256;

    /// Every chain read the pass needs, for every transaction in it, at
    /// `heads`. The scope does not read the chain, so the reads are batched.
    /// An error leaves the domain undecided until the next pass.
    async fn open_pass(
        &self,
        transactions: &[DurableTxEntry],
        ledger: &LedgerView,
        heads: &Heads,
    ) -> Result<Box<dyn PassScope>, RuntimeFailure>;
}

/// What a domain proved about its transactions for one pass.
pub trait PassScope: Send + Sync {
    /// Positive proof `tx` took effect at `head`. Safe to claim at the best
    /// head: a reorg demotes it again.
    fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool;

    /// Positive proof `tx` has not taken effect at `head`, and cannot have
    /// taken effect and been undone since. Past the era, at the finalized
    /// head, this fails the transaction for good, so claim it only for an
    /// effect that is monotone and written by this transaction alone.
    fn proven_not_completed(&self, _tx: &DurableTxEntry, _head: HeadKind) -> bool {
        false
    }
}

/// Every transaction of one domain as read at the start of a pass, so an
/// oracle can infer one transaction's completion from another's status.
pub struct LedgerView {
    entries: Vec<DurableTxEntry>,
}

impl LedgerView {
    /// A view of these entries.
    pub fn new(entries: Vec<DurableTxEntry>) -> Self {
        Self { entries }
    }

    /// Every entry of the domain, in registration order.
    pub fn entries(&self) -> &[DurableTxEntry] {
        &self.entries
    }

    /// The status of `id`, when it belongs to the domain.
    pub fn status_of(&self, id: DurableTxId) -> Option<DurableTxStatus> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.status)
    }
}

/// A domain whose transactions each produce one observation that, once true,
/// stays true and that nothing else can make true, such as a value appended
/// to a set only this app writes. Wrapped in [`Monotone`] it gets the whole
/// ladder from one batched read per head.
#[async_trait::async_trait]
pub trait MonotoneEffect: Send + Sync {
    /// Genesis hash of the chain the effects live on.
    fn chain(&self) -> H256;

    /// Whether each transaction's effect is visible at `at`. A transaction
    /// missing from the result is a read that did not answer and decides
    /// nothing; `false` claims the effect is absent.
    async fn effects_at(
        &self,
        transactions: &[DurableTxEntry],
        at: HashAndNumber,
    ) -> Result<HashMap<DurableTxId, bool>, RuntimeFailure>;
}

/// [`CompletionOracle`] over a [`MonotoneEffect`].
pub struct Monotone<E>(pub E);

#[async_trait::async_trait]
impl<E: MonotoneEffect> CompletionOracle for Monotone<E> {
    fn chain(&self) -> H256 {
        self.0.chain()
    }

    async fn open_pass(
        &self,
        transactions: &[DurableTxEntry],
        _ledger: &LedgerView,
        heads: &Heads,
    ) -> Result<Box<dyn PassScope>, RuntimeFailure> {
        Ok(Box::new(MonotoneScope {
            at_finalized: self.0.effects_at(transactions, heads.finalized).await?,
            at_best: self.0.effects_at(transactions, heads.best).await?,
        }))
    }
}

struct MonotoneScope {
    at_finalized: HashMap<DurableTxId, bool>,
    at_best: HashMap<DurableTxId, bool>,
}

impl MonotoneScope {
    fn effect(&self, tx: &DurableTxEntry, head: HeadKind) -> Option<bool> {
        match head {
            HeadKind::Finalized => self.at_finalized.get(&tx.id).copied(),
            HeadKind::Best => self.at_best.get(&tx.id).copied(),
        }
    }
}

impl PassScope for MonotoneScope {
    fn proven_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
        self.effect(tx, head) == Some(true)
    }

    fn proven_not_completed(&self, tx: &DurableTxEntry, head: HeadKind) -> bool {
        self.effect(tx, head) == Some(false)
    }
}

/// [`CompletionOracle`] for a domain that cannot read its effects: its
/// transactions are decided by their recorded inclusion and the body search
/// alone, which is correct, only slower.
pub struct Unobservable(pub H256);

#[async_trait::async_trait]
impl CompletionOracle for Unobservable {
    fn chain(&self) -> H256 {
        self.0
    }

    async fn open_pass(
        &self,
        _transactions: &[DurableTxEntry],
        _ledger: &LedgerView,
        _heads: &Heads,
    ) -> Result<Box<dyn PassScope>, RuntimeFailure> {
        Ok(Box::new(ProvesNothing))
    }
}

struct ProvesNothing;

impl PassScope for ProvesNothing {
    fn proven_completed(&self, _tx: &DurableTxEntry, _head: HeadKind) -> bool {
        false
    }
}

/// The oracle of every domain the engine serves. A domain without one cannot
/// register transactions.
#[derive(Default)]
pub struct DurableRegistry {
    oracles: HashMap<DomainId, Arc<dyn CompletionOracle>>,
}

impl DurableRegistry {
    /// A registry with no domains.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `domain`, decided by `oracle`.
    ///
    /// # Panics
    ///
    /// When `domain` is already registered: two oracles for one domain is a
    /// wiring bug.
    pub fn with_domain(mut self, domain: DomainId, oracle: Arc<dyn CompletionOracle>) -> Self {
        let duplicate = domain.clone();
        assert!(
            self.oracles.insert(domain, oracle).is_none(),
            "durable domain {} registered twice",
            duplicate.as_str()
        );
        self
    }

    /// The oracle of `domain`.
    pub fn oracle(&self, domain: &DomainId) -> Option<&Arc<dyn CompletionOracle>> {
        self.oracles.get(domain)
    }

    /// Genesis hashes of every chain a registered domain lives on.
    pub fn chains(&self) -> Vec<H256> {
        let mut chains: Vec<H256> = self.oracles.values().map(|oracle| oracle.chain()).collect();
        chains.sort();
        chains.dedup();
        chains
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::executor::block_on;

    use super::*;
    use crate::durable::ladder::{RuleOutcome, SearchResult, evaluate_ladder};
    use crate::durable::testing::{FakeView, entry};

    const BIRTH: u64 = 100;
    const PERIOD: u64 = 64;
    const DEATH: u64 = BIRTH + PERIOD;
    const FINALIZED: u64 = 130;
    const BEST: u64 = FINALIZED + 10;

    /// The reference a new domain copies: a set the app appends to, read once
    /// per head.
    struct ListMembership {
        present_at: HashMap<u64, HashSet<DurableTxId>>,
        unreadable: HashSet<u64>,
        reads: AtomicUsize,
    }

    impl ListMembership {
        fn new(present_at: &[(u64, DurableTxId)]) -> Self {
            let mut sets: HashMap<u64, HashSet<DurableTxId>> = HashMap::new();
            for (number, id) in present_at {
                sets.entry(*number).or_default().insert(*id);
            }
            Self {
                present_at: sets,
                unreadable: HashSet::new(),
                reads: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl MonotoneEffect for ListMembership {
        fn chain(&self) -> H256 {
            H256::zero()
        }

        async fn effects_at(
            &self,
            transactions: &[DurableTxEntry],
            at: HashAndNumber,
        ) -> Result<HashMap<DurableTxId, bool>, RuntimeFailure> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.unreadable.contains(&at.number) {
                return Ok(HashMap::new());
            }
            let present = self.present_at.get(&at.number);
            Ok(transactions
                .iter()
                .map(|tx| (tx.id, present.is_some_and(|set| set.contains(&tx.id))))
                .collect())
        }
    }

    const ID: DurableTxId = DurableTxId(1);

    fn evaluate(oracle: ListMembership, finalized: u64, search: SearchResult) -> DurableTxStatus {
        let transactions = vec![entry(ID, BIRTH, PERIOD)];
        let view = FakeView::new(finalized, search);
        block_on(async {
            let scope = Monotone(oracle)
                .open_pass(
                    &transactions,
                    &LedgerView::new(transactions.clone()),
                    &crate::durable::ladder::PinnedView::heads(&view),
                )
                .await
                .unwrap();
            match evaluate_ladder(&transactions[0], scope.as_ref(), &view, None).await {
                RuleOutcome::Decided(verdict) => verdict.status,
                RuleOutcome::Undecided => panic!("expected a decision"),
            }
        })
    }

    const ABSENT: SearchResult = SearchResult::NotFound {
        whole_range_read: true,
    };

    /// Android: `a value present at the finalized head finalizes`.
    #[test]
    fn a_value_present_at_finalized_finalizes() {
        assert_eq!(
            evaluate(ListMembership::new(&[(FINALIZED, ID)]), FINALIZED, ABSENT),
            DurableTxStatus::FinalizedSuccess
        );
    }

    /// Android: `a value present only at the best head is optimistic success`.
    #[test]
    fn a_value_present_only_at_best_is_optimistic_success() {
        assert_eq!(
            evaluate(ListMembership::new(&[(BEST, ID)]), FINALIZED, ABSENT),
            DurableTxStatus::PendingSuccess
        );
    }

    /// Android: `an absent value after mortality fails the transaction`.
    #[test]
    fn an_absent_value_after_the_era_fails() {
        assert_eq!(
            evaluate(ListMembership::new(&[]), DEATH + 1, ABSENT),
            DurableTxStatus::Failure
        );
    }

    /// Android: `an absent value before mortality keeps the transaction pending`.
    #[test]
    fn an_absent_value_before_the_era_ends_stays_pending() {
        assert_eq!(
            evaluate(ListMembership::new(&[]), BIRTH + 1, ABSENT),
            DurableTxStatus::Pending
        );
    }

    /// Android: `an unreadable list never fails the transaction`.
    #[test]
    fn an_unreadable_list_never_fails_the_transaction() {
        let mut oracle = ListMembership::new(&[]);
        oracle.unreadable = HashSet::from([DEATH + 1, DEATH + 11]);

        assert_eq!(
            evaluate(
                oracle,
                DEATH + 1,
                SearchResult::NotFound {
                    whole_range_read: false
                }
            ),
            DurableTxStatus::Pending
        );
    }

    /// Android: `an unreadable list still finalizes when the search finds the transaction`.
    #[test]
    fn an_unreadable_list_still_finalizes_when_the_search_finds_it() {
        let mut oracle = ListMembership::new(&[]);
        oracle.unreadable = HashSet::from([DEATH + 1, DEATH + 11]);

        assert_eq!(
            evaluate(
                oracle,
                DEATH + 1,
                SearchResult::Found {
                    block: crate::durable::testing::block(110),
                    outcome: Some(crate::chain::DispatchOutcome::Succeeded),
                }
            ),
            DurableTxStatus::FinalizedSuccess
        );
    }

    /// Android: `the list is read once per head no matter how many transactions are in flight`.
    #[test]
    fn the_effect_is_read_once_per_head_however_many_transactions() {
        let oracle = Monotone(ListMembership::new(&[]));
        let transactions: Vec<_> = (1..=25)
            .map(|id| entry(DurableTxId(id), BIRTH, PERIOD))
            .collect();
        let view = FakeView::new(FINALIZED, ABSENT);

        block_on(oracle.open_pass(
            &transactions,
            &LedgerView::new(transactions.clone()),
            &crate::durable::ladder::PinnedView::heads(&view),
        ))
        .unwrap();

        assert_eq!(oracle.0.reads.load(Ordering::SeqCst), 2);
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn a_domain_registered_twice_is_a_wiring_bug() {
        let oracle: Arc<dyn CompletionOracle> = Arc::new(Unobservable(H256::zero()));
        let _ = DurableRegistry::new()
            .with_domain(DomainId::from_static("test"), oracle.clone())
            .with_domain(DomainId::from_static("test"), oracle);
    }
}
