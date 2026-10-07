//! [`Monotone`]: the ladder for a domain whose effects, once visible, stay
//! visible.

use std::collections::HashMap;

use subxt::utils::H256;

use super::{CompletionOracle, LedgerView, PassScope};
use crate::chain::{HashAndNumber, Heads};
use crate::chain_runtime::RuntimeFailure;
use crate::durable::model::{DurableTxEntry, DurableTxId, HeadKind};

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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::executor::block_on;

    use super::*;
    use crate::durable::ladder::{RuleOutcome, SearchResult, evaluate_ladder};
    use crate::durable::model::DurableTxStatus;
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

    #[test]
    fn a_value_present_at_finalized_finalizes() {
        assert_eq!(
            evaluate(ListMembership::new(&[(FINALIZED, ID)]), FINALIZED, ABSENT),
            DurableTxStatus::FinalizedSuccess
        );
    }

    /// What the block search cannot give: a verdict above the finalized head.
    #[test]
    fn a_value_present_only_at_best_is_optimistic_success() {
        assert_eq!(
            evaluate(ListMembership::new(&[(BEST, ID)]), FINALIZED, ABSENT),
            DurableTxStatus::PendingSuccess
        );
    }

    #[test]
    fn an_absent_value_after_the_era_fails() {
        assert_eq!(
            evaluate(ListMembership::new(&[]), DEATH + 1, ABSENT),
            DurableTxStatus::Failure
        );
    }

    /// The transaction can still be included until its era ends.
    #[test]
    fn an_absent_value_before_the_era_ends_stays_pending() {
        assert_eq!(
            evaluate(ListMembership::new(&[]), BIRTH + 1, ABSENT),
            DurableTxStatus::Pending
        );
    }

    /// A failed read must not look like an empty list, or a network error past
    /// the era would fail a transaction that succeeded.
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
}
