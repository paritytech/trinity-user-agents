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
    /// Not in any block read.
    NotFound {
        /// The last block of the unbroken read from the start of the range,
        /// `None` when its first block could not be read. Absence is proven
        /// only up to it.
        read_advanced_to: Option<u64>,
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

/// Rule 5: nothing above decided it, so look for the transaction itself in
/// the blocks after those already searched, up to the earlier of its death
/// and the finalized head.
async fn search_rule(
    tx: &DurableTxEntry,
    view: &dyn PinnedView,
    window_closed: bool,
) -> RuleOutcome {
    let from = tx
        .scanned_to
        .map_or(tx.mortality.birth().number, |scanned| scanned + 1);
    let to = tx.mortality.death().min(view.heads().finalized.number);
    if from > to {
        return absent(tx, None, window_closed);
    }
    let result = view.search(from, to, tx.tx_hash).await;
    searched(tx, result, window_closed)
}

/// The verdict a search result supports.
fn searched(tx: &DurableTxEntry, result: SearchResult, window_closed: bool) -> RuleOutcome {
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
        SearchResult::NotFound { read_advanced_to } => absent(tx, read_advanced_to, window_closed),
        SearchResult::Found { outcome: None, .. } => decided(DurableTxStatus::Pending, None, None),
    }
}

/// Not found in any block searched so far, now read through
/// `read_advanced_to` or the recorded cursor. Absence fails the transaction
/// only once a closed window was read through its death.
fn absent(tx: &DurableTxEntry, read_advanced_to: Option<u64>, window_closed: bool) -> RuleOutcome {
    let scanned_to = read_advanced_to.or(tx.scanned_to);
    let read_to_death = scanned_to.is_some_and(|scanned| scanned >= tx.mortality.death());
    if window_closed && read_to_death {
        return decided(DurableTxStatus::Failure, None, Some(FailureKind::Expired));
    }
    RuleOutcome::Decided(Verdict {
        scanned_to: read_advanced_to,
        ..verdict(DurableTxStatus::Pending, None, None)
    })
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
    RuleOutcome::Decided(verdict(status, success_detected_at, failure))
}

