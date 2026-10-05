//! The completion ladder: decides one transaction against one pinned view of
//! its chain. The first rule that matches wins.

use subxt::utils::H256;

use super::model::{DurableTxEntry, DurableTxStatus, FailureKind, HeadKind, Verdict};
use super::oracle::PassScope;
use crate::chain::{DispatchOutcome, HashAndNumber, Heads};

/// One view of a chain, read once for a whole pass.
#[async_trait::async_trait]
pub trait PinnedView: Send + Sync {
    /// The heads the pass decides at.
    fn heads(&self) -> Heads;

    /// Looks for `tx_hash` in the canonical blocks `from..=to`, reading its
    /// dispatch outcome where it is found.
    async fn search(&self, from: u64, to: u64, tx_hash: H256) -> SearchResult;
}

/// Where a transaction was found, if anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchResult {
    /// In `block`, with its outcome, or `None` when the outcome could not be
    /// read: inclusion alone is not success.
    Found {
        /// The block that includes it.
        block: HashAndNumber,
        /// How it dispatched there.
        outcome: Option<DispatchOutcome>,
    },
    /// Not in any block read; absence proves something only when every block
    /// was read.
    NotFound {
        /// Whether every block in the range could be read.
        whole_range_read: bool,
    },
}

/// What the ladder concluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleOutcome {
    /// A verdict, which may restate the current status.
    Decided(Verdict),
    /// A read the transaction depended on failed; it keeps its status.
    Undecided,
}

/// Decides `tx`. `recorded_canonical` says whether the block its success was
/// recorded at is still canonical, `None` when that read failed; the pass
/// resolves it for every recorded transaction at once.
pub async fn evaluate_ladder(
    tx: &DurableTxEntry,
    scope: &dyn PassScope,
    view: &dyn PinnedView,
    recorded_canonical: Option<bool>,
) -> RuleOutcome {
    let heads = view.heads();
    if let Some(recorded) = tx.success_detected_at {
        return recorded_inclusion(tx, recorded, scope, &heads, recorded_canonical);
    }
    // One block past the era's death, as on Android, so a terminal verdict
    // never rests on the death block itself.
    let window_closed = heads.finalized.number > tx.mortality.death();
    if let Some(outcome) = oracle_rules(tx, scope, &heads, window_closed) {
        return outcome;
    }
    search_rule(tx, view, window_closed).await
}

/// Rules 1 to 4: what the domain's oracle proved, if it proved anything.
fn oracle_rules(
    tx: &DurableTxEntry,
    scope: &dyn PassScope,
    heads: &Heads,
    window_closed: bool,
) -> Option<RuleOutcome> {
    if scope.proven_completed(tx, HeadKind::Finalized) {
        return Some(decided(DurableTxStatus::FinalizedSuccess, None, None));
    }
    if scope.proven_completed(tx, HeadKind::Best) {
        return Some(decided(
            DurableTxStatus::PendingSuccess,
            Some(heads.best),
            None,
        ));
    }
    if window_closed && scope.proven_not_completed(tx, HeadKind::Finalized) {
        return Some(decided(
            DurableTxStatus::Failure,
            None,
            Some(FailureKind::Expired),
        ));
    }
    // Past the era the search is the only thing left that can decide it.
    let short_circuit = !window_closed && scope.proven_not_completed(tx, HeadKind::Best);
    short_circuit.then(|| decided(DurableTxStatus::Pending, None, None))
}

/// Rule 5: nothing above decided it, so look for the transaction itself
/// between its birth and the earlier of its death and the finalized head.
async fn search_rule(
    tx: &DurableTxEntry,
    view: &dyn PinnedView,
    window_closed: bool,
) -> RuleOutcome {
    let from = tx.mortality.birth().number;
    let to = tx.mortality.death().min(view.heads().finalized.number);
    if from > to {
        return decided(DurableTxStatus::Pending, None, None);
    }
    searched(view.search(from, to, tx.tx_hash).await, window_closed)
}