/// A verdict that leaves the recorded search cursor as it is.
fn verdict(
    status: DurableTxStatus,
    success_detected_at: Option<HashAndNumber>,
    failure: Option<FailureKind>,
) -> Verdict {
    Verdict {
        status,
        success_detected_at,
        failure,
        scanned_to: None,
    }
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
            scanned_to: None,
        })
    }

    const INCOMPLETE: SearchResult = SearchResult::NotFound {
        read_advanced_to: None,
    };
    const ABSENT: SearchResult = SearchResult::NotFound {
        read_advanced_to: Some(DEATH),
    };

    /// Not found in any block, every one read through `number`.
    fn read_to(number: u64) -> SearchResult {
        SearchResult::NotFound {
            read_advanced_to: Some(number),
        }
    }

    fn pending_scanned_to(number: u64) -> RuleOutcome {
        RuleOutcome::Decided(Verdict {
            scanned_to: Some(number),
            ..verdict(DurableTxStatus::Pending, None, None)
        })
    }

    fn scanned_to(number: u64) -> DurableTxEntry {
        DurableTxEntry {
            scanned_to: Some(number),
            ..tx(None)
        }
    }

    fn says() -> ScriptedScope {
        ScriptedScope::default()
    }

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

    /// A reorg removed the block the success was recorded at and the domain
    /// sees no effect any more: the transaction is pending again, so its
    /// effects are no longer trusted.
    #[test]
    fn rule_0_a_gone_record_with_nothing_visible_demotes_and_clears() {
        block_on(async {
            let outcome =
                evaluate(tx(Some(block(120))), says(), 130, Some(false), INCOMPLETE).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// The transaction was re-included on the new branch, so its effect is
    /// still visible at the best head.
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

    /// The recorded block's height could not be read. Even a domain that proves
    /// completion waits: the record has to be checked first.
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

    /// The transaction can still be included until its era ends, so absence of
    /// its effect means nothing yet.
    #[test]
    fn rule_3_proven_non_completion_before_the_era_ends_does_not_fail() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_finalized: true,
                ..says()
            };

            let outcome = evaluate(tx(None), scope, BIRTH + 1, None, read_to(BIRTH + 1)).await;

            assert_eq!(outcome, pending_scanned_to(BIRTH + 1));
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

            assert_eq!(outcome, pending_scanned_to(DEATH));
        })
    }

    /// A domain that answers both ways cannot fail a transaction that ran.
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

    /// A transaction with no evidence yet must not cost a body search on every
    /// new head.
    #[test]
    fn rule_4_short_circuits_the_search_while_the_window_is_open() {
        block_on(async {
            let scope = ScriptedScope {
                not_completed_at_best: true,
                ..says()
            };
            let view = FakeView::new(BIRTH + 1, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(None), &scope, &view, None).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
            assert_eq!(view.searches(), 0);
        })
    }

    /// Past the era the search is the only thing left that can decide it.
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

    /// The extrinsic was included and its dispatch failed: inclusion is not
    /// success.
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

    /// The extrinsic is in a block whose events are pruned on this node.
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

    /// The extrinsic never made it into any block of its era, and every block
    /// was read.
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

    /// Some blocks of the era could not be read, so absence proves nothing.
    #[test]
    fn rule_5_a_partially_read_window_stays_pending() {
        block_on(async {
            let outcome = evaluate(tx(None), says(), DEATH + 1, None, INCOMPLETE).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    #[test]
    fn rule_5_searches_from_birth_to_the_earlier_of_death_and_finalized() {
        block_on(async {
            let early = FakeView::new(130, INCOMPLETE);
            let late = FakeView::new(DEATH + 50, INCOMPLETE);

            evaluate_ladder(&tx(None), &says(), &early, None).await;
            evaluate_ladder(&tx(None), &says(), &late, None).await;

            assert_eq!(early.searched_ranges(), vec![(BIRTH, 130)]);
            assert_eq!(late.searched_ranges(), vec![(BIRTH, DEATH)]);
        })
    }

    /// The extrinsic was signed against a best block finality has not reached
    /// yet.
    #[test]
    fn a_birth_above_finalized_stays_pending_without_searching() {
        block_on(async {
            let view = FakeView::new(BIRTH - 1, INCOMPLETE);

            let outcome = evaluate_ladder(&tx(None), &says(), &view, None).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
            assert_eq!(view.searches(), 0);
        })
    }

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

    /// Blocks read on an earlier pass were finalized, so they cannot have
    /// changed: the next pass reads only the blocks finalized since.
    #[test]
    fn rule_5_searches_only_past_the_blocks_already_read() {
        block_on(async {
            let view = FakeView::new(123, INCOMPLETE);

            evaluate_ladder(&scanned_to(120), &says(), &view, None).await;

            assert_eq!(view.searched_ranges(), vec![(121, 123)]);
        })
    }

    #[test]
    fn rule_5_records_how_far_the_search_read() {
        block_on(async {
            let outcome = evaluate(scanned_to(120), says(), 123, None, read_to(123)).await;

            assert_eq!(outcome, pending_scanned_to(123));
        })
    }

    /// The first new block could not be read, so the cursor stays where it
    /// was and the next pass tries that block again.
    #[test]
    fn rule_5_an_unreadable_first_block_leaves_the_cursor() {
        block_on(async {
            let outcome = evaluate(scanned_to(120), says(), 123, None, INCOMPLETE).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
        })
    }

    /// Earlier passes read the window through its death; once it closes there
    /// is nothing left to read and absence is already proven.
    #[test]
    fn rule_5_a_window_read_through_its_death_expires_without_searching() {
        block_on(async {
            let view = FakeView::new(DEATH + 1, INCOMPLETE);

            let outcome = evaluate_ladder(&scanned_to(DEATH), &says(), &view, None).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::Failure, None, Some(FailureKind::Expired))
            );
            assert_eq!(view.searches(), 0);
        })
    }

    /// The last blocks of the era were read on this pass, the earlier ones on
    /// previous passes.
    #[test]
    fn rule_5_absence_read_across_passes_expires() {
        block_on(async {
            let outcome = evaluate(scanned_to(150), says(), DEATH + 1, None, ABSENT).await;

            assert_eq!(
                outcome,
                decided(DurableTxStatus::Failure, None, Some(FailureKind::Expired))
            );
        })
    }

    /// Nothing past the cursor is finalized yet.
    #[test]
    fn rule_5_nothing_newly_finalized_keeps_the_cursor_without_searching() {
        block_on(async {
            let view = FakeView::new(120, INCOMPLETE);

            let outcome = evaluate_ladder(&scanned_to(120), &says(), &view, None).await;

            assert_eq!(outcome, decided(DurableTxStatus::Pending, None, None));
            assert_eq!(view.searches(), 0);
        })
    }
}