/// The verdict a search result supports. Absence fails the transaction only
/// when every block of a closed window was read.
fn searched(result: SearchResult, window_closed: bool) -> RuleOutcome {
    match result {
        SearchResult::Found {
            block,
            outcome: Some(DispatchOutcome::Succeeded),
        } => decided(DurableTxStatus::FinalizedSuccess, Some(block), None),
        SearchResult::Found {
            outcome: Some(DispatchOutcome::Failed),
            ..
        } => decided(
            DurableTxStatus::Failure,
            None,
            Some(FailureKind::DispatchFailed),
        ),
        SearchResult::NotFound {
            whole_range_read: true,
        } if window_closed => decided(DurableTxStatus::Failure, None, Some(FailureKind::Expired)),
        SearchResult::Found { outcome: None, .. } | SearchResult::NotFound { .. } => {
            decided(DurableTxStatus::Pending, None, None)
        }
    }
}

/// Rule 0: the transaction was seen included; check that block is still
/// canonical. The record only exists where completion was already proven.
fn recorded_inclusion(
    tx: &DurableTxEntry,
    recorded: HashAndNumber,
    scope: &dyn PassScope,
    heads: &Heads,
    recorded_canonical: Option<bool>,
) -> RuleOutcome {
    match recorded_canonical {
        None => RuleOutcome::Undecided,
        Some(true) if recorded.number <= heads.finalized.number => {
            decided(DurableTxStatus::FinalizedSuccess, Some(recorded), None)
        }
        Some(true) => decided(DurableTxStatus::PendingSuccess, Some(recorded), None),
        Some(false) if scope.proven_completed(tx, HeadKind::Finalized) => decided(
            DurableTxStatus::FinalizedSuccess,
            Some(heads.finalized),
            None,
        ),
        Some(false) if scope.proven_completed(tx, HeadKind::Best) => {
            decided(DurableTxStatus::PendingSuccess, Some(heads.best), None)
        }
        Some(false) => decided(DurableTxStatus::Pending, None, None),
    }
}

fn decided(
    status: DurableTxStatus,
    success_detected_at: Option<HashAndNumber>,
    failure: Option<FailureKind>,
) -> RuleOutcome {
    RuleOutcome::Decided(Verdict {
        status,
        success_detected_at,
        failure,
    })
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::durable::model::DurableTxId;
    use crate::durable::oracle::{CompletionOracle, LedgerView, Unobservable};
    use crate::durable::testing::{FakeView, ScriptedScope, block, entry};

    const BIRTH: u64 = 100;
    const PERIOD: u64 = 64;
    const DEATH: u64 = BIRTH + PERIOD;

    fn tx(success_detected_at: Option<HashAndNumber>) -> DurableTxEntry {
        DurableTxEntry {
            success_detected_at,
            ..entry(DurableTxId(1), BIRTH, PERIOD)
        }
    }

    async fn evaluate(
        tx: DurableTxEntry,
        scope: ScriptedScope,
        finalized: u64,
        recorded_canonical: Option<bool>,
        search: SearchResult,
    ) -> RuleOutcome {
        evaluate_ladder(
            &tx,
            &scope,
            &FakeView::new(finalized, search),
            recorded_canonical,
        )
        .await
    }

    fn decided(
        status: DurableTxStatus,
        success_detected_at: Option<HashAndNumber>,
        failure: Option<FailureKind>,
    ) -> RuleOutcome {
        RuleOutcome::Decided(Verdict {
            status,
            success_detected_at,
            failure,
        })
    }

    const INCOMPLETE: SearchResult = SearchResult::NotFound {
        whole_range_read: false,
    };
    const ABSENT: SearchResult = SearchResult::NotFound {
        whole_range_read: true,
    };

    fn says() -> ScriptedScope {
        ScriptedScope::default()
    }

    /// Android: `a canonical record at or below the finalized head finalizes`.
    #[test]
    fn rule_0_a_canonical_record_at_or_below_finalized_finalizes() {
        block_on(async {
            let outcome = evaluate(tx(Some(block(120))), says(), 130, Some(true), INCOMPLETE).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::FinalizedSuccess, Some(block(120)), None)
            );
        })
    }

    /// Android: `a canonical record above the finalized head holds at PENDING_SUCCESS`.
    #[test]
    fn rule_0_a_canonical_record_above_finalized_holds_pending_success() {
        block_on(async {
            let outcome = evaluate(tx(Some(block(140))), says(), 130, Some(true), INCOMPLETE).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::PendingSuccess, Some(block(140)), None)
            );
        })
    }

    /// Android: `a record whose block is gone demotes to PENDING when nothing is visible any more`.
    #[test]
    fn rule_0_a_gone_record_with_nothing_visible_demotes_and_clears() {
        block_on(async {
            let outcome =
                evaluate(tx(Some(block(120))), says(), 130, Some(false), INCOMPLETE).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// Android: `a record whose block is gone re-records the best head when completion is still visible there`.
    #[test]
    fn rule_0_a_gone_record_still_completed_at_best_re_records_the_best_head() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_best: true,
                ..says()
            };
            let view = FakeView::new(130, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(Some(block(120))), &scope, &view, Some(false)).await;

            assert_eq!(
                outcome,
                decided(
                    DurableTxStatus::PendingSuccess,
                    Some(view.heads().best),
                    None
                )
            );
        })
    }

    /// Asked before the best head, or a chain that reorgs its head between
    /// passes would keep the transaction above F and never finalize it.
    #[test]
    fn rule_0_a_gone_record_completed_at_finalized_finalizes_there() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_finalized: true,
                completed_at_best: true,
                ..says()
            };
            let view = FakeView::new(130, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(Some(block(120))), &scope, &view, Some(false)).await;

            assert_eq!(
                outcome,
                decided(
                    DurableTxStatus::FinalizedSuccess,
                    Some(view.heads().finalized),
                    None
                )
            );
        })
    }

    /// Android: `an unreadable canonicality check leaves the transaction undecided`.
    #[test]
    fn rule_0_an_unreadable_canonicity_check_is_undecided() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(Some(block(120))), scope, 130, None, INCOMPLETE).await;

            assert_eq!(outcome, RuleOutcome::Undecided);
        })
    }

    /// Android: `completion proven at the finalized head finalizes`.
    /// iOS: `Rule 1 writes no record`.
    #[test]
    fn rule_1_completion_proven_at_finalized_finalizes() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, 130, None, INCOMPLETE).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::FinalizedSuccess, None, None)
            );
        })
    }

    /// Android: `completion proven only at the best head is PENDING_SUCCESS`.
    #[test]
    fn rule_2_completion_proven_only_at_best_is_pending_success() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_best: true,
                ..says()
            };
            let view = FakeView::new(130, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(None), &scope, &view, None).await;

            assert_eq!(
                outcome,
                decided(
                    DurableTxStatus::PendingSuccess,
                    Some(view.heads().best),
                    None
                )
            );
        })
    }

    /// Android: `the finalized head wins over the best head on the same evidence`.
    #[test]
    fn rule_1_wins_over_rule_2_on_the_same_evidence() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_finalized: true,
                completed_at_best: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, 130, None, INCOMPLETE).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::FinalizedSuccess, None, None)
            );
        })
    }

    /// Android: `proven non-completion after mortality fails the transaction`.
    #[test]
    fn rule_3_proven_non_completion_after_the_era_fails() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, DEATH + 1, None, INCOMPLETE).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::Failure, None, Some(FailureKind::Expired))
            );
        })
    }

    /// Android: `proven non-completion before mortality does not fail the transaction`.
    #[test]
    fn rule_3_proven_non_completion_before_the_era_ends_does_not_fail() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, BIRTH + 1, None, ABSENT).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// The window stays open through the death block itself, one block more
    /// than the era strictly allows, as on Android.
    #[test]
    fn rule_3_the_window_is_still_open_at_the_death_block() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, DEATH, None, ABSENT).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// Android: `proven completion beats proven non-completion`.
    #[test]
    fn proven_completion_beats_proven_non_completion() {
        block_on(async {
            let scope = ScriptedScope {
                completed_at_finalized: true,
                not_completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, DEATH + 1, None, INCOMPLETE).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::FinalizedSuccess, None, None)
            );
        })
    }

    /// Android: `proven non-completion at the best head short circuits the search while the window is open`.
    #[test]
    fn rule_4_short_circuits_the_search_while_the_window_is_open() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_best: true,
                ..says()
            };
            let view = FakeView::new(BIRTH + 1, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(None), &scope, &view, None).await;

            assert_eq!(
                (outcome, view.searches()),
                (decided(DurableTxStatus::Pending, None, None), 0)
            );
        })
    }

    /// Android: `the short circuit does not fire once mortality has expired`.
    #[test]
    fn rule_4_does_not_fire_once_the_era_has_ended() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_best: true,
                ..says()
            };
            let view = FakeView::new(DEATH + 1, INCOMPLETE);

            evaluate_ladder(&tx(None), &scope, &view, None).await;

            assert_eq!(view.searches(), 1);
        })
    }

    /// Android: `a successful dispatch found in the window finalizes`.
    #[test]
    fn rule_5_a_successful_dispatch_found_finalizes_at_its_block() {
        block_on(async {
            let found = SearchResult::Found {
                block: block(110),
                outcome: Some(DispatchOutcome::Succeeded),
            };

            let outcome = evaluate(tx(None), says(), 130, None, found).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::FinalizedSuccess, Some(block(110)), None)
            );
        })
    }

    /// Android: `a failed dispatch found in the window fails the transaction`,
    /// `a failed dispatch is reported as a dispatch failure`.
    #[test]
    fn rule_5_a_failed_dispatch_found_fails_as_dispatch_failed() {
        block_on(async {
            let found = SearchResult::Found {
                block: block(110),
                outcome: Some(DispatchOutcome::Failed),
            };

            let outcome = evaluate(tx(None), says(), 130, None, found).await;

            assert_eq!(
                outcome,
                decided(
                    DurableTxStatus::Failure,
                    None,
                    Some(FailureKind::DispatchFailed)
                )
            );
        })
    }

    /// Android: `an unreadable outcome leaves the transaction PENDING`.
    #[test]
    fn rule_5_an_unreadable_outcome_stays_pending() {
        block_on(async {
            let found = SearchResult::Found {
                block: block(110),
                outcome: None,
            };

            let outcome = evaluate(tx(None), says(), DEATH + 1, None, found).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// Android: `a transaction that never ran within its window is reported as expired`,
    /// `absence fails the transaction only once the whole window was read and mortality expired`.
    #[test]
    fn rule_5_absence_over_the_whole_closed_window_expires() {
        block_on(async {
            let outcome = evaluate(tx(None), says(), DEATH + 1, None, ABSENT).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::Failure, None, Some(FailureKind::Expired))
            );
        })
    }

    /// Android: `a partially read window leaves the transaction PENDING`.
    #[test]
    fn rule_5_a_partially_read_window_stays_pending() {
        block_on(async {
            let outcome = evaluate(tx(None), says(), DEATH + 1, None, INCOMPLETE).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// iOS: `the search window runs from the checkpoint to F, never past mortality`.
    #[test]
    fn rule_5_searches_from_birth_to_the_earlier_of_death_and_finalized() {
        block_on(async {
            let early = FakeView::new(130, INCOMPLETE);
            let late = FakeView::new(DEATH + 50, INCOMPLETE);

            evaluate_ladder(&tx(None), &says(), &early, None).await;
            evaluate_ladder(&tx(None), &says(), &late, None).await;

            assert_eq!(
                (early.searched_ranges(), late.searched_ranges()),
                (vec![(BIRTH, 130)], vec![(BIRTH, DEATH)])
            );
        })
    }

    /// iOS: `a checkpoint above F has nothing to search and stays pending`.
    #[test]
    fn a_birth_above_finalized_stays_pending_without_searching() {
        block_on(async {
            let view = FakeView::new(BIRTH - 1, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(None), &says(), &view, None).await;

            assert_eq!(
                (outcome, view.searches()),
                (decided(DurableTxStatus::Pending, None, None), 0)
            );
        })
    }

    /// Android: `a domain with no oracle is still decided by the search`.
    #[test]
    fn an_unobservable_domain_is_still_decided_by_the_search() {
        block_on(async {
            let view = FakeView::new(
                DEATH + 1,
                SearchResult::Found {
                    block: block(110),
                    outcome: Some(DispatchOutcome::Succeeded),
                },
            );
            let scope = Unobservable(H256::zero())
                .open_pass(&[], &LedgerView::new(vec![]), &view.heads())
                .await
                .unwrap();

            let outcome = evaluate_ladder(&tx(None), scope.as_ref(), &view, None).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::FinalizedSuccess, Some(block(110)), None)
            );
        })
    }
}
